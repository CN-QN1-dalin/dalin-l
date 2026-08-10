# Dalin L 3.0 — 修复留底台账（Fix Log / Audit Trail）

> ## ⚠️ 留底政策（强制，自 2026-08-10 起）
> **所有修复（正确性 / 安全 / 稳健性 / 性能 / 语义，无论大小）都必须在本文档留底。**
> 目的：让后续任何审计 agent、贡献者、自进化 patch 都能快速了解
> 「发生了什么、为什么、改了哪、怎么验证」，避免重复报告已修复项、并了解当前基线。
>
> ### 留底规则
> 1. 每个修复一个 `FIX-XXX` 条目，按序递增（FIX-001, FIX-002, …）。
> 2. **必填字段**：日期 · 组件/文件 · 摘要 · 根因 · 改动 · 新增/修改测试 · 验证结果 · 审计来源（如某次审计 #N）· 关联不变量（`INV-x`）。
> 3. 改动合并前必须已完成 `cargo test`（全绿）/ `cargo clippy`（零警告）/ `cargo fmt --check`（干净），并在条目中记录验证结果。
> 4. 关联运行时不变量见 [`docs/runtime-safety-invariants.md`](./runtime-safety-invariants.md)；二者分工：**本台账记录"发生了什么"，不变量文档记录"必须守住什么"**。
> 5. 修复若对应某条不变量，交叉引用 `INV-x`；若来自某次审计，交叉引用其编号。
>
> ### 审计 agent 使用说明
> 审计前**先读本台账 + `runtime-safety-invariants.md`**，可跳过已修复项、了解当前质量基线
> （测试数、clippy/fmt 门禁），并据此判断"哪些是真未修复、哪些是审计失真（phantom 文件/过时数字）"。

---

