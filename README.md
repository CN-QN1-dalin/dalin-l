# Dalin L — Agent-Native Programming Language

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-1.95+-orange.svg)

**Dalin L** 是一门面向 AI Agent 的编程语言，支持中文标识符、HM 类型推断、模式匹配与管道操作。

> Python 原型 → Rust 移植，从设计验证到可运行实现。

---

## 特性

- **中文标识符全链路支持** — 变量名、函数名、参数名全线中文化
- **HM 类型推断** — Robinson Unification + 多态函数调用
- **模式匹配** — wild / ident / lit / ctor / struct 五种模式 + 守卫 + 嵌套
- **管道操作** — `data |> filter |> map` 链式语法
- **if/match 表达式** — `let r = if true { 42 }`
- **Option / Result** — `Some(v) / None / Ok(v) / Err(e)`
- **递归 + 闭包** — 函数是第一公民
- **零依赖** — 纯 Rust 标准库实现
- **七通道类型系统** — Effect / Capability / Governance / Latency / Confidence / QN / Cognitive Loop
- **LLM 辅助编程** — `@llm("...")` 编译时指令，自动生成函数体骨架
- **自进化闭环** — Phase J 错误聚类 + 策略自动生成 + 人类审批接口

---

## 快速开始

**GitHub**

```bash
git clone https://github.com/CN-QN1-dalin/dalin-l.git
cd dalin-l
cargo run          # 运行演示
cargo run --repl   # 启动交互式 REPL
cargo run --test   # 运行测试
```

**Gitee（国内加速）**

```bash
git clone https://gitee.com/dalin-x/dalin-l.git
cd dalin-l
cargo run          # 运行演示
cargo run --repl   # 启动交互式 REPL
cargo run --test   # 运行测试
```

## 示例

```rust
let 名字 = "大林"
fn greet(n) {
    return "你好, " + n + "!"
}
println(greet(名字))       // → 你好, 大林!

// 递归阶乘
fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
println(fact(5))            // → 120

// 模式匹配
let opt = Some(100)
match opt {
    Some(v) => println("got", v),
    None => println("empty"),
}                            // → got 100

// 管道操作
fn double(x) { return x * 2 }
let r = 1 |> double |> double
println(r)                   // → 4

// 范围 + for 循环
let mut sum = 0
for i in 0..5 { sum = sum + i }
println(sum)                 // → 10
```

---

## 架构

```
Source ─→ Lexer ─→ Tokens ─→ Parser ─→ AST ─→ LLM Expand
                                          │
                                   Ty2 (七通道推断)
                                          │
                                 Latency Verifier
                                          │
                                       TaskSpec
                                          │
                          ┌───────────────┼───────────────┐
                          ▼               ▼               ▼
                    Control Plane    Runtime (DLVM)   Evolution Loop (Phase J)
```

### 编译器模块

| 模块 | 职责 |
|------|------|
| `token.rs` | 65+ Token 类型定义 |
| `ast.rs` | 30+ AST 节点 (表达式/语句/模式/宏/模块) |
| `lexer.rs` | 词法分析器 (中文标识符 / 转义 / 注释) |
| `parser.rs` | 递归下降语法分析器 (错误恢复 / 运算符优先级 / 多通道注解) |
| `ty.rs` | HM 类型推断引擎 (Robinson Unification) |
| `ty2.rs` | 七通道类型推断引擎 (Effect/Capability/Governance/Latency/Confidence/QN/Cognitive Loop) |
| `task_spec.rs` | TaskSpec 生成（编译器 → 控制面边界） |
| `latency.rs` | 延迟约束验证器 |
| `qn1.rs` | QN1 查询语言解析器 |
| `llm.rs` | LLM 编译扩展引擎 |
| `module.rs` | 模块系统 (模块树/依赖图/命名空间/冲突检测) |
| `package.rs` | 包管理系统 (dalin.toml/SemVer/依赖解析/缓存) |
| `macro_expand.rs` | 宏展开器 (declarative + derive) |
| `stdlib_loader.rs` | 标准库加载器 (.dal 文件解析/AST注入/缓存管理) |
| `error.rs` | 结构化错误类型 |
| `runtime.rs` | 运行时绑定定义 |

---

## 路线图

### Phase A — G — 已完成 ✓

