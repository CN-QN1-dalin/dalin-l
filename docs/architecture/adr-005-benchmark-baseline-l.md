# ADR-005: Phase L — Benchmark Baseline

- **状态**: In Progress
- **日期**: 2026-07-17
- **上下文**: SFA 注意力路由、QN1 量子退火、ultra-infer 缺量化数据
- **决策**: 建立 Rust-side benchmark tests 作为基线测量手段

### 基准测试套件

| 类别 | 文件 | 测试项 |
|------|------|--------|
| Compilation Speed | `compiler/tests/bench_compile.rs` | lex, parse, type_check, scalable_growth |
| Seven-Channel Inference | `compiler/tests/bench_seven_channel.rs` | effect, capability, confidence, cognitive_loop, governance, time_constraint |
| Runtime Performance | `runtime/tests/bench_runtime.rs` | env_get_set, lookup, spawn_overhead, nesting |

### CI Pipeline
- `.github/workflows/ci.yml` — 主 CI: fmt/clippy/build/test/e2e/lsp
- `.github/workflows/benchmark.yml` — 独立 benchmark gate: compile/parse/typecheck/J

### 目标
- 每次 PR 自动产出编译时间基线
- 24 月环比增长可追溯
