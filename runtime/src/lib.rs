//! Dalin L 3.0 — 运行时 crate
//! 树遍历解释器 + Agent-Native 并发原语（spawn / channel / await）
//! 复用 dalin-compiler 的 AST / TaskSpec。并发调度由 `scheduler` 模块负责——
//! M:N 工作窃取线程池，将 M 个协程复用到 N 个 OS 线程（取代 1:1 内核线程）。
pub mod bridge;
pub mod cognitive;
pub mod env;
pub mod gc;
pub mod interpreter;
pub mod profiler;
pub mod scheduler;
