//! RedisTaskStore — `TaskStore` 的 Redis 后端（生产级分布式持久化）
//!
//! 数据布局（均存 JSON 字符串，简单且对 schema 演进友好）：
//!   - `dalin:task:{id}`                 → TaskRecord JSON
//!   - `dalin:idem:{parent}/{key}`       → task id（幂等索引）
//!   - `dalin:children:{parent}`         → SET<task id>（直接子任务）
//!   - `dalin:tasks`                     → SET<task id>（全部任务）
//!   - 事件：`PUBLISH dalin:tasks:events` WireEvent JSON（跨节点传播）
//!
//! 跨节点事件：本地写入同时 (a) 发到本地 broadcast（本节点订阅者立即可见）
//! 与 (b) PUBLISH 到 Redis 频道；后台订阅任务收到外部事件后，origin != self 才转发本地
//! broadcast，从而在不丢事件的前提下避免回声。
//!
//! 并发：`ConnectionManager` 廉价克隆（内部 Arc），每个调用克隆一份即可在 `&self`
//! 方法里拿到 `&mut` 连接，无需全局锁。

use async_trait::async_trait;
use futures_util::StreamExt;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tokio::sync::broadcast;

use crate::registry::{TaskEvent, TaskRecord, TaskStatus};
use crate::store::{TaskStore, WireEvent};

const PREFIX: &str = "dalin";
const EVENTS_CHANNEL: &str = "dalin:tasks:events";

pub struct RedisTaskStore {
    conn: ConnectionManager,
    tx: broadcast::Sender<TaskEvent>,
    node_id: String,
}

impl RedisTaskStore {
    /// 连接 Redis 并启动跨节点事件订阅任务。
    ///
    /// `url` 形如 `redis://127.0.0.1:6379`；`node_id` 为本控制面实例标识（事件去重用）。
    pub async fn connect(url: &str, node_id: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let conn = client.get_connection_manager().await?;
        let (tx, _rx) = broadcast::channel(1024);
        let store = Self {
            conn,
            tx: tx.clone(),
            node_id: node_id.to_string(),
        };
        store.spawn_subscriber(url, tx, node_id.to_string()).await?;
        Ok(store)
    }

    /// 廉价克隆连接（内部 Arc），用于 `&self` 方法里拿 `&mut`。
    fn conn(&self) -> ConnectionManager {
        self.conn.clone()
    }

    async fn spawn_subscriber(
        &self,
        url: &str,
        tx: broadcast::Sender<TaskEvent>,
        node_id: String,
    ) -> redis::RedisResult<()> {
        // pub/sub 需独立连接（ConnectionManager 不擅长订阅）。
        // 注意：pub/sub 必须用非 multiplexed 连接，`get_async_connection` 虽被标记
        // deprecated 但仍是唯一支持 `into_pubsub` 的入口，故此处显式放行。
        #[allow(deprecated)]
        let client = redis::Client::open(url)?;
        #[allow(deprecated)]
        let sub_conn = client.get_async_connection().await?;
        let mut pubsub = sub_conn.into_pubsub();
        pubsub.subscribe(EVENTS_CHANNEL).await?;
        tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let payload: Vec<u8> = match msg.get_payload() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if let Ok(we) = serde_json::from_slice::<WireEvent>(&payload) {
                    if we.origin != node_id {
                        let _ = tx.send(we.event);
                    }
                }
            }
        });
        Ok(())
    }

    /// 本地写入后：发本地 broadcast（本节点立即可见）+ PUBLISH 到 Redis（其它节点可见）。
    async fn publish_event(&self, event: TaskEvent) {
        let we = WireEvent {
            origin: self.node_id.clone(),
            event: event.clone(),
        };
        if let Ok(payload) = serde_json::to_vec(&we) {
            let mut conn = self.conn();
            let _: redis::RedisResult<()> = conn.publish(EVENTS_CHANNEL, payload).await;
        }
        let _ = self.tx.send(event);
    }

    async fn get_record(&self, id: &str) -> Option<TaskRecord> {
        let mut conn = self.conn();
        let key = format!("{}:task:{}", PREFIX, id);
        let v: Option<String> = conn.get(key).await.ok().flatten();
        v.and_then(|s| serde_json::from_str(&s).ok())
    }
}

