//! EtcdTaskStore — `TaskStore` 的 etcd 后端（强一致 / 多节点）
//!
//! 数据布局（值均为 JSON 或空标记）：
//!   - `dalin/task/{id}`            → TaskRecord JSON
//!   - `dalin/idem/{parent}/{key}`  → task id（幂等索引）
//!   - `dalin/children/{parent}/{id}` → 空标记（直接子任务，前缀 range 列出）
//!   - `dalin/tasks/{id}`           → 空标记（全部任务，前缀 range 列出）
//!   - 事件：`dalin/events/{uuid}`  → WireEvent JSON（watch 前缀传播）
//!
//! 跨节点事件：etcd 无原生 pub/sub，改用 `watch dalin/events/` 前缀；本地写入同时
//! (a) 发本地 broadcast 与 (b) PUT 一个事件键，watch 任务收到外部事件后 origin != self
//! 才转发本地 broadcast，避免回声。
//!
//! 并发：`etcd_client::Client` 克隆成本低（内部 Arc），每个调用克隆一份即可在 `&self`
//! 方法里拿到 `&mut`，包括长期的 watch 任务持有独立客户端。

use async_trait::async_trait;
use etcd_client::{Client, GetOptions, WatchOptions};
use futures_util::StreamExt;
use tokio::sync::broadcast;

use crate::registry::{TaskEvent, TaskRecord, TaskStatus};
use crate::store::{TaskStore, WireEvent};

const PREFIX: &str = "dalin";
const EVENTS_PREFIX: &str = "dalin/events/";

pub struct EtcdTaskStore {
    client: Client,
    tx: broadcast::Sender<TaskEvent>,
    node_id: String,
}

impl EtcdTaskStore {
    /// 连接 etcd 集群并启动事件 watch 任务。
    ///
    /// `seeds` 为 `host:port` 列表（可多个，集群故障转移）；`node_id` 为本实例标识。
    pub async fn connect(seeds: Vec<String>, node_id: &str) -> Result<Self, etcd_client::Error> {
        let client = Client::connect(seeds, None).await?;
        let (tx, _rx) = broadcast::channel(1024);
        let store = Self {
            client: client.clone(),
            tx: tx.clone(),
            node_id: node_id.to_string(),
        };
        store.spawn_watcher(client, tx, node_id.to_string());
        Ok(store)
    }

