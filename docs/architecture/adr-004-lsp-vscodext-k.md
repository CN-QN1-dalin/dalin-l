# ADR-004: Phase K — LSP + VSCode Extension Infrastructure

- **状态**: Implemented
- **日期**: 2026-07-17
- **上下文**: 外部评价指出"生态空白" — 需要 IDE 级支持
- **决策**: 实现基于 LSP 3.17 的语言服务器 + VSCode 官方扩展

### 组件清单

| 组件 | 位置 | 说明 |
|------|------|------|
| LSP Server | `lsp/src/main.rs` | JSON-RPC over stdio, diagnostics/completion/hover |
| VSCode Ext | `extensions/vscode-dalan/src/extension.ts` | LanguageClient 桥接 |
| TextMate Grammar | `extensions/vscode-dalan/syntaxes/dalan.tmLanguage.json` | 词法高亮 113 行规则 |
| Extension Manifest | `extensions/vscode-dalan/package.json` | 5 commands, 6 config options |

### LSP API 兼容性
- ty2.rs 公开 `SevenChannelInferencer` 供 LSP 消费
- parser.rs 使用 `TokenType::*` 通配符导入解决 re-export
- 10 LSP warnings 全部清零

### 后果
- 开发者可在 VSCode 中获得语法高亮 + 实时诊断 + 代码补全
- CI pipeline 包含 LSP binary build check job
