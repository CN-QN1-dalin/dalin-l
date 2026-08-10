# Dalin L 3.0 工程审计报告 — 重审正仓 `dalin-l-rs`

- **日期**：2026-08-11
- **审计对象**：`~/Desktop/dalin-l-rs`（正仓，15 crate，~37.5k 行）
- **审计背景**：用户提交了一份自称 scope `~/Desktop/Dalin-L-3.0` 的"工程级审计"（1 Blocker + 3 Critical），
  经复核该审计实际落在**陈旧兄弟副本**上，对正仓 0 项适用（见 `docs/fix-log.md` 审计复核记录）。
  用户选定「重审正仓 dalin-l-rs」，本文件即真实重审结果。
- **审计方法**：recon-first（先读源码/调用链再下结论）；严格区分 `#[cfg(test)]` 与 **生产代码**；
  只对"不可信输入可达的解析/执行路径"做稳健性判定；数字取当前基线（实跑 `cargo test/clippy/fmt`）。
- **审计范围**：源码编译入口、模块/stdlib 加载、包清单解析、运行时解释器、CLI、网络/registry（不可信输入面）。

---

## 一、结论（Verdict）

> **无需返工（No Blocker / No Critical）。生产代码中不可信输入可达的解析与执行路径，经 recon 确认均已 panic-safe。**

- 此前在 `docs/fix-log.md` 中误标的"真问题"（`runtime.rs:1769` / `ty2.rs:1896,2138` / `package.rs` 的
  `.expect("parse ...")`）**经复验为 test-only 辅助函数，非生产缺陷，已撤回标注**（见第四节）。
- 仅发现 1 个门禁类小问题（`cargo fmt --check` 漂移，3 个文件），已修复并复验通过（见第五节）。
- 发现 2 个低严重度次要项（CLI `init` 鲁棒性、借用检查器行号归因），非阻断，按需整改（见第六节）。

---

## 二、生产不可信输入路径 — 逐项 recon 结果

| # | 路径 | 文件:行 | 错误处理方式 | 畸形输入是否 panic |
| --- | --- | --- | --- | --- |
| 1 | 源码编译主入口 | `compiler/src/lib.rs:71-93` | `lex.tokenize()` / `parser.parse()` 均 `match` → `CompileResult::Err(...)` | ❌ 不 panic |
| 2 | 模块 / stdlib 加载 | `compiler/src/stdlib_loader.rs:205-227` | `tokenize()`/`parse()` 均 `match` → 带行列号 `Err(...)` | ❌ 不 panic |
| 3 | 包清单 `dalan.toml` 解析 | `cli/src/cmd/pkg.rs:34-38` | `read_manifest` 直接 `return parse_package_manifest(&content)`（`Result`） | ❌ 不 panic |
| 4 | 运行时解释器 | `runtime/src/interpreter.rs` | 生产代码仅 `Mutex::lock().unwrap()`（毒锁极端边界，标准 Rust 模式）+ `.unwrap_or(...)` 兜底；`panic!/expect` 仅在 `#[cfg(test)]`（:1759 起） | ❌ 生产不 panic |

**判定**：4 条不可信输入主链均将解析/词法错误转为结构化 `Err` 上抛，未在任何生产路径对
`Parser::parse()` / `Lexer::tokenize()` 的 `Result` 做 `.unwrap()`/`.expect()`。

---

## 三、已确认安全的子项（支撑结论）

- `parse_package_manifest`（`compiler/src/package.rs:189`）返回 `Result<PackageManifest, String>`，
  全工作区唯一生产调用方 `cli/src/cmd/pkg.rs:37` 以 `?`/返回值透传，无 unwrap。
- `compiler/src/package.rs:30-59` `SemVer::parse` 用 `.parse().map_err(...)?` + `unwrap_or(0)` 兜底。
- `compiler/src/parser.rs:1171-1172,1440-1444` 数字字面量 `tok.value.parse().unwrap_or(0/0.0)` 为安全兜底。
- `compiler/src/self_evolution.rs:173` `template_id.parse().unwrap_or(0)` 安全兜底。

---

## 四、Phantom 发现 — 撤回与说明（重要）

> 审计纪律：凡"看起来像缺陷但实为测试辅助/陈旧副本"的项，必须明确撤回，避免后续 agent 误改正仓。

