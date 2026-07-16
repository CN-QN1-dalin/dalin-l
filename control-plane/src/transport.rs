//! 事件总线：把任务事件发布到外部（NATS 多节点）或内存（测试/单机）。
//!
//! `EventBus` 用 `async_trait` 以便以 `Arc<dyn EventBus>` 形式注入服务，
//! 生产用 `NatsEventBus`（连真实 NATS 集群），测试/单机用 `InMemoryEventBus`。
//!
//! 集群增强（`NatsEventBus`）：
//!   - 接受逗号分隔的多 seed 地址，客户端自动在集群内故障转移。
//!   - 设置客户端名（`dalin-cpd`）便于运维识别。
//!   - 指数退避重连（100ms..8s），断网后自愈，不丢连接身份。
//!   - 连不上 → 调用方降级到 `InMemoryEventBus`（单机也能跑通）。

use async_nats::ConnectOptions;
use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::registry::TaskEvent;

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: &TaskEvent);
}

/// 内存总线：单机 / 测试用。内部用 broadcast，可被 WatchTasks 之外的订阅者观察。
#[derive(Clone)]
pub struct InMemoryEventBus {
    tx: broadcast::Sender<TaskEvent>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, event: &TaskEvent) {
        let _ = self.tx.send(event.clone());
    }
}

/// NATS 总线：生产多节点事件传播。subject = `dalin.tasks`。
///
/// 事件以 JSON 序列化发布到 `dalin.tasks`；集群内任意订阅者（worker / 其它控制面副本）
/// 均可接收，实现跨节点传播。生产环境可在 NATS 侧开启 JetStream 以获得持久化与重投。
pub struct NatsEventBus {
    client: async_nats::Client,
    subject: String,
}

impl NatsEventBus {
    /// 连接 NATS 集群。
    ///
    /// `urls` 可为单个地址或逗号分隔的多个 seed（如 `nats://a:4222,nats://b:4222`），
    /// 客户端会在 seed 列表间故障转移。`cpd` 连不上时应降级到 `InMemoryEventBus`。
    pub async fn connect(urls: &str) -> Result<Self, async_nats::Error> {
        let opts = ConnectOptions::new()
            .name("dalin-cpd")
            .reconnect_delay_callback(|attempt: usize| {
                // 指数退避（100ms·2^attempt，夹在 100ms..8s），断网自愈不暴力重连
                let ms = (100u64.checked_mul(1u64 << attempt.min(6)).unwrap_or(8000)).min(8000);
                std::time::Duration::from_millis(ms)
            });
        let client = opts.connect(urls.to_string()).await?;
        Ok(Self {
            client,
            subject: "dalin.tasks".to_string(),
        })
    }

    /// 事件 subject（便于测试 / 运维核对）。
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

#[async_trait]
impl EventBus for NatsEventBus {
    async fn publish(&self, event: &TaskEvent) {
        if let Ok(payload) = serde_json::to_vec(event) {
            // fire-and-forget；若需 At-Least-Once，可在 NATS 侧启用 JetStream 并 await publish ack
            let _ = self.client.publish(self.subject.clone(), payload.into()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{TaskRecord, TaskStatus};

    fn sample() -> TaskEvent {
        TaskEvent::Submitted(TaskRecord {
            id: "t1".into(),
            name: "w".into(),
            parent: None,
            effect: "spawn".into(),
            capability: "cpu".into(),
            idempotency_key: "k".into(),
            status: TaskStatus::Queued,
            node: None,
            submitted_at: 0,
        })
    }

    #[tokio::test]
    async fn in_memory_bus_delivers() {
        let bus = InMemoryEventBus::new();
        let mut rx = bus.subscribe();
        bus.publish(&sample()).await;
        // 等广播到达
        let ev = rx.recv().await.unwrap();
        match ev {
            TaskEvent::Submitted(rec) => assert_eq!(rec.id, "t1"),
            _ => panic!("unexpected event"),
        }
    }
}
