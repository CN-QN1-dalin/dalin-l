# Dalin L 3.0 — 项目长期记忆

## 自修复自进化 (Phase 2, 2026-07-24)

### P0-1: Borrow Checker / Memory Safety 引擎
- **路径**: `compiler/src/borrow_check/`
- **核心**: `BorrowChecker::check(&Program) -> Vec<BorrowError>` — AST 遍历型借用分析
- **数据模型**: ScopeForest (层次作用域树), Binding (name+mutability+copyable), ActiveBorrow
- **Tier 1**: Copy/Move 语义 — copy 类型 (int/float/bool/char) 可自由复制, 非 copy 类型记录移动事件
- **Tier 2**: 借用互斥 — &T 允许多个, &mut T 独占, 借助 ScopeForest 验证
- **集成**: `pub mod borrow_check` in lib.rs, 8 个测试覆盖

### P0-2: Parser Error Recovery (Token Sync)
- **改动**: `parser.parse()` 从 `Result<Program, ParseError>` 改为 `-> Program` (永不失败)
- **恢复机制**: `try_recover(err)` 跳过 token 直到遇到 SYNC_TOKENS 或 Newline
- **SYNC_TOKENS**: 18 个语句级关键字 (let/fn/if/while/for/match/struct/enum/trait 等)
- **错误收集**: `parser.recovered() -> &[ParseError]` 供调用方检查
- **影响**: 20+ 文件适配新签名, 所有编译管线/CLI/LSP/DLVM 调用方
- **关键变化**: 语法错误不再硬中断编译 — 错误恢复后继续解析后续语句

### P1-2: Performance Profiler
- **路径**: `runtime/src/profiler.rs`
- **核心**: `Profiler` 结构体, 跟踪函数调用次数/总时间/最小/最大/平均值
- **数据**: `fn_stats: HashMap<String, FnStats>`, `expr_samples: Vec<ExprSample>`
- **CLI**: `dalin profile <file>` — 全链路(词法→语法→运行+剖析), 输出表格报告
- **集成**: `Interpreter::call_function` → `profile_enter_fn/profile_exit_fn`
- **测试**: 6 个单元测试
- **可开关**: `interp.enable_profiling()` / `.disable_profiling()`
