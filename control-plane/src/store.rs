//! TaskStore — 控制面任务存储抽象（持久化 seam）
//!
//! 把"任务树 + 状态机"从具体实现里抽出来，控制面只依赖这个 trait：
//! - 内置 `InMemoryTaskStore`（默认，单机/测试用，见 registry.rs）
//! - 将来可加 RedisTaskStore / EtcdTaskStore（同一 trait，运行时按配置切换）
//!
//! 所有方法都是 `&self` + async，内部自行管理并发/持久化；
//! 服务层持 `Arc<dyn TaskStore>`，对后端无感。

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::registry::{TaskEvent, TaskRecord, TaskStatus};

#[async_trait]
pub trait TaskStore: Send + Sync {
    /// 注册新任务；同一 (parent, idempotency_key) 已存在 → 返回既有记录（幂等）。
    async fn register(
        &self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
        idempotency_key: &str,
    ) -> TaskRecord;

    async fn set_status(&self, id: &str, status: TaskStatus) -> Option<TaskRecord>;

    async fn assign_node(&self, id: &str, node: &str);

    async fn cancel(&self, id: &str) -> bool;

    async fn get(&self, id: &str) -> Option<TaskRecord>;

    /// 直接子任务（parent == 给定 id）。
    async fn children_of(&self, parent: &str) -> Vec<TaskRecord>;

    async fn list(&self, parent: Option<&str>) -> Vec<TaskRecord>;

    /// 订阅任务事件（Submitted / StatusChanged / Canceled）。
    async fn subscribe(&self) -> broadcast::Receiver<TaskEvent>;
}

/// 跨节点事件线格式：携带 origin（发布者节点 id）用于去重。
///
/// 分布式 store（Redis / etcd）把本地产生的 `TaskEvent` 包成 `WireEvent` 发到外部通道，
/// 其它节点收到后若 `origin != self` 才转发到本地 `broadcast`，避免回声回路。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WireEvent {
    pub origin: String,
    pub event: TaskEvent,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::registry::InMemoryTaskStore;

    /// 通用 trait 验收：任何 TaskStore 实现都应满足（Redis/Etcd 上线后复用此测试）。
    pub async fn exercise_store<S: TaskStore + ?Sized>(store: &S) {
        let root = store.register("root", None, "spawn", "cpu", "k-root").await;
        let _child = store
            .register("leaf", Some(&root.id), "spawn", "cpu", "k-leaf")
            .await;
        let kids = store.children_of(&root.id).await;
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].parent.as_deref(), Some(root.id.as_str()));

        let reused = store.register("root", None, "spawn", "cpu", "k-root").await;
        assert_eq!(reused.id, root.id, "同一 (parent, idem) 应幂等复用");
    }

    #[tokio::test]
    async fn in_memory_store_satisfies_trait() {
        let store = InMemoryTaskStore::new();
        exercise_store(&store).await;
    }
}
