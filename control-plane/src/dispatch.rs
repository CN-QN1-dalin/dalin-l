//! Dispatch Broker — 按 capability 分 topic 派发任务
//!
//! 每个能力（cpu/gpu/sfa/net）对应一个 NATS subject：
//!   `dalin.task.<capability>`
//!
//! 调度器完成 capability placement 后，DispatchBroker 将 TaskSpec
//! 以 JSON 形式发布到对应 subject。Worker 节点订阅其能力对应的 subject，
//! 消费任务并执行。
//!
//! 架构：
//! ```text
//! Scheduler → place(task) → Placement(node_id, cap)
//!                ↓
//!         DispatchBroker.publish(task_spec, cap)
//!                ↓
//!         NATS: dalin.task.cpu / .gpu / .sfa / .net
//!                ↓
//!         Worker (订阅 subject) → 执行 → 报告结果
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tonic::async_trait;

/// 派发到 Worker 的任务消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchTask {
    pub task_id: String,
    pub fn_name: String,
    pub effect: String,
    pub capability: String,
    pub parent: Option<String>,
    pub args_json: Option<String>,
}

/// 任务结果（Worker → 控制面回传）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResult {
    pub task_id: String,
    pub ok: bool,
    pub output_json: Option<String>,
    pub error: Option<String>,
}

/// 能力 → NATS subject 映射
pub fn capability_subject(cap: &str) -> String {
    format!("dalin.task.{}", cap)
}

/// 结果回传 subject
pub fn result_subject() -> &'static str {
    "dalin.task.result"
}

/// 派发总线 trait（可 NATS / 内存实现）
#[async_trait]
pub trait DispatchBroker: Send + Sync {
    /// 将任务派发到对应能力的 topic。
    async fn dispatch(&self, task: &DispatchTask) -> Result<(), String>;
    /// 接收任务执行结果。
    async fn report_result(&self, result: &DispatchResult) -> Result<(), String>;
}

/// NATS 派发总线实现
pub struct NatsDispatchBroker {
    nc: Arc<async_nats::Client>,
}

impl NatsDispatchBroker {
    pub fn new(nc: Arc<async_nats::Client>) -> Self {
        Self { nc }
    }
}

#[async_trait]
impl DispatchBroker for NatsDispatchBroker {
    async fn dispatch(&self, task: &DispatchTask) -> Result<(), String> {
        let subject = capability_subject(&task.capability);
        let payload = serde_json::to_vec(task).map_err(|e| format!("serialize: {e}"))?;
        self.nc
            .publish(subject, payload.into())
            .await
            .map_err(|e| format!("nats publish: {e}"))
    }

    async fn report_result(&self, result: &DispatchResult) -> Result<(), String> {
        let payload = serde_json::to_vec(result).map_err(|e| format!("serialize: {e}"))?;
        self.nc
            .publish(result_subject().to_string(), payload.into())
            .await
            .map_err(|e| format!("nats publish: {e}"))
    }
}

/// 内存派发总线（测试 / 单机模式）
pub struct InMemoryDispatchBroker {
    /// dispatch → Vec of `DispatchTask` entries
    pub history: std::sync::Mutex<Vec<DispatchTask>>,
}

impl Default for InMemoryDispatchBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDispatchBroker {
    pub fn new() -> Self {
        Self {
            history: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DispatchBroker for InMemoryDispatchBroker {
    async fn dispatch(&self, task: &DispatchTask) -> Result<(), String> {
        self.history.lock().unwrap().push(task.clone());
        Ok(())
    }

    async fn report_result(&self, _result: &DispatchResult) -> Result<(), String> {
        Ok(())
    }
}

// ═══════════════════════════════
//  将 DispatchBroker 接入 submit_task 流程
// ═══════════════════════════════

/// 在 submit_task 流程中，完成 placement 后调用此函数。
pub fn build_dispatch_task(
    task_id: &str,
    fn_name: &str,
    effect: &str,
    capability: &str,
    parent: Option<String>,
) -> DispatchTask {
    DispatchTask {
        task_id: task_id.to_string(),
        fn_name: fn_name.to_string(),
        effect: effect.to_string(),
        capability: capability.to_string(),
        parent,
        args_json: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_mapping() {
        assert_eq!(capability_subject("cpu"), "dalin.task.cpu");
        assert_eq!(capability_subject("gpu"), "dalin.task.gpu");
        assert_eq!(capability_subject("sfa"), "dalin.task.sfa");
        assert_eq!(capability_subject("net"), "dalin.task.net");
    }

    #[tokio::test]
    async fn test_in_memory_broker() {
        let broker = InMemoryDispatchBroker::new();
        let task = DispatchTask {
            task_id: "t1".into(),
            fn_name: "worker".into(),
            effect: "spawn".into(),
            capability: "cpu".into(),
            parent: None,
            args_json: None,
        };
        broker.dispatch(&task).await.unwrap();
        assert_eq!(broker.history.lock().unwrap().len(), 1);
        assert_eq!(broker.history.lock().unwrap()[0].task_id, "t1");
    }
}
