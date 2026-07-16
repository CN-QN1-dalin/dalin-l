# ADR-019: VSCode 官方扩展 — Language Client

## Status
Accepted

## Context
Dalin L 需要 IDE/编辑器支持才能降低开发门槛。LSP server (`dalin-ls`) 已经实现了 LSP 3.17 协议核心能力（diagnostic/hover/completion/signatureHelp）。VSCode 作为市场占有率最高的编辑器，必须支持。

## Decision
采用 `vscode-languageclient` v8 作为 LSP 客户端桥接器，通过 stdio 与 `dalin-ls` 进程通信。

### 架构
```
┌───────────────────────────┐
│       VSCode Editor       │
│  ┌─────────────────────┐  │
│  │ VSCode Extension    │  │
│  │ (extension.ts)      │  │
│  │   ┌───────────────┐ │  │
│  │   │LanguageClient │ │  │
│  │   │ (stdio: dalin │ │  │
│  │   │  -ls --stdio) │ │  │
│  │   └───────┬───────┘ │  │
│  └───────────┼─────────┘  │
└──────────────┼────────────┘
               │ JSON-RPC
               │ over stdio
               ▼
┌───────────────────────────┐
│     Dalin L LSP Server    │
│   (dalin-ls binary)       │
│  ┌─────────────────────┐  │
│  │ DocumentManager     │  │
│  │ Lexer → Parser →   │  │
│  │ Ty2 Inference      │  │
│  └─────────────────────┘  │
└───────────────────────────┘
```

### 技术选型对比
| 方案 | 优点 | 缺点 |
|------|------|------|
| vscode-languageclient (选) | 标准化、社区成熟、自动处理连接/重连 | 需要编译 TS → JS |
| 手动 spawn cp | 零依赖 | 需要自己处理 JSON-RPC 帧、超时、重连 |
| Web-based playground | 跨平台 | 无法提供 IDE 级体验 |

### 扩展功能清单
| 功能 | 实现方式 | 优先级 |
|------|---------|--------|
| 语法高亮 | TextMate Grammar (.tmLanguage.json) | P0 — 立即可用 |
| 错误诊断 | LSP diagnostic (已内置) | P0 — dalin-ls 已实现 |
| 代码补全 | LSP completion (已内置) | P0 — dalin-ls 已实现 |
| 悬停信息 | LSP hover (含七通道数据) | P0 — dalin-ls 已实现 |
| 快捷键 | ctrl+space / ctrl+shift+space | P1 — 用户友好 |
| 命令面板 | dalib compile/run/init | P1 — 一键操作 |
| 代码片段 | snippets/dalan.json (20+ snippets) | P2 — 开发效率 |
| 括号匹配 | language-configuration.json | P2 — 编辑器体验 |

## Consequences
- **更容易的采用**: 开发者安装扩展即可开始写 .dal 文件
- **开发闭环**: 编辑 → 编译 → 诊断 → 修复，在编辑器内完成
- **维护负担**: 需要维护 VSCode 扩展的生命周期 (ts-loader, webpack, npm publish)
- **依赖 dalin-ls**: 扩展完全依赖 LSP server 的运行，如果 dalin-ls 不兼容扩展会静默失效