#[async_trait]
impl TaskStore for RedisTaskStore {
    async fn register(
        &self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
        idempotency_key: &str,
    ) -> TaskRecord {
        let mut conn = self.conn();
        let idem_key = format!("{}/{}", parent.unwrap_or(""), idempotency_key);
        let idem_redis = format!("{}:idem:{}", PREFIX, idem_key);
        if let Some(existing_id) = conn.get::<_, Option<String>>(&idem_redis).await.ok().flatten() {
            if let Some(rec) = self.get_record(&existing_id).await {
                return rec; // 幂等复用
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let rec = TaskRecord {
            id: id.clone(),
            name: name.to_string(),
            parent: parent.map(str::to_string),
            effect: effect.to_string(),
            capability: capability.to_string(),
            idempotency_key: idempotency_key.to_string(),
            status: TaskStatus::Queued,
            node: None,
            submitted_at: now_ms(),
        };
        let task_key = format!("{}:task:{}", PREFIX, id);
        let _: redis::RedisResult<()> = conn
            .set(&task_key, serde_json::to_string(&rec).unwrap())
            .await;
        let _: redis::RedisResult<()> = conn.set(&idem_redis, &id).await;
        if let Some(p) = parent {
            let _: redis::RedisResult<()> = conn
                .sadd(format!("{}:children:{}", PREFIX, p), &id)
                .await;
        }
        let _: redis::RedisResult<()> = conn.sadd(format!("{}:tasks", PREFIX), &id).await;
        self.publish_event(TaskEvent::Submitted(rec.clone())).await;
        rec
    }

    async fn set_status(&self, id: &str, status: TaskStatus) -> Option<TaskRecord> {
        let mut conn = self.conn();
        let rec = self.get_record(id).await?;
        let updated = TaskRecord { status, ..rec };
        let task_key = format!("{}:task:{}", PREFIX, id);
        let _: redis::RedisResult<()> = conn
            .set(&task_key, serde_json::to_string(&updated).unwrap())
            .await;
        self.publish_event(TaskEvent::StatusChanged(updated.clone()))
            .await;
        Some(updated)
    }

    async fn assign_node(&self, id: &str, node: &str) {
        let mut conn = self.conn();
        if let Some(mut rec) = self.get_record(id).await {
            rec.node = Some(node.to_string());
            let task_key = format!("{}:task:{}", PREFIX, id);
            let _: redis::RedisResult<()> = conn
                .set(&task_key, serde_json::to_string(&rec).unwrap())
                .await;
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        let mut conn = self.conn();
        match self.get_record(id).await {
            Some(mut rec) => {
                rec.status = TaskStatus::Canceled;
                let task_key = format!("{}:task:{}", PREFIX, id);
                let _: redis::RedisResult<()> = conn
                    .set(&task_key, serde_json::to_string(&rec).unwrap())
                    .await;
                self.publish_event(TaskEvent::Canceled(id.to_string())).await;
                true
            }
            None => false,
        }
    }

    async fn get(&self, id: &str) -> Option<TaskRecord> {
        self.get_record(id).await
    }

    async fn children_of(&self, parent: &str) -> Vec<TaskRecord> {
        let mut conn = self.conn();
        let key = format!("{}:children:{}", PREFIX, parent);
        let ids: Vec<String> = conn.smembers(key).await.unwrap_or_default();
        self.fetch_many(&ids).await
    }

    async fn list(&self, parent: Option<&str>) -> Vec<TaskRecord> {
        match parent {
            Some(p) => self.children_of(p).await,
            None => {
                let mut conn = self.conn();
                let ids: Vec<String> = conn
                    .smembers(format!("{}:tasks", PREFIX))
                    .await
                    .unwrap_or_default();
                self.fetch_many(&ids).await
            }
        }
    }

    async fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }
}

impl RedisTaskStore {
    async fn fetch_many(&self, ids: &[String]) -> Vec<TaskRecord> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(rec) = self.get_record(id).await {
                out.push(rec);
            }
        }
        out
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{TaskEvent, TaskStatus};
    use crate::store::tests::exercise_store;
    use tokio::sync::broadcast;

    fn redis_url() -> String {
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into())
    }

    /// 在超时内等待订阅者收到满足谓词的事件（broadcast 可能 lag，超时即失败）。
    async fn wait_for_event<F>(rx: &mut broadcast::Receiver<TaskEvent>, mut pred: F, ms: u64) -> bool
    where
        F: FnMut(TaskEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Ok(ev)) => {
                    if pred(ev) {
                        return true;
                    } // 收到但不匹配，继续等
                }
                _ => return false, // 超时 / 通道关闭 / lagged
            }
        }
    }

    // 门控：无 Redis 服务时跳过（cargo test 默认不跑）。
    // 运行：先 `docker compose up -d`，再 `cargo test -p control-plane -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn redis_store_satisfies_trait() {
        let store = RedisTaskStore::connect(&redis_url(), "node-redis-test")
            .await
            .expect("connect redis (set REDIS_URL or run `docker compose up -d`)");
        exercise_store(&store).await;
    }

    // 跨节点事件桥接：node-a 写入，node-b 经 Redis 频道收事件（验证分布式事件传播）
    #[tokio::test]
    #[ignore]
    async fn redis_cross_node_event_bridge() {
        let a = RedisTaskStore::connect(&redis_url(), "node-a")
            .await
            .expect("connect redis a");
        let b = RedisTaskStore::connect(&redis_url(), "node-b")
            .await
            .expect("connect redis b");
        // 让 b 的订阅任务完成频道 SUBSCRIBE，避免竞态丢首条事件
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let mut rx = b.subscribe().await;

        let rec = a.register("t", None, "spawn", "cpu", "k-bridge").await;
        assert!(
            wait_for_event(
                &mut rx,
                |e| matches!(e, TaskEvent::Submitted(r) if r.id.as_str() == rec.id.as_str()),
                5000
            )
            .await,
            "node-b 应经 Redis 频道收到 node-a 的提交事件"
        );

        let _ = a.set_status(&rec.id, TaskStatus::Succeeded).await;
        assert!(
            wait_for_event(
                &mut rx,
                |e| {
                    matches!(e, TaskEvent::StatusChanged(r) if r.id.as_str() == rec.id.as_str()
                        && r.status == TaskStatus::Succeeded)
                },
                5000
            )
            .await,
            "node-b 应收到状态变更事件"
        );
    }
}
