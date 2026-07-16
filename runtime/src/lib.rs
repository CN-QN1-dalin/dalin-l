/// Dalin L 2.0 — 运行时 crate
///
/// 树遍历解释器 + Agent-Native 并发原语（spawn / channel / await），
/// 复用 dalin-compiler 的 AST / TaskSpec。并发侧表（task_tree / task_results /
/// channel_registry）跨 OS 线程共享，是分布式控制面任务树的本地缩影。
pub mod env;
pub mod interpreter;
