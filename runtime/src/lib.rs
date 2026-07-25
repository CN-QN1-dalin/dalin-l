//! Dalin L 3.0 — 运行时 crate
//!
//! 树遍历解释器 + Agent-Native 并发原语（spawn / channel / await），
//! 复用 `dalin-compiler` 的 AST / `TaskSpec`。
//!
//! ## 架构概述
//!
//! - **env** — 运行时环境（作用域链、变量绑定）
//! - **interpreter** — 树遍历解释器核心（递归下降执行 AST 节点）
//! - **profiler** — 性能剖析器（跟踪函数调用耗时）
//!
//! `并发侧表（task_tree` / `task_results` / `channel_registry）跨` OS 线程共享，
//! 是分布式控制面任务树的本地缩影。
//!
//! ## 七通道运行模型
//!
//! 每个运行时实例可被约束为一个或多个通道：
//! - `@cpu` — 计算密集型任务
//! - `@io` — IO 型任务
//! - `@net` — 网络通信任务
//! - `@perceive` — 感知/认知循环
//!
//! ## 使用示例
//!
//! ```ignore
//! use dalin_runtime::interpreter::Interpreter;
//!
//! let mut interp = Interpreter::new();
//! let result = interp.eval("fn add(a, b) { return a + b }");
//! ```

pub mod env;
pub mod interpreter;
pub mod profiler;
