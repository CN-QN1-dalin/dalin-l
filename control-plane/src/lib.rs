#![allow(clippy::all)]
//! Dalin L 3.0 — 分布式控制面（Phase 2 脚手架）
//!
//! 复用 dalin-compiler 的 TaskSpec，把三通道类型契约落到真实调度：
//! - `scheduler`: 能力格放置（Cpu ≤ Gpu ≤ Sfa ≤ Net）+ 负载均衡
//! - `registry`: 任务树 + 状态机（runtime task_tree 的跨节点版）+ 事件广播
//! - `transport`: 事件总线（NATS 生产实现 + InMemory 测试实现）
//! - `server`/`client`: gRPC 控制面服务与客户端
//! - `k8s`: K8s Operator 控制器（DalinTask CRD → Deployment）
//!
//! gRPC 代码由 build.rs 从 proto/control.proto 生成（package = dalin_control）。

pub mod agent_registry;
pub mod dispatch;
pub mod effect_monitor;
pub mod scheduler;
pub mod registry;
pub mod store;
pub mod store_redis;
pub mod store_etcd;
pub mod store_factory;
pub mod store_postgres;
pub mod transport;
pub mod convert;
pub mod server;
pub mod client;
pub mod k8s;
pub mod tests;

tonic::include_proto!("dalin_control");
