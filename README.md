# Dalin L v3.0 — Agent-Native Programming Language

[![Rust](https://img.shields.io/badge/rust-1.95+-orange.svg)](https://www.rust-lang.org/)
[![Workspace](https://img.shields.io/badge/workspace-12%20crates-blue.svg)](.)

**Dalin L** 是一门面向 AI Agent 的编程语言，由 Cohen 力迫法、Connes 非交换几何、Voevodsky HoTT 和 Banach 不动点定理 统一构建自演化认知架构。当前版本 v3.0-dev。

> **QN1 幻化引擎 (Dalin Soma 系列)** 核心组件 — 理论线 + 工程线并行推进

---

## 特性速览

| 特性 | 说明 |
|------|------|
| **中文标识符全链路** | 变量名 / 函数名 / 参数名全线中文化 |
| **七通道类型系统** | Effect · Capability · Governance · Latency · Confidence · QN · Cognitive Loop |
| **HM + Robinson Unification** | 完整类型推断引擎 |
| **模式匹配** | wild / ident / lit / ctor / struct 五种模式 + 守卫 + 嵌套 |
| **管道操作符** | `data |> filter |> map` 链式语法 |
| **Borrow Checker** | 完整的内存安全引擎：owner / move / borrow / lifetime tracking |
| **Parser Error Recovery** | Token sync 到语句边界，可恢复解析继续编译 |
| **DLVM M:N 协程调度** | CoopSpawn / CoopAwait / CoopYieldResume |
| **LLM 辅助编程** | `@llm("...")` 编译时指令，自动生成函数体骨架 |
| **自进化闭环** | J1 模式学习 → J2 策略生成 → J3 进化验证 → J4 人类审批 |
| **LSP 支持** | 接入 lsp-types crate，completion / hover / signatureHelp |
| **C FFI Bridge** | libloading + resolve_symbol，跨平台 FFI |
| **Cryo 包管理器** | dalin.toml / SemVer / 联网依赖解析 |
| **128+ 标准库模块** | json / crypto / sha256 / bloom / trie / merkle / http 等完整实现 |

---

## 快速开始

```bash
# GitHub
git clone https://github.com/CN-QN1-dalin/dalin-l.git

# Gitee (国内加速)
git clone git@gitee.com:dalin-x/dalin-l.git

cd dalin-l
cargo run          # 运行演示
cargo run --repl   # 启动交互式 REPL
cargo test         # 运行测试 (554 passing / 0 failed)
cargo clippy       # Clippy 零警告
```

## 示例

```dalin
// 中文标识符
let 名字 = "大林"
fn greet(n) {
    return "你好, " + n + "!"
}
println(greet(名字))       // → 你好, 大林!

// 递归 + Option
fn fact(n) {
    if n <= 1 { return Some(1) }
    return Some(n * fact(n - 1).unwrap())
}

// 管道操作
fn double(x) { return x * 2 }
let r = 1 |> double |> double |> double
println(r)                  // → 8

// 模式匹配 + 守卫
let opt = Some(42)
match opt {
    Some(v) if v > 0 => println("positive", v),
    _ => println("other"),
}

// 闭包 + 高阶函数
let nums = [1, 2, 3, 4, 5]
let result = nums.map(|x| x * x).filter(|x| x > 4)
println(result)             // → [9, 16, 25]
```

---

## 架构总览

```
Source ─→ Lexer (65+ Token) ─→ Parser ─→ AST (30+ Node)
                                              │
                                    Ty2 (七通道推断)
                                              │
                             Borrow Checker / Memory Safety
                                              │
                            Latency Verifier / TaskSpec
                                              │
            ┌───────────────┼───────────────┼───────────────┐
            ▼               ▼               ▼               ▼
      Control Plane     DLVM (VM)    Phase J        LSP Server
      (K8s CRD)    (stack-based)   (Self-Evolve)   (lsp-types)
```

### Workspace 结构 (12 Crates)

| Crate | Source Files | 职责 |
|-------|-------------|------|
| `dalin-compiler` | 40 files | Lexer / Parser / Ty2 / Borrow Checker / Macro Expand |
| `dalin-cli` | 24 files | `dalib` CLI (build / run / repl / evolve / lsp / fmt) |
| `dalin-control-plane` | 20 files | Capability Scheduler / API Gateway / K8s CRD |
| `dalin-runtime` | 8 files | Interpreter / Environment / Value / DLVM VM |
| `dalin-dlvm` | 2 files | Bytecode Compiler + Stack-Based VM + M:N Coroutines |
| `dalin-lsp` | 1 file | Language Server Protocol (lsp-types integration) |
| `dalin-dap` | 3 files | Debug Adapter Protocol |
| `dalin-codegen` | 2 files | WASM Code Generation |
| `dalin-registry` | 1 file | Package registry client |
| `dalin-stdlib` | 1 file | Stdlib loader & manifest |
| `dalin-fmt` | 1 file | Code formatter |
| `dalin-pyo3` | — | Python bindings (environment-dependent) |

### 代码规模

| 指标 | 数值 |
|------|------|
| Rust 源码 | ~122 files, ~35,384 LOC (excluding tests/benches) |
| 标准库 | 128 .dal modules, ~5,466 LOC |
| 测试 | 554 passing / 0 failed |
| Clippy | 0 warnings (default + pedantic) |

---

## 版本演进时间线

| 日期 | 事件 |
|------|------|
| 2026-06-24 | v0.1.0: HM 类型推断 + 树遍历解释器 + 模式匹配 (Python 原型 → Rust 移植) |
| 2026-07-15 | Phase A-J 全线完成: 七通道类型系统 + SelfHealing + LSP + K8s Operator |
| 2026-07-17 | P1-P10 升级: Trait System + GC 分代 + Criterion Bench; LSP/Deloy CRD/LLM 注入防护 |
| 2026-07-18 | Dalin Soma v3.0 技术报告: 菲尔兹奖数学应用于认知架构 |
| 2026-07-19 | **Dalin L 3.0 启动**: null 关键字 / ?? Elvis / is-as 类型检查 / C FFI / M:N 协程调度 |
| 2026-07-24 | **P0/P1 全面落地**: Borrow Checker ✅ / Parser Error Recovery ✅ / DLVM Opcode 全覆盖 ✅ / Stdlib 128 模块无 stub ✅ / Clippy 零警告 ✅ / 554 tests green ✅ |

---

## 公开资料

### CSDN 技术文章

| # | 标题 | 链接 |
|---|------|------|
| 1 | [Dalin L — 我造了一门支持中文编程的语言, 完整移植到 Rust 了](https://blog.csdn.net/2601_96175637/article/details/162883913) | [CSDN](https://blog.csdn.net/2601_96175637/article/details/162883913) |
| 2 | [Dalin L 2.0: 七通道类型系统 + 自修复运行时 + 语言服务器 + K8s 调度器](https://adg.csdn.net/6a58e5de10ee7a33f28e36df.html) | [CSDN](https://adg.csdn.net/6a58e5de10ee7a33f28e36df.html) |
| 3 | [AI Agent 技术社区 · Dalin L 自进化编程语言](https://agent.csdn.net/6a58e5de662f9a54cb9010ed.html) | [CSDN Agent](https://agent.csdn.net/6a58e5de662f9a54cb9010ed.html) |
| 4 | [Dalin Soma v3.0 — 用菲尔兹奖数学给 ASI 意识奠基](https://blog.csdn.net/2601_96175637/article/details/162539119) | [CSDN](https://blog.csdn.net/2601_96175637/article/details/162539119) |

### 项目分支索引

| 分支 | 版本 | 说明 | 链接 |
|------|------|------|------|
| `master` | v3.0-dev | 当前主分支: Borrow Checker / Error Recovery / DLVM Opcode / Stdlib 128 模块 | [GitHub](https://github.com/CN-QN1-dalin/dalin-l) · [Gitee](https://gitee.com/dalin-x/dalin-l) |
| `v2-types` | v2.0 | Phase A-J 全线完成: 七通道类型系统 / SelfHealing / LSP / K8s 算子 | [GitHub](https://github.com/CN-QN1-dalin/dalin-l/tree/v2-types) · [Gitee](https://gitee.com/dalin-x/dalin-l/tree/v2-types) |
| `main` | v1.0 | Phase A-J 初版: 7 道类型 · CLI 18cmd · 分布式控制面 | [GitHub](https://github.com/CN-QN1-dalin/dalin-l/tree/main) · [Gitee](https://gitee.com/dalin-x/dalin-l/tree/main) |
| `v0.1.0` | v0.1.0 | Python 原型: 树遍历解释器 + HM 类型推断 | [GitHub Tag](https://github.com/CN-QN1-dalin/dalin-l/releases/tag/v0.1.0) · [Gitee Tags](https://gitee.com/dalin-x/dalin-l/tags/v0.1.0) |

---

## 作者

**贾大林** ([@CN-QN1-dalin](https://github.com/CN-QN1-dalin) · [@dalin-x](https://gitee.com/dalin-x))

独立 AI 研究者 / 工程师，全栈（Python / Rust / C++ / Metal Shading Language）。

> QN1 幻化引擎 (Dalin Soma) — 工程线 + 理论线并行。
