//! Dalin L 3.0 — 编译器核心
//!
//! 提供从源码到 DLVM 字节码的全链路编译能力：
//! - **Lexer** — 词法分析，支持中文标识符
//! - **Parser** — LL(1) 递归下降解析器，支持错误恢复
//! - **Type Checker** — HM 类型推断 + 七通道类型系统
//! - **Macro Expansion** — 内置宏展开（assert, dbg 等）
//! - **Bytecode Cache** — 基于文件 hash 的增量编译缓存
//! - **Self-Evolution** — J1/J2/J3 自修复协议引擎
//! - **Stdlib Loader** — 标准库 .dal 文件加载
//! - **LLM Integration** — 外部 LLM API 调用
//! - **Runtime** — 测试执行引擎
//! - **Package Manager** — dalin.toml 解析 + 依赖解析
//!

use std::fmt::Write;

pub mod ast;
pub mod error;
pub mod latency;
pub mod lexer;
pub mod llm;
pub mod parser;
pub mod qn1;
pub mod runtime;
pub mod task_spec;
/// Dalin L 3.0 — 编译器工具链 crate
///
/// 把源码落到七通道类型系统的"可执行单元" (`TaskSpec`)：
/// token → lexer → parser → 宏展开/LLM扩展 → (ty2 七通道推断) → `task_spec`。
/// 纯编译期，无运行时并发依赖，可作为独立库被 runtime / control-plane 复用。
pub mod token;
pub mod ty;
pub mod ty2;
// Phase H: 模块/包系统 + 宏系统
pub mod macro_expand;
pub mod module;
pub mod package;
// Phase H+: 标准库加载器
pub mod stdlib_loader;
// Phase J: 自进化闭环
pub mod j1_pattern_learning;
pub mod j2_strategy_gen;
pub mod j3_evolution_verify;
// Bytecode cache for incremental compilation
pub mod cache;
// Phase JIT: LLVM ORC JIT 编译器骨架
pub mod jit;
/// Static code quality analyzer — industry benchmarked lint rules
pub mod quality;
// Borrow Checker (Memory Safety) - P0 milestone
pub mod borrow_checker;
pub mod self_evolution;

use crate::ast::{Program, Stmt};
use crate::borrow_checker::BorrowChecker;
use crate::error::ChannelError;
use crate::self_evolution::SelfEvolutionEngine;
use crate::task_spec::TaskSpec;
use crate::ty2::SevenChannelInferencer;

/// Full compilation pipeline (including @llm expansion):
///   token → lexer → parser → macro expansion/LLM expansion → ty2 inference → `task_spec`
///
/// @llm expansion stage: scan the AST for all `Stmt::Fn` { `llm_prompt`: Some(prompt), .. },
/// call `LlmEngine.process_directive()` to generate a function body skeleton and replace the original body.
#[must_use]
pub fn compile_with_llm(src: &str) -> CompileResult {
    // Step 1: Lexer
    let mut lex = lexer::Lexer::new(src);
    let tokens = match lex.tokenize() {
        Ok(t) => t,
        Err(e) => return CompileResult::Err(format!("{e}")),
    };

    // Step 2: Parser
    let mut parser = parser::Parser::new(tokens);
    let prog = match parser.parse() {
        Ok((p, errors)) => {
            // 错误恢复模式下收集到的语法错误必须上报，不能静默吞掉。
            // 恢复的目的是继续发现更多错误，而不是假装程序有效。
            if !errors.is_empty() {
                let detail = errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                return CompileResult::Err(format!("parse errors: {detail}"));
            }
            p
        }
        Err(e) => return CompileResult::Err(format!("{e}")),
    };

    // Step 3: LLM 扩展
    let expanded = expand_llm(&prog);

    // Step 4: 七通道类型推断
    let mut infer = SevenChannelInferencer::new();
    infer.infer_program(&expanded);

    // Step 5: Borrow checker — memory safety (P0 milestone)
    let mut borrows = BorrowChecker::new();
    borrows.check_program(&expanded);

    // Step 6: 自进化错误收集 — 所有 borrow checker 错误进入 J1 流水线
    let mut evolution_engine = SelfEvolutionEngine::new("/tmp/dalin_kb.jsonl");
    for err in borrows.errors() {
        // 将每条 borrow 错误转化为 J1 事件记录；主行号取自错误变体自身的真实位置（不再以 0 占位）
        evolution_engine.record_borrow_error(err, err.primary_line());
    }

    // Step 7: 延迟验证（Phase D — 时序契约）
    let latency_result = latency::LatencyVerifier::verify(&expanded);

    // Step 8: 生成 TaskSpec
    let specs = task_spec::from_program(&expanded);

    let mut report = infer.print_report();
    if !borrows.errors().is_empty() {
        report.push_str("\n=== Borrow Checker Errors ===\n");
        for err in borrows.errors() {
            writeln!(report, "  ❌ Borrow check error: {}", err).unwrap();
        }
    }
    if !latency_result.errors.is_empty() {
        report.push_str("\n=== Latency Violations ===\n");
        for err in &latency_result.errors {
            writeln!(report, "  ❌ {err}").unwrap();
        }
    }
    // 打印自进化状态（开发/调试用）
    if !borrows.errors().is_empty() {
        report.push_str(&format!(
            "\n=== Self-Evolution Status: {} ===\n",
            evolution_engine.current_status()
        ));
    }

    CompileResult::Ok {
        program: expanded,
        report,
        specs,
        errors: borrows
            .errors()
            .iter()
            .map(|e| ChannelError::BorrowCheckFailed {
                location: crate::error::SourceLocation {
                    line: 0,
                    column: 0,
                    filename: "borrow".into(),
                },
                detail: e.to_string(),
            })
            .chain(
                latency_result
                    .errors
                    .iter()
                    .map(|e| ChannelError::LatencyViolation {
                        location: crate::error::SourceLocation {
                            line: 0,
                            column: 0,
                            filename: "compile".into(),
                        },
                        declared_ms: 0,
                        actual_ms: 0,
                        detail: e.clone(),
                    }),
            )
            .collect::<Vec<_>>(),
    }
}

