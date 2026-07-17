# ADR-001: Dalin L v3.0-dev Versioned Types

- **状态**: Accepted
- **日期**: 2026-07-17
- **上下文**: v2-types 分支经历了 CheckContext 重构、parser.rs token import、ty2.rs 恢复等关键变更
- **决策**: 使用 `3.0.0-dev` 版本号，Cargo.toml/Cargo.toml/Cargo.toml 中统一升级

### 技术变更
1. `ty2.rs` 从 8 行恢复至 2172 行 — 七通道类型系统核心完整恢复
2. `parser.rs` 加回 `use crate::token::{Token, TokenType, TokenType::*}` 消除 179 个 E0408
3. 246 个测试全绿，0 Clippy warnings

### 后果
- 版本升级后所有引用 "2.0" 的地方需同步更新
- LSP、VSCode 扩展需重新验证与 ty2 的 API 兼容性