## 目录
- [FIX-001](#fix-001--逻辑运算符短路求值) — 逻辑运算符短路求值（INV-1）
- [FIX-002](#fix-002--整数溢出与除零保护) — 整数溢出与除零保护（INV-2）

---

## FIX-001 — 逻辑运算符短路求值
- **日期**：2026-08-10
- **组件/文件**：`runtime/src/interpreter.rs` → `eval_binary`
- **摘要**：`&&` / `||` 原本先急切求值两操作数再运算，丢失短路语义，导致右值副作用
  （如 `10 / b > 0` 除零、`arr[i]` 越界）在左值已能决定结果时仍被触发 → 崩溃。
- **根因**：短路算子与普通算子共用"先求左右、再运算"路径，未对 `&&`/`||` 单独处理。
- **改动**：在急切求值之前对 `&&`/`||` 单独分支——先求左值，按 `truthy(left)` 决定是否求右值；
  并在 `eval_binary` 加 doc 注释指向 `runtime-safety-invariants.md`。
- **新增测试**：`test_short_circuit_and_skips_rhs` / `test_short_circuit_or_skips_rhs` /
  `test_non_short_circuit_still_works`（`runtime/tests/integration_error_paths.rs`）
- **验证**：`cargo test` 全工作区 0 failed（基线 **705**）；`cargo clippy -p dalin-runtime` 干净；
  `cargo fmt` 干净。
- **审计来源**：安全/质量审计 **#1**（高优先级 · 正确性/安全）
- **关联不变量**：INV-1
- **状态**：✅ 已修 · 本地未 push（等用户指令）

---

## FIX-002 — 整数溢出与除零保护
- **日期**：2026-08-10
- **组件/文件**：`runtime/src/interpreter.rs` → `eval_binary`
- **摘要**：`i64` 的 `+ - *` 溢出在 debug 下 panic、release 下静默回绕；`/ %` 无零守卫 → 除零/模零 panic。
- **根因**：直接使用 Rust 原生算术，未使用 `checked_*` 与零守卫。
- **改动**：
  - `+ - *` → `checked_add / checked_sub / checked_mul`，溢出映射 `RuntimeError("integer overflow in ...")`；
  - `/` → `b == 0` 守卫返回 `division by zero`，否则 `checked_div`（溢出亦映射 `RuntimeError`）；
  - `%` → `b == 0` 守卫返回 `modulo by zero`。
  - 运行时错误统一为 `RuntimeError(String)`（**注意：不存在 `RuntimePanic`**，审计用词需映射为此）。
- **新增测试**：`test_integer_overflow_is_runtime_error` / `test_integer_division_by_zero_is_runtime_error` /
  `test_integer_modulo_by_zero_is_runtime_error`
- **验证**：`cargo test` 全工作区 0 failed（基线 705）；`cargo clippy` 干净；`cargo fmt` 干净。
- **审计来源**：安全/质量审计 **#2**（高优先级 · 正确性/安全）
- **关联不变量**：INV-2
- **备注**：当前构建**不支持 `i64::MAX` 路径语法**（报 `Undefined variable: 'i64'`），
  溢出测试须用大整数字面量（如 `4631686018427387904 * 2`）。
- **状态**：✅ 已修 · 本地未 push（等用户指令）

---

<!-- 新修复追加在上方目录与下方：复制 FIX-002 块，编号 +1，填必填字段。 -->

---

## 审计复核记录（Audit Reconciliation Log）

> 记录"某次审计经复核被判定为不适用 / 失真"的事件，供后续 agent 快速跳过，避免重复劳动或误改正仓。

### 2026-08-11 — 「工程级审计」（自称 scope: `~/Desktop/Dalin-L-3.0`）判定为「不适用 dalin-l-rs」
- **复核结论**：该审计实际在**陈旧兄弟副本 `~/Desktop/Dalin-L-3.0`** 上生成（gitee `dalin-x/dalin-l`，
  Phase R/S，~558 tests，旧架构：`parse() -> Program` 非 `Result`、`pyo3-bindings` 为 workspace 成员、含 `cffi.rs`），
  并非正仓 `~/Desktop/dalin-l-rs`（705 tests）。对正仓 **0 项适用**，且重复报告了已修复的 #1/#2。
- **逐条裁定（针对正仓 `dalin-l-rs`）**：

  | 审计条目 | 裁定 | 证据 |
  | --- | --- | --- |
  | 🔴 Blocker: pyo3 致 `cargo check --workspace` 失败 | **不适用** | 正仓 `pyo3-bindings` 在 `Cargo.toml` `exclude`；`parse()` 返回 `Result`，`.map_err` 合法 |
  | 🔴 Critical: cffi.rs transmute UB（:239-726） | **phantom** | 正仓无 `cffi.rs`、无 `call_c_impl`、无 `transmute`、无 `extern "C"`/`libloading`/`c_void` |
  | 🔴 Critical: cffi.rs 持锁 panic（:165） | **phantom** | 同上（FFI 能力尚未迁入正仓） |
  | 🔴 Critical: 借用检查器空转 | **不成立** | `compiler/src/lib.rs:103-104` 调用 `check_program` 并遍历 AST |
  | 🟠 #1 `&&`/`||` 不短路 | **已修** | FIX-001 已在 `interpreter.rs`（短路分支） |
  | 🟠 #2 整数无溢出保护 | **已修** | FIX-002 已在 `interpreter.rs`（`checked_*` + 零守卫） |
  | 🟡 测试数 268 | **失真** | 正仓 705；即便 `Dalin-L-3.0` 自身历史亦 558 |
  | 🟡 文件 `engine.rs` / `borrow_check/engine.rs` | **phantom** | 两仓均不存在（正仓实为 `interpreter.rs` / `borrow_checker.rs`） |

- **复核中标注的"真问题"经复验为 phantom（已更正）**：原记录称正仓
  `compiler/src/runtime.rs:1769`、`ty2.rs:1896/2138`、`package.rs:635/656/700/715`
  对合法 `Result` 用 `.expect("parse ...")` 为"畸形输入会 panic"。**复验结论：这些调用
  全部位于 `#[cfg(test)] mod tests`（`runtime.rs:1703`、`ty2.rs:1690`）或 `#[test]` 函数内部
  （`package.rs`），是对已知合法输入的测试辅助函数，不构成生产路径稳健性风险。**
  详见下方「2026-08-11（续）— 重审正仓 dalin-l-rs」条目。**请勿据此对测试辅助函数做无意义整改。**
- **处置**：本审计**不驱动任何 dalin-l-rs 整改**。后续若需行动，按以下分流：
  1. 想要对正仓的真实审计 → 重新审计 `dalin-l-rs`；
  2. 想要修复 `Dalin-L-3.0` → 以该目录为目标，本审计条目对其有效（pyo3 / cffi / borrow 等）。

---

### 2026-08-11（续）— 重审正仓 `dalin-l-rs`（真实工程审计）
- **复核范围**：用户选定"重审正仓 dalin-l-rs"。对生产代码（非 `#[cfg(test)]`）中
  **不可信输入可达的解析/执行路径**做 recon，重点：源码编译入口、模块/stdlib 加载、包清单解析、运行时解释器、网络/registry。
- **关键结论（生产路径已 panic-safe）**：
  - 源码编译主入口 `compiler/src/lib.rs:71-93`：`lex.tokenize()` 与 `parser.parse()` 均用 `match`，
    失败返回 `CompileResult::Err(...)`，**畸形源码不 panic**。
  - 模块/stdlib 加载 `compiler/src/stdlib_loader.rs:205-227`：`tokenize()`/`parse()` 均用 `match`，
    返回带行列号的 `Err(...)`，**加载含语法错误的 .dal 不 panic**。
  - 包清单解析 `cli/src/cmd/pkg.rs:34-38`：`read_manifest` 直接 `return parse_package_manifest(&content)`
    （`Result`），畸形 `dalan.toml` 上抛为 `Err(String)`，CLI 链友好报错。
  - 运行时解释器 `runtime/src/interpreter.rs`：生产代码仅用 `Mutex::lock().unwrap()`（毒锁极端边界，
    标准 Rust 模式）与 `.unwrap_or(...)` 安全兜底；`panic!/expect` 仅出现在 `#[cfg(test)]`（:1759 起）。
- **对"phantom 真问题"的再确认**：`runtime.rs:1769` / `ty2.rs:1896,2138` / `package.rs:635,656,700,715`
  的 `.expect("parse ...")` 全部在测试辅助函数内，**非生产缺陷**，撤回原"真问题"标注。
- **仍属实的次要发现（低严重度，非 Blocker/Critical，按需整改）**：
  1. `cli/src/cmd/init.rs`（:86,112,124,133,141,142,153）对 `project_name.to_str()` 与
     `read_to_string(...)` 用 `.unwrap()`——`init` 刚写出的文件被假定必存在/路径必合法，
     正常路径安全，但文件系统竞争/非法 UTF-8 路径会 panic 而非友好报错（健壮性，非安全）。
  2. 借用检查器错误行号仍以 `0` 占位（`compiler/src/lib.rs:110` `record_borrow_error(err, 0)`），
     自进化 J1 事件缺真实位置归因（#3+#9 路线项，正确性/可观测性）。
  3. 路线图遗留正确性项（用户此前未撤销）：#4 pipe 语义、#5 命名参数生效、#6/#7/#8 性能，
     属功能/正确性工作，非本次"安全/稳健"审计的阻断项。
- **处置建议**：无需紧急整改；#1 可作 CLI 健壮性小修（改为 `?`/友好报错），#2 纳入 #3+#9，
  #3 按路线图推进。生产不可信输入路径经 recon 确认 **无 panic 类阻断缺陷**。