/// Compilation result: AST + report + `TaskSpec` + structured errors
pub enum CompileResult {
    Err(String),
    Ok {
        program: Program,
        report: String,
        specs: Vec<TaskSpec>,
        /// 结构化编译错误（七通道违规 + 延迟违规）
        errors: Vec<ChannelError>,
    },
}

impl std::fmt::Display for CompileResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileResult::Err(e) => write!(f, "Compile error: {e}"),
            CompileResult::Ok {
                program,
                report,
                specs,
                errors,
            } => {
                writeln!(f, "Compiled {} statements", program.statements.len())?;
                write!(f, "{report}")?;
                for err in errors {
                    write!(f, "{err}")?;
                }
                for spec in specs {
                    writeln!(
                        f,
                        "  Task: {} [effect={:?} cap={:?} idempotent={}]",
                        spec.fn_id, spec.effect, spec.capability, spec.idempotency_key
                    )?;
                }
                Ok(())
            }
        }
    }
}

/// LLM 扩展：遍历 AST，遇到 `llm_prompt=Some` 的函数则调用 `LlmEngine`
fn expand_llm(prog: &Program) -> Program {
    let mut stmts = Vec::new();
    for stmt in &prog.statements {
        if let Stmt::Fn {
            name,
            params,
            return_type,
            effect,
            capability,
            llm_prompt,
            confidence: _,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            body: _,
            async_,
            pub_,
        } = stmt
        {
            if let Some(prompt) = llm_prompt.clone() {
                // 调用 LLM 引擎生成代码
                let r_gen = llm::LlmEngine::process_directive(&prompt, Some(name));
                // 如果生成的语句中有 Fn，提取其 body 作为当前函数的 body；否则用生成语句本身
                let new_body = if !r_gen.statements.is_empty()
                    && matches!(&r_gen.statements[0], Stmt::Fn { .. })
                {
                    match &r_gen.statements[0] {
                        Stmt::Fn { body, .. } => (*body).clone(),
                        _ => Box::new(vec![]),
                    }
                } else {
                    Box::new(r_gen.statements)
                };
                stmts.push(Stmt::Fn {
                    name: name.clone(),
                    params: params.clone(),
                    return_type: return_type.clone(),
                    effect: effect.clone(),
                    capability: capability.clone(),
                    llm_prompt: None,
                    confidence: None,
                    cognitive_loop: cognitive_loop.clone(),
                    governance: governance.clone(),
                    latency: latency.clone(),
                    timeout: timeout.clone(),
                    throughput: throughput.clone(),
                    body: new_body,
                    async_: *async_,
                    pub_: *pub_,
                });
            } else {
                stmts.push(stmt.clone());
            }
        } else {
            stmts.push(stmt.clone());
        }
    }
    Program {
        statements: stmts,
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    }
}