| Phase | 名称 | 状态 | 核心成果 |
|-------|------|------|----------|
| A | 基础语法 + HM 推断 | ✓ | Lexer, Parser, TypeInferencer |
| B | 运行时解释器 | ✓ | Tree-traversal Interpreter |
| C | 认知 + 治理 | ✓ | `@perceive/@reason/@decide/@act`, `@gov(level)` |
| D | 时序契约 | ✓ | `@latency/@timeout/@throughput`, LatencyVerifier |
| E | QN 查询语言 | ✓ | QN1 解析器 + 推理 |
| F | 运行时并发 | ✓ | DLVM spawn/async/runtime |
| G | 控制面 | ✓ | Capability Scheduler + API Gateway |
| H | 模块/包系统 | ✓ | mod/use/derive, dalin.toml, SemVer |
| I | 宏系统 | ✓ | Declarative macros + derive 属性 |

### Phase I — L — 进行中 (v3.0-dev)

| Phase | 名称 | 状态 | 当前进展 |
|-------|------|------|----------|
| I | 标准库建设 | 🟡 进行中 | 28 个 .dal 模块已定义 (stdlib/)，实证测试开发中 |
| J | 自进化闭环 | 🔴 **实现中** | **v3.0 核心目标：J1~J4 完整落地** |
| K | Benchmark 基线 | ⚪ 待启动 | RingBuffer / SFA / ultra-infer 量化 |
| L | 跨 Agent 协同进化 | ⚪ 规划中 | 联邦学习 + 社区模板生态 |

#### Phase I: 标准库建设

标准库目录 `stdlib/` 包含以下模块：

| 模块 | 描述 |
|------|------|
| `core_types.dal` | Option, Result, Vec, String, HashMap 公开 API |
| `prelude.dal` | 预导入集合（自动加载） |
| `macros.dal` | assert, dbg, vec!, hashmap! 宏定义 |
| `iterators.dal` | Iterator, Iter, Range 迭代器协议 |
| `fn_traits.dal` | Fn/FnMut/FnOnce trait 族 |
| `traits_common.dal` | Display, Debug, Clone, Eq, Ord 等通用 trait |
| `math.dal` | 数学运算 + PI/E/TAU 常量 |
| `strings.dal` | String 方法扩展 |
| `collections.dal` | HashSet, LinkedList, BTreeMap 等集合 |
| `io.dal` | Read/Write trait + file/std io |
| `fs_extra.dal` | 文件系统扩展操作 |
| `net.dal` | TCP/HTTP/HTTPS 网络通信 |
| `json.dal` | JSON 序列化/反序列化 |
| `serialize.dal` | 通用序列化协议 |
| `encoding.dal` | Base64, Hex, UTF-8 编码工具 |
| `crypto.dal` | Hash, HMAC, AES 加密原语 |
| `regex.dal` | 正则表达式引擎 |
| `fmt.dal` | 格式化字符串 $"" 插值 |
| `errors.dal` | 统一错误类型和结果构建 |
| `result_builder.dal` | Result 链式构建工具 |
| `bit_ops.dal` | 位运算工具集 |
| `hash_funcs.dal` | 哈希函数集合 |
| `logging.dal` | 日志框架 |
| `testing.dal` | 测试框架和断言宏 |
| `uuid.dal` | UUID 生成器 |
| `async_primitives.dal` | Future, Promise, async/await 原语 |
| `concurrency.dal` | Mutex, RwLock, Atomic 等同步原语 |
| `time.dal` | 时间/日期/时钟工具 |
| `path_util.dal` | 路径解析和操作工具 |
| `process.dal` | 进程管理接口 |

标准库加载器已在编译器集成：
```rust
use compiler::stdlib_loader::{StdLibLoader, StdLibConfig};

// 从项目根目录加载
let loader = StdLibLoader::new(project_root)?;

// 按需加载模块
let core_ast = loader.load_module("core_types")?;

// 或一次性加载全部
let all_modules = loader.load_all()?;  // 返回 28 个模块名
```

#### Phase J: 自进化闭环

设计文档：[`docs/PHASE_J_SELF_EVOLUTION.md`](docs/PHASE_J_SELF_EVOLUTION.md)

核心机制：
- **J1 模式学习引擎**：运行时错误 → 语义哈希 → DBSCAN 聚类 → 修复模板
- **J2 策略自动生成**：从成功修复中学习新 recovery mode，动态更新 Calibrator 权重
- **J3 进化验证框架**：AB 实验分组 + 三层回归测试 + 综合评分函数
- **J4 人类审查接口**：`dalan evolve review` CLI + 审批决策矩阵 + atomic swap 回滚