    fn spawn_watcher(&self, mut client: Client, tx: broadcast::Sender<TaskEvent>, node_id: String) {
        tokio::spawn(async move {
            let watch = match client
                .watch(EVENTS_PREFIX, Some(WatchOptions::new().with_prefix()))
                .await
            {
                Ok(w) => w,
                Err(_) => return,
            };
            let (_watcher, mut stream) = watch; // watcher 持活至任务结束
            while let Some(resp) = stream.next().await {
                let resp = match resp {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                for ev in resp.events() {
                    if let Some(kv) = ev.kv()
                        && let Ok(we) = serde_json::from_slice::<WireEvent>(kv.value())
                            && we.origin != node_id {
                                let _ = tx.send(we.event);
                            }
                }
            }
        });
    }

    async fn publish_event(&self, event: TaskEvent) {
        let we = WireEvent {
            origin: self.node_id.clone(),
            event: event.clone(),
        };
        if let Ok(payload) = serde_json::to_vec(&we) {
            let mut client = self.client.clone();
            let key = format!("{}{}", EVENTS_PREFIX, uuid::Uuid::new_v4());
            let _ = client.put(key, payload, None).await;
        }
        let _ = self.tx.send(event);
    }

    async fn get_record(&self, id: &str) -> Option<TaskRecord> {
        let mut client = self.client.clone();
        let key = format!("{}/task/{}", PREFIX, id);
        let resp = client.get(key, None).await.ok()?;
        let kv = resp.kvs().first()?;
        serde_json::from_slice(kv.value()).ok()
    }
}

#[async_trait]
impl TaskStore for EtcdTaskStore {
    async fn register(
        &self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
        idempotency_key: &str,
    ) -> TaskRecord {
        let mut client = self.client.clone();
        let idem_key = format!("{}/{}", parent.unwrap_or(""), idempotency_key);
        let idem_etcd = format!("{}/idem/{}", PREFIX, idem_key);
        if let Ok(resp) = client.get(idem_etcd.as_str(), None).await
            && let Some(kv) = resp.kvs().first() {
                let existing_id = String::from_utf8_lossy(kv.value()).to_string();
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
        let task_key = format!("{}/task/{}", PREFIX, id);
        let _ = client
            .put(task_key.into_bytes(), serde_json::to_vec(&rec).unwrap(), None)
            .await;
        let _ = client
            .put(idem_etcd.into_bytes(), id.as_bytes().to_vec(), None)
            .await;
        if let Some(p) = parent {
            let _ = client
                .put(
                    format!("{}/children/{}/{}", PREFIX, p, id).into_bytes(),
                    Vec::new(),
                    None,
                )
                .await;
        }
        let _ = client
            .put(format!("{}/tasks/{}", PREFIX, id).into_bytes(), Vec::new(), None)
            .await;
        self.publish_event(TaskEvent::Submitted(rec.clone())).await;
        rec
    }

    async fn set_status(&self, id: &str, status: TaskStatus) -> Option<TaskRecord> {
        let mut client = self.client.clone();
        let rec = self.get_record(id).await?;
        let updated = TaskRecord { status, ..rec };
        let task_key = format!("{}/task/{}", PREFIX, id);
        let _ = client
            .put(task_key.as_str(), serde_json::to_vec(&updated).unwrap(), None)
            .await;
        self.publish_event(TaskEvent::StatusChanged(updated.clone()))
            .await;
        Some(updated)
    }

    async fn assign_node(&self, id: &str, node: &str) {
        let mut client = self.client.clone();
        if let Some(mut rec) = self.get_record(id).await {
            rec.node = Some(node.to_string());
            let task_key = format!("{}/task/{}", PREFIX, id);
            let _ = client
                .put(task_key, serde_json::to_vec(&rec).unwrap(), None)
                .await;
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        let mut client = self.client.clone();
        match self.get_record(id).await {
            Some(mut rec) => {
                rec.status = TaskStatus::Canceled;
            let task_key = format!("{}/task/{}", PREFIX, id);
            let _ = client
                .put(task_key, serde_json::to_vec(&rec).unwrap(), None)
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
        let mut client = self.client.clone();
        let prefix = format!("{}/children/{}/", PREFIX, parent);
        let resp = client
            .get(prefix, Some(GetOptions::new().with_prefix()))
            .await
            .ok();
        let ids = extract_ids(resp);
        self.fetch_many(&ids).await
    }

    async fn list(&self, parent: Option<&str>) -> Vec<TaskRecord> {
        match parent {
            Some(p) => self.children_of(p).await,
            None => {
                let mut client = self.client.clone();
                let prefix = format!("{}/tasks/", PREFIX);
                let resp = client
                    .get(prefix, Some(GetOptions::new().with_prefix()))
                    .await
                    .ok();
                let ids = extract_ids(resp);
                self.fetch_many(&ids).await
            }
        }
    }

    async fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }
}

impl EtcdTaskStore {
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

/// 从前缀 range 的 key 里抽出末段 task id。
fn extract_ids(resp: Option<etcd_client::GetResponse>) -> Vec<String> {
    resp.map(|r| {
        r.kvs()
            .iter()
            .filter_map(|kv| {
                let s = String::from_utf8_lossy(kv.key()).to_string();
                s.rsplit('/').next().map(str::to_string)
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
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

    fn etcd_seeds() -> Vec<String> {
        vec![std::env::var("ETCD_URL").unwrap_or_else(|_| "127.0.0.1:2379".into())]
    }

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
                _ => return false,
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn etcd_store_satisfies_trait() {
        let store = EtcdTaskStore::connect(etcd_seeds(), "node-etcd-test")
            .await
            .expect("connect etcd (set ETCD_URL or run `docker compose up -d`)");
        exercise_store(&store).await;
    }

    // 跨节点事件桥接：etcd 无原生 pub/sub，用 watch 前缀模拟；node-a 写入 node-b 应收到
    #[tokio::test]
    #[ignore]
    async fn etcd_cross_node_event_bridge() {
        let a = EtcdTaskStore::connect(etcd_seeds(), "node-a")
            .await
            .expect("connect etcd a");
        let b = EtcdTaskStore::connect(etcd_seeds(), "node-b")
            .await
            .expect("connect etcd b");
        // 让 b 的 watch 任务建立监听，避免竞态丢首条事件
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let mut rx = b.subscribe().await;

        let rec = a.register("t", None, "spawn", "cpu", "k-bridge-etcd").await;
        assert!(
            wait_for_event(
                &mut rx,
                |e| matches!(e, TaskEvent::Submitted(r) if r.id.as_str() == rec.id.as_str()),
                5000
            )
            .await,
            "node-b 应经 etcd watch 收到 node-a 的提交事件"
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