// ═══════════════════════════════
//  P2.3 — E2E 集成测试
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_e2e_simple_pure_fn() {
        let src = "fn add(x: int, y: int) -> int { return x + y }";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok {
                program,
                report,
                specs,
                errors,
            } => {
                assert_eq!(program.statements.len(), 1, "one function");
                assert_eq!(specs.len(), 1, "one TaskSpec");
                assert_eq!(specs[0].fn_id, "add");
                assert!(errors.is_empty(), "no compile errors for simple fn");
                assert!(report.contains("No type errors"), "report says clean");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
    }

    #[test]
    fn test_e2e_multi_channel_annotations() {
        let src = "\
fn sensor_read() @ io @ cpu @ perceive @ gov(prepare) @ latency(50ms) {
    return 42
}";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok {
                program,
                specs,
                errors,
                report,
            } => {
                assert_eq!(program.statements.len(), 1, "one function");
                assert_eq!(specs.len(), 1, "one TaskSpec");
                // 验证 TaskSpec 的正确保留
                assert_eq!(specs[0].fn_id, "sensor_read");
                // 验证报告包含所有通道
                assert!(report.contains("@ io"), "report shows effect");
                assert!(report.contains("@ cpu"), "report shows capability");
                assert!(
                    report.contains("loop(perceive)"),
                    "report shows cognitive loop"
                );
                assert!(report.contains("gov(prepare)"), "report shows governance");
                // latency 可能不在 Display 中，但 time_constraint 在
                assert!(errors.is_empty(), "no errors for valid multi-channel fn");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
    }

    #[test]
    fn test_e2e_confidence_annotation() {
        let src = "fn verified_fn() @ pure @ cpu @ verified { return true }";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok { report, errors, .. } => {
                // 验证置信度出现在报告中
                assert!(
                    report.contains("@ verified"),
                    "report should show confidence @ verified, got: {}",
                    report
                );
                assert!(errors.is_empty(), "no errors for verified fn");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
    }

    #[test]
    fn test_e2e_llm_directive_expansion() {
        // @ llm 指令应生成骨架代码（模板匹配触发生成）
        let src = "fn sort_data(data) @ pure @ cpu @ llm(\"sort ascending\") { return data }";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok { program, .. } => {
                assert_eq!(program.statements.len(), 1, "one function");
                // llm_prompt 在扩展后应为 None（消费掉了）
                // body 应该被 LLM 生成的内容替换
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
    }

    #[test]
    fn test_e2e_latency_violation() {
        // f 声明 @latency(20ms) 但调用 g (50ms) → 超限
        let src = "\
fn g() @ latency(50ms) { return 1 }
fn f() @ latency(20ms) { return g() }";
        let result = compile_with_llm(src);
        match &result {
            CompileResult::Ok { errors, .. } => {
                assert!(!errors.is_empty(), "should report latency violation");
                let has_latency = errors
                    .iter()
                    .any(|e| matches!(e, ChannelError::LatencyViolation { .. }));
                assert!(has_latency, "at least one LatencyViolation error");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
        // Display 输出应包含延迟违规
        let display = format!("{}", result);
        assert!(
            display.contains("延迟违规") || display.contains("Latency"),
            "display should mention latency"
        );
    }

    #[test]
    fn test_e2e_syntax_error_returns_err() {
        let src = "fn broken( { return } ";
        let result = compile_with_llm(src);
        assert!(
            matches!(result, CompileResult::Err(_)),
            "broken syntax should return Err"
        );
    }

    #[test]
    fn test_e2e_empty_program() {
        let src = "";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok {
                program,
                specs,
                errors,
                report,
            } => {
                assert!(program.is_empty(), "empty program");
                assert!(specs.is_empty(), "no specs");
                assert!(errors.is_empty(), "no errors");
                assert!(report.contains("No type errors"), "clean report");
            }
            CompileResult::Err(e) => panic!("empty program should not fail: {}", e),
        }
    }

    #[test]
    fn test_e2e_async_fn_sugar() {
        let src = "async fn fetch(url) @ net { return url }";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok { specs, errors, .. } => {
                assert_eq!(specs.len(), 1, "one TaskSpec");
                assert_eq!(specs[0].fn_id, "fetch");
                assert!(errors.is_empty(), "valid async fn should have no errors");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
    }

    #[test]
    fn test_stdlib_loader_loads_all_modules() {
        use crate::stdlib_loader::StdLibLoader;
        use std::path::PathBuf;

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let mut loader = StdLibLoader::new(project_root)
            .expect("StdLibLoader should initialize with project root");

        let result = loader.load_all();
        assert!(
            result.is_ok(),
            "load_all() should succeed: {:?}",
            result.err()
        );
        let modules = result.unwrap();
        assert!(
            modules.len() >= 2,
            "Should load at least prelude and core_types, got {} modules: {:?}",
            modules.len(),
            modules
        );
        assert!(
            modules.contains(&"prelude".to_string()),
            "prelude should be loaded"
        );
        assert!(
            modules.contains(&"core_types".to_string()),
            "core_types should be loaded"
        );
    }
}
