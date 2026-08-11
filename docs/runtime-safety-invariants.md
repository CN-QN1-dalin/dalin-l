# Dalin L 运行时安全不变量与审计防复发清单

> 目的：把一次安全/质量审计中暴露的系统性缺陷，沉淀为**可复查的运行时不变量**与**审计执行纪律**。
> 任何后续审计、贡献者 PR、或自进化 patch 都必须对照本清单复查，避免同类错误复发。
>
> 关联整改：`runtime/src/interpreter.rs` 的 `eval_binary`（#1 短路求值、#2 溢出/除零保护）。
> 关联回归测试：`runtime/tests/integration_error_paths.rs`。

---

## 0. 背景：本次整改暴露了什么

在一次安全/质量审计中，Dalin L 3.0 解释器（`runtime/src/interpreter.rs`，树遍历解释器）
被指出两类**正确性 / 安全**缺陷，实跑确认属实并已修复：

- **#1 逻辑运算符不短路**：`&&` / `||` 先急切求值两个操作数再做布尔运算，导致右值里的
  副作用（如 `10 / b > 0` 的除零、`arr[i]` 的越界）在左值已经能决定结果时仍被触发 → 崩溃。
- **#2 整数运算无溢出 / 除零保护**：`i64` 的 `+ - *` 在 debug 下 panic、release 下静默回绕；
  `/ %` 无零守卫 → 除零 / 模零 panic。

这两类都是**语言语义正确性**问题，不是性能优化。它们在形式化语言实现里属于
"必须被不变量守住"的硬约束，因此本文把它们写成不变量。

---

## 1. 运行时不变量（Runtime Invariants）

### INV-1：逻辑运算符必须短路（Short-circuit semantics）

| 项 | 内容 |
| --- | --- |
| **语义** | `a && b`：先求 `a`，若 `truthy(a)` 为 false 则整体为 false，**不求值 `b`**。<br>`a \|\| b`：先求 `a`，若 `truthy(a)` 为 true 则整体为 true，**不求值 `b`**。 |
| **根因** | 在 `eval_binary` 中对 `&&`/`\|\|` 与 `+ - *` 等普通算子走同一"先急切求值左右、再运算"路径，丢失了短路语义。 |
| **错误形态** | `let b = 0; b != 0 && 10 / b > 0` 在 `b == 0` 时除零崩溃；`i > 0 && arr[i]` 越界。 |
| **正确形态** | 在急切求值之前，对 `&&`/`\|\|` 单独处理：先求左值，按 `truthy(left)` 决定是否（以及是否）求右值。 |
| **判定（人工/自动）** | 含右值副作用且在左值足以决定结果时不崩溃、返回符合短路预期的值。 |
| **回归测试** | `test_short_circuit_and_skips_rhs` / `test_short_circuit_or_skips_rhs` / `test_non_short_circuit_still_works` |

> ⚠️ 短路是**语言契约**，不是实现细节。任何把 `&&`/`||` 退化为"先求两操作数"的重构都违反本不变量。

### INV-2：整数运算溢出与除零必须返回错误，绝不 panic

| 项 | 内容 |
| --- | --- |
| **语义** | 整数算术溢出、除零、模零一律返回 `RuntimeError(String)`（运行时唯一错误类型），**不得在 debug 抛 Rust panic、不得在 release 静默回绕**。 |
| **根因** | 直接用 Rust 原生 `a + b` / `a - b` / `a * b` / `a / b` / `a % b`，未使用 `checked_*` 与零守卫。 |
| **错误形态** | `i64::MAX + 1` 在 debug panic；`10 / 0` 除零 panic；`10 % 0` 模零 panic。 |
| **正确形态** | `+ - *` → `checked_add / checked_sub / checked_mul`，返回 `None` 时映射为 `RuntimeError("integer overflow in ...")`；<br>`/` → `b == 0` 守卫返回 `division by zero`，否则 `checked_div`（溢出亦映射为 `RuntimeError`）；<br>`%` → `b == 0` 守卫返回 `modulo by zero`。 |
| **错误类型约定** | 运行时**只有** `RuntimeError(pub String)`（`interpreter.rs:10`）。不存在 `RuntimePanic`——审计若写"返回 RuntimePanic"属用词失真，应映射为 `RuntimeError`。 |
| **判定（人工/自动）** | 触发溢出的程序退出码非 0 且打印 `RuntimeError: integer overflow ...`；触发除/模零同理打印 `division by zero` / `modulo by zero`。绝不出现 Rust panic backtrace。 |
| **回归测试** | `test_integer_overflow_is_runtime_error` / `test_integer_division_by_zero_is_runtime_error` / `test_integer_modulo_by_zero_is_runtime_error` |

