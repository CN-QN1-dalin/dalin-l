# Contributing to Dalin L 3.0

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/CN-QN1-dalin/dalin-l.git
cd dalin-l

# 编译
cargo build --workspace --exclude dalin-pyo3

# 测试
cargo test --workspace --exclude dalin-pyo3

# 运行
cargo run --bin dalib -- run examples/hello.dal
```

## 代码规范

- 运行 `cargo fmt --all` 格式化代码
- 确保 `cargo clippy --workspace --exclude dalin-pyo3` 零警告
- 新功能必须有测试 (`cargo test --workspace --exclude dalin-pyo3`)
- 公共 API 必须有文档注释 (`///`)

## 分支策略

- `master`: 稳定版
- `v2-types`: 开发主线
- 功能分支从 `v2-types` 分出

## 架构概览

```
compiler/       — 编译器核心 (Lexer/Parser/TypeChecker/DLVM Codegen/Cache)
runtime/        — AST 解释器 + 并发原语 + Profiler
cli/            — dalib 命令行接口
dlvm/           — 字节码定义 + 编译器
codegen/        — LLVM 原生代码生成 (可选)
lsp/            — 语言服务器协议
fmt/            — 代码格式化
registry/       — 包管理器 (Cryo)
control-plane/  — 分布式控制面
dalin-handshake/— Agent 握手协议 SDK
stdlib/         — 标准库 (.dal 文件)
dap/            — 调试适配器协议
pyo3-bindings/  — Python 绑定 (隔离)
```

## 测试

```bash
# 全量测试
cargo test --workspace --exclude dalin-pyo3

# 指定 crate
cargo test -p dalin-compiler

# 基准测试
cargo bench --workspace --exclude dalin-pyo3

# 集成测试
cargo test --test integration_compile_pipeline
```

## 自进化协议

Dalin L 支持自修复自进化：

```bash
# 运行自进化诊断
cargo run --bin dalib -- evolve diagnose

# 生成进化策略
cargo run --bin dalib -- evolve plan

# 执行进化
cargo run --bin dalib -- evolve apply
```

## 报告问题

在 [GitHub Issues](https://github.com/CN-QN1-dalin/dalin-l/issues) 提交。
