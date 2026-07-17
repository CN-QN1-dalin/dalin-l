//! InMemoryTaskStore — TaskStore 的默认实现（单机 / 测试）
//!
//! 复用 runtime 的 parent 指针 + 唯一 id 思路，扩展为带状态、node、事件广播的注册表。
//! 内部用 `tokio::sync::broadcast` 把任务变更推给 WatchTasks 订阅者（无需 NATS 即可服务）。
//! 通过 `TaskStore` trait 暴露，控制面可零改动切换到 Redis/Etcd 后端。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::store::TaskStore;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus {
    Queued,
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub effect: String,
    pub capability: String,
    pub idempotency_key: String,
    pub status: TaskStatus,
    pub node: Option<String>,
    pub submitted_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TaskEvent {
    Submitted(TaskRecord),
    StatusChanged(TaskRecord),
    Canceled(String),
}

struct Inner {
    tasks: HashMap<String, TaskRecord>,
    /// idempotency_key(+parent) → id，用于幂等去重
    idem: HashMap<String, String>,
    events: broadcast::Sender<TaskEvent>,
}

#[derive(Clone)]
pub struct InMemoryTaskStore {
    inner: Arc<Mutex<Inner>>,
}

impl Default for InMemoryTaskStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTaskStore {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            inner: Arc::new(Mutex::new(Inner {
                tasks: HashMap::new(),
                idem: HashMap::new(),
                events: tx,
            })),
        }
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn register(
        &self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
        idempotency_key: &str,
    ) -> TaskRecord {
        let mut g = self.inner.lock().await;
        let idem_key = format!("{}/{}", parent.unwrap_or(""), idempotency_key);
        if let Some(id) = g.idem.get(&idem_key)
            && let Some(existing) = g.tasks.get(id) {
                return existing.clone();
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
            submitted_at: Self::now(),
        };
        g.idem.insert(idem_key, id.clone());
        g.tasks.insert(id.clone(), rec.clone());
        let _ = g.events.send(TaskEvent::Submitted(rec.clone()));
        rec
    }

    async fn set_status(&self, id: &str, status: TaskStatus) -> Option<TaskRecord> {
        let mut g = self.inner.lock().await;
        let rec = g.tasks.get_mut(id)?;
        rec.status = status;
        let updated = rec.clone();
        let _ = g.events.send(TaskEvent::StatusChanged(updated.clone()));
        Some(updated)
    }

    async fn assign_node(&self, id: &str, node: &str) {
        let mut g = self.inner.lock().await;
        if let Some(rec) = g.tasks.get_mut(id) {
            rec.node = Some(node.to_string());
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        let mut g = self.inner.lock().await;
        match g.tasks.get_mut(id) {
            Some(r) => {
                r.status = TaskStatus::Canceled;
                let _ = g.events.send(TaskEvent::Canceled(id.to_string()));
                true
            }
            None => false,
        }
    }

    async fn get(&self, id: &str) -> Option<TaskRecord> {
        let g = self.inner.lock().await;
        g.tasks.get(id).cloned()
    }

    async fn children_of(&self, parent: &str) -> Vec<TaskRecord> {
        let g = self.inner.lock().await;
        g.tasks
            .values()
            .filter(|t| t.parent.as_deref() == Some(parent))
            .cloned()
            .collect()
    }

    async fn list(&self, parent: Option<&str>) -> Vec<TaskRecord> {
        let g = self.inner.lock().await;
        match parent {
            Some(p) => g
                .tasks
                .values()
                .filter(|t| t.parent.as_deref() == Some(p))
                .cloned()
                .collect(),
            None => g.tasks.values().cloned().collect(),
        }
    }

    async fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        let g = self.inner.lock().await;
        g.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_links_parent_and_lists_children() {
        let store = InMemoryTaskStore::new();
        let root = store.register("root", None, "spawn", "cpu", "k-root").await;
        let _child = store
            .register("leaf", Some(&root.id), "spawn", "cpu", "k-leaf")
            .await;
        let kids = store.children_of(&root.id).await;
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].parent.as_deref(), Some(root.id.as_str()));
    }

    #[tokio::test]
    async fn idempotency_reuses_existing() {
        let store = InMemoryTaskStore::new();
        let a = store.register("w", None, "io", "cpu", "same-key").await;
        let b = store.register("w", None, "io", "cpu", "same-key").await;
        assert_eq!(a.id, b.id);
    }

    #[tokio::test]
    async fn status_transitions_and_cancel() {
        let store = InMemoryTaskStore::new();
        let t = store.register("w", None, "io", "cpu", "k").await;
        store.set_status(&t.id, TaskStatus::Running).await;
        store.cancel(&t.id).await;
        let rec = store.get(&t.id).await.unwrap();
        assert_eq!(rec.status, TaskStatus::Canceled);

        let mut rx = store.subscribe().await;
        // 触发一次取消事件并确认广播到达
        store.cancel(&t.id).await;
        let mut seen_cancel = false;
        while let Ok(ev) = rx.try_recv() {
            if let TaskEvent::Canceled(id) = ev {
                assert_eq!(id, t.id);
                seen_cancel = true;
            }
        }
        assert!(seen_cancel);
    }
}