1. **`parse().expect` "真问题" → 撤回**：`compiler/src/runtime.rs:1769`、`ty2.rs:1896/2138`、
   `package.rs:635/656/700/715` 的 `.expect("parse ...")` 全部位于：
   - `#[cfg(test)] mod tests {`（边界 `runtime.rs:1703`、`ty2.rs:1690`）内的测试辅助函数
     （`simple_fn`/`parse_fn_annotations`/`parse_fn_all_annotations`）；
   - 或 `#[test]` 函数（`package.rs` 的 `test_parse_*`）内对**已知合法输入**的辅助调用。
   → **不构成生产稳健性风险，不做整改。** 原 `docs/fix-log.md` 中相关"真问题"标注已更正。

2. **前一份"工程级审计"的 Blocker/Critical → 全部不适用正仓**（已在 fix-log 审计复核记录裁定）：
   pyo3 Blocker（正仓已 `exclude`）、cffi transmute UB / 持锁 panic（正仓无 FFI）、
   借用检查器空转（正仓 `lib.rs:103-104` 已接线遍历 AST）、#1/#2（已修 FIX-001/FIX-002）、
   测试数 268 / 文件 `engine.rs`（失真/phantom）。

---

## 五、门禁状态（实跑验证，2026-08-11）

| 门禁 | 结果 | 说明 |
| --- | --- | --- |
| `cargo test` | ✅ **720 passed / 0 failed** | 高于此前 705 基线；全工作区 `test result: ok` |
| `cargo clippy --workspace` | ✅ **0 warnings** | 零警告 |
| `cargo fmt --check` | ✅ **clean** | 修复前 3 文件漂移（`jit.rs`/`lexer.rs`/`parser.rs`），已 `cargo fmt` 复验通过 |

> 修复动作：`cargo fmt`（仅重排 3 文件的空白/折行，无语义改动）。已本地提交（未 push，等用户指令）。

---

## 六、仍属实的次要发现（低严重度，非阻断）

| # | 位置 | 问题 | 严重度 | 建议 |
| --- | --- | --- | --- | --- |
| A | `cli/src/cmd/init.rs:86,112,124,133,141,142,153` | 对 `project_name.to_str()` 与 `read_to_string(...)` 用 `.unwrap()`；`init` 假定刚写出的文件必存在/路径必合法 | 低（健壮性） | 改 `?` + 友好报错；正常路径安全，仅极端边界（竞态/非法 UTF-8 路径）才 panic |
| B | `compiler/src/lib.rs:110` `record_borrow_error(err, 0)` | 借用检查错误行号以 `0` 占位，自进化 J1 事件缺真实位置归因 | 低（可观测性/正确性） | 并入路线图 #3+#9，从 AST `Span` 提取真实行列 |
| C | 路线图遗留正确性项 #4/#5/#6/#7/#8（pipe 语义 / 命名参数 / 性能） | 功能/正确性工作，非本次"安全/稳健"审计阻断项 | 中（功能） | 按原路线图推进，不属本审计紧急项 |

---

## 七、建议

1. **无需紧急整改**：生产不可信输入路径经 recon 确认无 panic 类阻断缺陷，门禁全绿。
2. **可选小修**：项 A 可作 CLI 鲁棒性小修（低风险、纯工程）；项 B 随 #3+#9 路线图处理。
3. **保持纪律**：后续任何审计 agent 先读 `docs/fix-log.md` + `docs/runtime-safety-invariants.md`，
   并严格区分 `#[cfg(test)]` 与生产代码，避免重复报告 phantom 项（尤其"测试辅助函数里的 expect"）。
4. **勿对陈旧副本误改**：`~/Desktop/Dalin-L-3.0` 是独立陈旧 checkout，其审计条目仅对该目录有效。

---

## 附录：本次 recon 实读文件

- `compiler/src/lib.rs`（:55-125 编译管线 + 借用检查接线）
- `compiler/src/stdlib_loader.rs`（:195-241 模块加载错误处理）
- `compiler/src/package.rs`（:25-60 `SemVer`/`parse_package_manifest`；:600-717 测试辅助）
- `compiler/src/runtime.rs`（:1703-1771 测试模块边界 + `parse` 辅助）
- `compiler/src/ty2.rs`（:1690-1906 / 2132-2160 测试模块边界 + 注解解析辅助）
- `cli/src/cmd/pkg.rs`（:20-60 清单读写错误处理）
- `runtime/src/interpreter.rs`（:1759-1834 测试模块边界；生产 `Mutex`/`unwrap_or` 用法）
- `registry/src/lib.rs`、`cli/src/cmd/{init,run,build,check,profile,evolve}.rs`（unwrap 分布扫描）