---

## 测试

```bash
cargo run --test
# 42/42 passed, 0 failed
```

## 许可证

[MIT](LICENSE)

---

## 公开资料

### CSDN 系列技术文章

| # | 标题 | 日期 | 链接 |
|---|------|------|------|
| 1 | [Dalin L — 我造了一门支持中文编程的语言,完整移植到 Rust 了](https://blog.csdn.net/2601_96175637/article/details/162883913) | 2026-06-24 | [CSDN](https://blog.csdn.net/2601_96175637/article/details/162883913) |
| 2 | [Dalin L 2.0: 七通道类型系统 + 自修复运行时 + 语言服务器 + K8s 调度器](https://adg.csdn.net/6a58e5de10ee7a33f28e36df.html) | 2026-07-17 | [CSDN](https://adg.csdn.net/6a58e5de10ee7a33f28e36df.html) |
| 3 | [AI Agent 技术社区 · Dalin L 自进化编程语言](https://agent.csdn.net/6a58e5de662f9a54cb9010ed.html) | 2026-07-17 | [CSDN Agent](https://agent.csdn.net/6a58e5de662f9a54cb9010ed.html) |
| 4 | [Dalin L 2.0 — 2 万行 Rust 实现自进化语言](http://www.xxmr.cn/news/40733) *(镜像)* | 2026-07-17 | [镜像](http://www.xxmr.cn/news/40733) |
| 5 | [Dalin Soma v3.0 — 用菲尔兹奖数学给 ASI 意识奠基](https://blog.csdn.net/2601_96175637/article/details/162539119) | 2026-07-18 | [CSDN](https://blog.csdn.net/2601_96175637/article/details/162539119) |

### 项目分支索引

| 分支 | 版本 | 说明 | 链接 |
|------|------|------|------|
| `master` | v3.0-dev | 当前主分支: null/??/is-as/C FFI/M:N 调度器/stdlib 58 模块 | [GitHub](https://github.com/CN-QN1-dalin/dalin-l) · [Gitee](https://gitee.com/dalin-x/dalin-l) |
| `origin/v2-types` | v2.0 | Phase A-J 全线完成: 七通道类型系统/SelfHealing/LSP/K8s 算子 | [GitHub v2-types](https://github.com/CN-QN1-dalin/dalin-l/tree/v2-types) · [Gitee v2-types](https://gitee.com/dalin-x/dalin-l/tree/v2-types) |
| `origin/main` | v1.0 | Phase A-J 初版: 7 道类型·CLI 18cmd·分布式控制面 | [GitHub main](https://github.com/CN-QN1-dalin/dalin-l/tree/main) · [Gitee main](https://gitee.com/dalin-x/dalin-l/tree/main) |
| `v0.1.0` | v0.1.0 | Python v0.2 树遍历解释器 + HM 类型推断原型 | [GitHub tag](https://github.com/CN-QN1-dalin/dalin-l/releases/tag/v0.1.0) · [Gitee tag](https://gitee.com/dalin-x/dalin-l/tags/v0.1.0) |

### 版本演进时间线

| 日期 | 事件 |
|------|------|
| 2026-06-24 | v0.1.0 发布: HM 类型推断 + 树遍历解释器 + 模式匹配 (Python 原型 → Rust 移植) |
| 2026-07-15 | Dalin L 2.0 Phase A-J 全线完成 (2 万行 Rust, 318 测试全绿) |
| 2026-07-17 | P1-P10 升级: Trait System + GC 分代 + Criterion Bench; LSP/Deloy CRD/LLM 注入防护 |
| 2026-07-17 | v2-types 分支发布: 七通道类型系统 + SelfHealing + VSCode 扩展 + K8s Operator |
| 2026-07-18 | Dalin Soma v3.0 技术报告: 菲尔兹奖数学 (力迫法/非交换几何/同伦类型论) 应用于认知架构 |
| 2026-07-19 | **Dalin L 3.0 启动**: null 关键字/?? Elvis/is-as 类型检查/var 语句/C FFI 桥接/真实包管理器联网/M:N 协程调度器/stdlib 扩至 58 模块 (目标 100+) |

## 作者

**贾大林** ([@CN-QN1-dalin](https://github.com/CN-QN1-dalin) · [@dalin-x](https://gitee.com/dalin-x))
