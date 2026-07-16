# ADR-016: LSP 协议 — VSCode/IDEA 语法高亮+代码补全

## Status
Accepted

## Context
Dalin L 目前的唯一开发入口是 `dalib build/run/check` + REPL。没有 IDE 支持意味着：
1. 用户无法在 VSCode/IntelliJ 中获得语法高亮、智能提示、错误诊断
2. 社区协作门槛极高——开发者不知道语言怎么写
3. LSP 是事实标准，VSCode/VS/JetBrains 全部通过 LSP 提供语言服务

## Decision
使用 **Rust 原生 LSP 协议栈**，而非 JSON-RPC 手工实现：
- **协议版本**：LSP 3.17（稳定版，覆盖所有核心能力）
- **传输方式**：stdio（VSCode 内置支持，无需进程管理）
- **架构分层**：

```
┌──────────────────────────────────────────────┐
│              LSP Protocol Layer               │
│  (jsonrpc2 + lsp-types crate)                │
├──────────────────────────────────────────────┤
│              Language Server                  │
│  - Request Router                              │
│  - Capability Negotiation                      │
├──────────────────────────────────────────────┤
│           Compiler Integration Layer          │
│  - DocumentManager (in-memory source)          │
│  - DiagnosticsBuilder                          │
│  - CompletionEngine (tokens + AST + type env)  │
│  - HoverProvider                               │
│  - DefinitionFinder                            │
├──────────────────────────────────────────────┤
│            Dalin Compiler Core                │
│  (lexer, parser, ty2, ast, module)             │
└──────────────────────────────────────────────┘
```

- **核心 crate 依赖**：
  - `lsp-types` = LSP 3.17 JSON schema 定义
  - `jsonrpc2` = JSON-RPC 2.0 over serde_json
  - `tokio` = async runtime（LSP 本质是异步 I/O 密集）
  - `dalin-compiler` = 复用已有 Lexer/Parser/Ty2/AST

## Consequences

### 变得更轻松
- VSCode 用户开箱即可以用 `dalib-ls` 获得语法高亮 + 错误诊断 + 代码补全
- Jetbrains IDEA 通过 LSP4IJ 插件兼容
- 零额外开发成本，因为编译器已有 Lexer/Parser/Ty2

### 变难的事情
- LSP 是无状态协议，文档变更需要增量解析或全量重析——大文件性能需要考虑
- 需要维护 `lsp-types` 依赖（~10k LoC），增加编译时间
- 需要从 `main.rs` 重构为 `lib.rs + main.rs` 双模式
