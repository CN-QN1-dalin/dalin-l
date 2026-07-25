# Changelog

## v3.0.0-dev (2026-07-25)

### 🚀 新特性
- **Agent Handshake Protocol (AHP)**: 通用 Agent 握手协议 SDK，支持文件/Unix/TCP/内存四种传输层，Rust SDK + Python 桥接 (`dalin-handshake/`)
- **调试器 (DAP)**: Debug Adapter Protocol 服务器，集成 VS Code 调试
- **原生编译 (LLVM)**: `dalib build --native` 通过 LLVM IR 生成原生二进制 (`codegen/`)
- **控制面 (Control Plane)**: 分布式 Agent 调度系统，支持 gRPC/Redis/Postgres/etcd/Kubernetes
- **七通道类型系统**: 效应(Effect)/能力(Capability)/置信度/认知循环/治理/时间约束/延迟七维类型检查
- **自进化协议 (J1-J4)**: 诊断→策略生成→验证→报告的全自动代码进化闭环
- **增量编译缓存**: 基于文件 hash 的 `.dalin_cache/`，跳过未变更文件的重复编译
- **性能分析器**: `dalib profile` 子命令，函数级调用统计

### 🔧 语言功能
- **模式匹配**: Literal/Wildcard/Guard 子句/Tuple/Enum Variant/Nested match
- **错误处理**: Try/Catch 块，支持 Result/Option 传播
- **字符串插值**: `"Hello, {\\name}"` 语法
- **枚举 + 关联数据**: `Option<T>`, `Result<T,E>`, 变体解构
- **Trait 泛型**: 定义/实现/单态化
- **协程**: spawn/channel/await 并发原语
- **C FFI**: 通过 libloading 动态调用 C 库
- **LLM API 集成**: 通过 ureq 调用外部大模型

### 📚 标准库
- **33 个 .dal 模块**: core_types, collections, math, json, io, time, crypto, networking, testing, qn (认知引擎) 等
- **4 个生产级 Agent Demo**: 监控平台 / 交易引擎 / 代码审查 / CI/CD 流水线

### 🛠️ 工具链
- **CLI (dalib)**: 23 个子命令 — run/build/check/profile/repl/init/tree/info/evolve/vm 等
- **LSP 服务器**: 完整语言服务器协议 (24 测试覆盖)
- **格式化器**: dalin-fmt 代码格式化
- **包管理器 (Cryo)**: dalin.toml / SemVer 解析 / 依赖锁定
- **REPL**: 交互式 Shell

### ✅ 质量
- **测试**: 491 个测试，全部通过
- **代码质量**: Clippy 零警告 (默认+pedantic)，零 unsafe 代码，零生产代码 panic/unwrap
- **文档**: 语言规范 / API 参考 / 用户指南 / 握手协议规范
- **CI**: GitHub Actions 多平台矩阵 (Linux/macOS/Windows, stable/beta)

### ⚠️ 已知限制
- 无 JIT 编译器 (纯 AST/字节码解释执行)
- 标准库深度有限 (33 模块，工业级需要 100+)
- 无 WASM 编译目标
- 无 VS Code 插件
- 包注册中心需联网部署

---

## v0.1.0 (2026-06)

### 初始版本
- 基础编译器 (Lexer/Parser/Type Checker)
- DLVM 字节码虚拟机
- 基础 CLI
- HM 类型推断
- 基础 stdlib
