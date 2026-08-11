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
- [FIX-003](#fix-003--通用-c-ffi零依赖-dlopendlsym) — 通用 C FFI（零依赖 dlopen/dlsym）
- [FIX-004](#fix-004--包-registry-联网下载--索引解析--dalentoml-内联表解析修复) — 包 registry 联网下载 + 索引解析 + dalan.toml 内联表解析修复
- [FIX-005](#fix-005----运算符错误传播) — `?` 错误传播运算符（Option/Result 早退）

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

## FIX-003 — 通用 C FFI（零依赖 dlopen/dlsym）
- **日期**：2026-08-09
- **组件/文件**：`runtime/src/interpreter.rs`（`ffi_load` / `ffi_call` / `ffi_close` 内建 + FFI 原语）
  + `runtime/ffi_fixture/`（`cdylib` 测试固件）+ `runtime/build.rs`（编译期编译固件）
- **摘要**：正仓此前对 C 库的调用能力仅有空 `call_ffi` 桩，无任何真实 FFI 路径（recon 三大缺口之一）。
  现落地零依赖通用 C ABI 绑定：`ffi_load(path)` 经 `dlopen` 加载动态库并返回句柄 id，
  `ffi_call(lib_id, symbol, ret_type, args)` 经 `dlsym` 取地址并按 `f64/i64/void` 返回类 +
  0–4 参数（全 f64 / 全 i64 / 混合）封送调用，`ffi_close(lib_id)` 经 `dlclose` 卸载。
- **根因**：`extern "C"` 绑定从未实现；macOS Apple Silicon 无独立 `/usr/lib/*.dylib`（仅 dyld 共享缓存），
  故改用自构建 `cdylib` 固件验证，比直接 `dlopen(libm)` 更贴近真实通用 FFI 场景。
- **改动**：
  - 直接用 `dlopen/dlsym/dlclose/dlerror`（`cfg(macos|linux)`，句柄以 `usize` 存地址以保 `Interpreter: Send`），
    不引入 `libloading`；`ffi_handles: Arc<Mutex<HashMap<i64, usize>>>` 新增字段。
  - edition 2024 规则：`extern "C"` 须 `unsafe extern "C"`，`unsafe fn` 体内调用亦须显式 `unsafe{}`。
  - 删除全仓无调用点的 `call_ffi` 桩（避免 dead_code 破坏 clippy）。
  - `build.rs` 在编译期 `cargo build --release` 子 `ffi_fixture` 并将 `.dylib/.so` 拷至 `OUT_DIR` 供测试加载。
- **新增测试**：`ffi_fixture_basic_calls`（df_dsqrt/df_add/df_mul/df_sum4/df_iabs/df_strlen/df_void_print）
  / `ffi_call_unknown_symbol_errors` / `ffi_load_missing_lib_errors` /
  `ffi_close_unknown_handle_errors` / `ffi_close_then_call_errors`（macOS/Linux gated，5 passed）。
- **验证**：`cargo test -p dalin-runtime` 全量 13+5 passed；`cargo clippy -p dalin-runtime` 0 警告；
  `cargo fmt` 干净；全工作区 `cargo build` 通过。
- **审计来源**：生态能力 recon（2026-08-11）三大 Critical 缺口之首
- **关联不变量**：无（属能力新增，非安全不变量）

---

## FIX-004 — 包 registry 联网下载 + 索引解析 + dalan.toml 内联表解析修复
- **日期**：2026-08-11
- **组件/文件**：
  - `registry/src/net.rs`（新增）：零依赖 HTTP/1.1 客户端 + `fetch_package_index` + `download_artifact`
    + `resolve_best`；`registry/src/sha256.rs`（新增）：纯 std SHA-256。
  - `cli/src/cmd/pkg.rs`：`cmd_build` 对 `DependencySource::Registry` 真正拉取 `.dal` 并落盘
    `dalan.lock`（含 `url=` + `sha256=`，缓存至 `.dalan/registry/<name>/<ver>.dal`，已缓存则跳过下载）。
  - `compiler/src/package.rs`：`parse_dep_entry` 内联表 `{ }` 括号剥离修复（修复 `version`/`source`
    被静默丢弃的潜在 bug）。
- **摘要**：recon 第二大缺口——`Package.artifact_url` 已就位但无网络下载逻辑；`compiler::PackageManager::download_package`
  为伪造 mock。现 `dalin-registry` 真正联网：从 `http://<host>/index/<name>` 拉 JSON 包索引、按版本需求
  （`*`/`^`/`>=`/精确）选最优版本、将 `artifact_url` 指向的工件下载到本地缓存并计算 SHA-256 写入锁文件。
- **根因**：
  1. 联网下载缺口：registry crate 仅有内存 `PackageRegistry`，无 HTTP 客户端。
  2. **内联表解析 bug（潜在）**：`parse_dep_entry` 未剥离 `{ }`，导致 `version`/`source` 字段被静默丢弃；
     用户写 `mylib = { version = "1.0", source = "host" }` 时 `source` 失效（端到端验证时暴露）。
- **改动**：
  - `net.rs`：`http_get` 基于 `TcpStream`，支持重定向（≤5 跳）与 chunked 解码；`download_artifact` 写文件 + SHA-256；
    `resolve_best` 复用 `dalin_compiler::package::SemVer` 做版本匹配（registry→compiler 单向依赖，无环）。
  - `cli` 新增 `dalin-registry` 依赖；`cmd_build` 对 Registry 源调用 `fetch_package_index`→`resolve_best`→`download_artifact`。
  - `package.rs`：`parse_dep_entry` 先 `strip_prefix('{')`/`strip_suffix('}')` 再按 `,` 拆分，正确解析
    `version`/`optional`/`default-features`/`source`。
- **新增测试**：`registry` 集成测试 `integration_http_index_and_download`（本地 `TcpListener` 桩 server 验证
  索引+下载+SHA-256+版本选择，1 passed）；`sha256::known_vectors`（abc/空串/狐狸语已知向量，2 passed）；
  `net` 单元（parse_req/resolve_best/decode_chunked/parse_response，6 passed）；
  `compiler::test_parse_dep_entry_inline_table_with_source`（内联表 source 解析，1 passed）。
- **验证**：`cargo test -p dalin-registry` 14 passed；`cargo test -p dalin-compiler package` 25 passed；
  `cargo clippy -p dalin-registry -p dalin_l` 0 警告；`cargo fmt` 干净；全工作区 `cargo build` 通过；
  **端到端**：本地 stub registry + `dalib pkg build` 实测下载 `mylib@1.0.0.dal` 并生成带 `url`/`sha256` 的 `dalan.lock`。
- **审计来源**：生态能力 recon（2026-08-11）第二大 Critical 缺口
- **关联不变量**：无（属能力新增 + 解析正确性修复）
- **备注**：`compiler::PackageManager::download_package` 的 mock 保留为无网络占位（已在 doc 注释指向真实实现），
  避免改动 dev 模式单测语义。

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

---

## FIX-005 — `?` 错误传播运算符
- **日期**：2026-08-11
- **组件/文件**：
  - `compiler/src/ast.rs` → `Expr` 枚举新增 `Try(Box<Expr>)`
  - `compiler/src/parser.rs` → 抽取 `parse_postfix` 方法；`Option/Result` 字面量、括号、数组、块、标识符统一走 postfix 链，新增 `?` 分支（`Expr::Try`）；`expr_to_string` 补 `Try` 分支
  - `runtime/src/interpreter.rs` → `eval_expr` 加 `Expr::Try` 分发 + 新增 `eval_try`（主运行时语义）
  - `compiler/src/runtime.rs` → 编译器内置解释器 `eval_expr` 加 `Try` 分发 + `eval_try`（借 `returned`/`return_value` 标志，与 `return` 同通道）
  - `compiler/src/ty.rs` → `infer_expr` 补 `Try` 分支（内层仅做类型推断，解包结果以 `Unknown` 暴露；类型系统对 Option/Result 不做参数化，无法静态还原 `T`）
  - `compiler/src/jit.rs` → `expr_to_ir_expr` 大 or-pattern 补 `| Expr::Try(_)`（`unimplemented` 兜底）
  - `targets/wasm/src/lib.rs` → `expr_to_wat` 补 `Expr::Try(inner) => expr_to_wat(inner)`（降级编译内层）
- **摘要**：新增 `?` 运算符，对齐 Rust `?` 的值级错误传播语义——`Some(v)?`/`Ok(v)?` 解包为 `v`；
  `None?`/`Err(e)?` 以错误值经 `return` 的 `CTRL_RETURN` 哨兵 + `return_value` 机制**早退**，
  错误值逐层向上一调用者的 `?` 传播（与 Rust 同构）。`?` 可接在任何 primary 之后：`a?`、`f()?`、`a.b()?.c`、`Some(42)?`、`(g())?`、`arr[0]?`。
- **根因**：
  1. 原 `Expr` 无 `Try` 变体，lexer 虽已 tokenize `?` 为 `QuestionMark`，但 parser/interpreter 未处理；
  2. `Some/None/Ok/Err` 构造函数走独立 `parse_primary` 分支直接 `return`，**绕过**了 `.`/`(`/`[`/`?` 的 postfix 循环，
     导致 `Some(42)?` 的 `?` 不被消费（`None?`/`Some(42)?` 在 `let x = ...?` 中因 `?` 未解析而行为错乱）。
- **改动**：
  1. `ast.rs`：`Expr::Try(Box<Expr>)`；
  2. `parser.rs`：把内联 postfix 循环抽成 `parse_postfix(&mut self, obj)`，`Option/Result` 字面量、括号、数组、块在构造基础表达式后均 `return self.parse_postfix(base)`；`?` 作为 postfix 分支包裹为 `Expr::Try`；
  3. `interpreter.rs`：`eval_try` 求值内层后按形状判定——
     `Option(false,_)`→置 `return_value=Some(Option(false,None))` 并 `Err(CTRL_RETURN)` 早退；
     `Option(true,Some(v))`→`Ok(*v)`；`Option(true,None)`→`Ok(None)`；
     `Result(false,_,Some(e))`→置 `return_value=Some(e)` 并 `Err(CTRL_RETURN)` 早退；
     `Result(true,Some(v),_)`→`Ok(*v)`；`Result(true,None,_)`→`Ok(None)`；其余→原样产出（宽松语义，不强制类型）；
  4. `compiler/src/runtime.rs`：同构实现（用 `returned`/`return_value` 布尔标志，`exec_block` 逐层提前退出）；
  5. 其余 5 个对 `Expr` 穷尽 match 的文件（ty/jit/wasm/编译器内置解释器）补齐 `Try` 分支，确保全工作区编译通过。
- **新增测试**（均置于 `runtime/src/interpreter.rs` `#[cfg(test)] mod tests`，经 `run()` 端到端覆盖 parse+eval）：
  - `try_operator_ok_unwraps`：`inner()=Ok(7)` → `outer()=7`
  - `try_operator_err_propagates`：`inner()=Err("boom")` → `outer()` 提前返回 `"boom"`
  - `try_operator_some_unwraps`：`Some(42)?` → `42`
  - `try_operator_none_early_returns`：`None?` → 提前返回 `None`（值级）
  - `try_operator_nested_propagation`：`lvl3→lvl2→lvl1` 三级 `Err("deep")` 经两层 `?` 传播到 `lvl1()="deep"`
- **验证**：`cargo test --workspace` 全绿（含 runtime 705→现全量、compiler 396+）；`cargo clippy --workspace --all-targets` 零警告；`cargo fmt --check` 干净。
- **审计来源**：#696（本期 `?` 运算符实现任务）。
- **关联不变量**：复用 `return` 的 `CTRL_RETURN` + `return_value` 通道（见 `runtime-safety-invariants.md` INV-1/INV-2 上下文）；`?` 不引入新控制流哨兵，避免与 `break`/`continue` 越界诊断冲突。
- **已知边界**：类型检查阶段 `Try` 的解包结果类型为 `Unknown`（类型系统 Option/Result 非参数化），属宽松设计；`?` 真实早退语义在主运行时 `dalin-runtime::interpreter` 完整实现，编译器内置解释器（`compiler/src/runtime.rs`）及 JIT/WASM 后端为降级/兜底路径。
