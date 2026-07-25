//! Dalin L 3.0 — 原生代码生成器
//!
//! 将 DLVM 字节码编译为 LLVM IR → 原生机器码。
//! - LLVM 可用时：`emit_from_bytecode()` 生成可执行文件
//! - LLVM 不可用时：优雅降级，返回错误消息
//!
//! 依赖 `inkwell` crate (LLVM Rust 绑定)，可选 feature-gated。
//!

mod native;

pub use native::emit_from_bytecode;