> ⚠️ 注意：当前构建**不支持** `i64::MAX` 路径语法（会报 `Undefined variable: 'i64'`）。
> 溢出测试须用大整数字面量（如 `4631686018427387904 * 2`），不要依赖 `i64::MAX`。

---

## 2. 审计执行纪律（Audit Discipline）—— 防止审计本身犯错

本次审计还暴露了一个**元问题**：审计结论基于一份**过时/失真的代码快照**，引用了不存在的文件与数字。
为使后续审计可靠，确立以下纪律：

1. **先 recon，再下结论（核实优先）**
   - 引用的文件/行号必须先 `find` / `grep` 确认**当前仓库确实存在**。审计曾引用
     `runtime/engine.rs`、`borrow_check/engine.rs`，但当前仓库实际为
     `runtime/src/interpreter.rs`（1796 行，树遍历解释器）与 `compiler/src/borrow_checker.rs`。
   - 行号随重构漂移，仅作线索，不作为依据。

2. **行为类缺陷以"实跑复现"为准**
   - 不下"看起来会崩"的结论。必须写最小复现程序，用 `cargo build -p dalin_l` 后
     `./target/debug/dalib run -i <case>.dal` 实跑确认（panic / 错误 / 正确）。
   - 本仓库 `dalib run` 入口遵循 `main` 约定（`run_program`），纯 `fn main(){...}` 会被自动调用。

3. **数字必须取当前基线**
   - 审计称"268 个单测全绿"已过时；当前全工作区基线为 **705 tests**。
     任何"X 个测试"的断言都用 `cargo test 2>&1 | grep "test result:"` 重新统计。

4. **错误类型以源码枚举为准**
   - 运行时错误统一是 `RuntimeError(String)`。不得臆造 `RuntimePanic` 等不存在的类型名。

5. **改动不得破坏既定质量门禁**
   - 每次整改后必须跑：`cargo test`（全工作区 0 failed）、`cargo clippy`（零警告）、
     `cargo fmt --check`（干净）。基线纪律见 `docs/release-process.md`。

6. **审计必须先确认"目标仓库路径"就是正仓（最强纪律，新增于 2026-08-11）**
   - **正仓唯一路径**：`~/Desktop/dalin-l-rs`（15 crate，基线 **705 tests**，`parse()` 返回
     `Result`，`pyo3-bindings` 按设计 `exclude`，**无 C FFI**）。
   - 仓库内存在**陈旧兄弟副本 `~/Desktop/Dalin-L-3.0`**（gitee 远程 `dalin-x/dalin-l`，停在
     Phase R/S，约 558 tests，旧架构：`parse() -> Program` 非 `Result`、`pyo3-bindings` 为
     workspace 成员、含 `cffi.rs`）。它与正仓是**两个独立目录**，不要混淆。
   - **2026-08-11 事故**：一份"工程级审计"在 `Dalin-L-3.0` 上生成，却以 `Dalin L 3.0` 名义提交，
     其结论对 `dalin-l-rs` **100% 不适用**（逐条裁定见 `fix-log.md` 审计复核记录）。
   - **教训**：审计 agent 启动前必须用 `git -C <path> remote -v` / `ls <path>` 确认它在审计
     **正仓**，而非某个同名/相似名的陈旧副本；若报告 scope 路径 ≠ `dalin-l-rs`，先停下核对，
     **不要基于疑似陈旧副本下整改结论**。

---

## 3. 回归测试索引（随改随查）

文件：`runtime/tests/integration_error_paths.rs`（用 `run_program` 真实调用零参 `main`，
断言返回值 `Vec<Value>` 末位）：

| 测试 | 守护的不变量 |
| --- | --- |
| `test_short_circuit_and_skips_rhs` | INV-1：`&&` 左 false 跳过右值，返回 false 不崩溃 |
| `test_short_circuit_or_skips_rhs` | INV-1：`\|\|` 左 true 跳过右值，返回 true 不崩溃 |
| `test_non_short_circuit_still_works` | INV-1：非短路路径仍正确求值 |
| `test_integer_overflow_is_runtime_error` | INV-2：乘法溢出 → `RuntimeError("integer overflow ...")` |
| `test_integer_division_by_zero_is_runtime_error` | INV-2：除零 → `RuntimeError("division by zero")` |
| `test_integer_modulo_by_zero_is_runtime_error` | INV-2：模零 → `RuntimeError("modulo by zero")` |

新增同类缺陷的回归用例时，直接往该文件追加，并同步更新上表。

> 历史修复条目（含本不变量对应的 **FIX-001 / FIX-002**）统一登记在
> [`docs/fix-log.md`](./fix-log.md)（强制留底台账）。审计前请两文档一起读：
> 本文件记"必须守住什么"，`fix-log.md` 记"已经发生了什么、怎么验证"。

---

## 4. 复查命令（一键验证不变量未被破坏）

```bash
# 1) 构建
cargo build -p dalin_l

# 2) 全工作区测试（基线 705，必须 0 failed）
cargo test 2>&1 | grep -E "test result:|FAILED|panicked"

# 3) 仅跑错误路径回归
cargo test -p dalin-runtime --test integration_error_paths

# 4) 质量门禁
cargo clippy --workspace
cargo fmt --check
```

---

## 5. 后续同类敏感点（审计 Checklist 扩展位）

以下属于同族的语义正确性敏感点及其整改进展（✅ 已修 · phantom 撤回 · ⏳ 待推进），后续审计按状态复查（详见整改清单）：

- **#3 + #9 借用检查器真正生效**：`compiler/src/borrow_checker.rs` 已被 `compiler/src/lib.rs:103-104`
  调用并遍历 AST（**非 no-op**，2026-08-11 复核确认）；但 `BorrowError` 是否带准确行列号、
  use-after-move / 不可变重绑定是否全捕获，仍待逐项确认。
- **#4 pipe 语义**：`map/filter/fold` 不得是误导性空实现。✅ **已满足**（2026-08-11）：`|>` 在 lexer/parser/interpreter/ty/jit/wasm/dlvm 全覆盖，`iter_map`/`iter_filter`/`iter_fold` 在 `stdlib/iterators.dal` 为真实实现（非空壳）。
- **#5 命名参数**：`name: expr` 形式参数必须按形参名绑定并做缺参/超参/未知参数检查，不得静默丢弃。⏳ **待推进**（设计级新功能，全仓无 `NamedArg` 实现，非 bug）：等用户设计决策。
- **#6 Range 物化上限**：`a..b` 不得在无上限下物化超大序列（DoS 面）。✅ **已加固**（2026-08-11，FIX-008）：`MAX_RANGE_LEN = 1_000_000`，生产解释器 `eval_range`（`runtime/src/interpreter.rs`）+ 编译器内置解释器 `Expr::Range`（`compiler/src/runtime.rs`）两处守卫，加回归测试 `range_materialization_is_bounded`。
- **#7 / #8 / #10 lexer 稳健性**：
  - **#7 每 token 重建 HashMap**（性能）：**phantom 撤回**——`build_keywords()` 仅 `Lexer::new` 构建一次（`self.keywords` 字段），后续只 `get` 查询（lexer.rs:336），非每 token 重建。
  - **#8 数字误切分**（正确性）：✅ **已修**（FIX-007），越界整数 / 空 hex 返回清晰 `LexerError`。
  - **#10 块注释未闭合**：⏳ 待确认（lexer `read_block_comment` 对未闭合 `/*` 到 EOF 的处理）。

> 任何新写入 `eval_binary` / `eval_unary` / 算术路径的算子，都必须先过 INV-1 / INV-2 两条不变量。
