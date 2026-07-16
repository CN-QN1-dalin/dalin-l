/// Dalin L 2.0 — 编译器工具链 crate
///
/// 把源码落到七通道类型系统的"可执行单元" (TaskSpec)：
/// token → lexer → parser → [llm_expand] → (ty2 七通道推断) → task_spec。
/// 纯编译期，无运行时并发依赖，可作为独立库被 runtime / control-plane 复用。
pub mod token;
pub mod ast;
pub mod lexer;
pub mod parser;
pub mod ty;
pub mod ty2;
pub mod task_spec;
pub mod error;
pub mod llm;
pub mod latency;
pub mod qn1;
pub mod runtime;
// Phase H: 模块/包系统 + 宏系统
pub mod module;
pub mod package;
pub mod macro_expand;
// Phase H+: 标准库加载器
pub mod stdlib_loader;
// Phase J: 自进化闭环
pub mod j1_pattern_learning;
pub mod j2_strategy_gen;
pub mod j3_evolution_verify;

use crate::ast::{Program, Stmt};
use crate::error::ChannelError;
use crate::task_spec::TaskSpec;
use crate::ty2::SevenChannelInferencer;

/// 完整的编译管线（含 @llm 扩展）：
///   token → lexer → parser → llm_expand → ty2 inference → task_spec
///
/// @llm 扩展阶段：扫描 AST 中所有 Stmt::Fn { llm_prompt: Some(prompt), .. }，
/// 调用 LlmEngine.process_directive() 生成函数体骨架，替换原 body。
pub fn compile_with_llm(src: &str) -> CompileResult {
    // Step 1: Lexer
    let mut lex = lexer::Lexer::new(src);
    let tokens = match lex.tokenize() {
        Ok(t) => t,
        Err(e) => return CompileResult::Err(format!("{}", e)),
    };

    // Step 2: Parser
    let mut parser = parser::Parser::new(tokens);
    let prog = match parser.parse() {
        Ok(p) => p,
        Err(e) => return CompileResult::Err(format!("{}", e)),
    };

    // Step 3: LLM 扩展
    let expanded = expand_llm(&prog);

    // Step 4: 七通道类型推断
    let mut infer = SevenChannelInferencer::new();
    infer.infer_program(&expanded);

    // Step 5: 延迟验证（Phase D — 时序契约）
    let latency_result = latency::LatencyVerifier::verify(&expanded);

    // Step 6: 生成 TaskSpec
    let specs = task_spec::from_program(&expanded);

    let mut report = infer.print_report();
    if !latency_result.errors.is_empty() {
        report.push_str("\n=== Latency Violations ===\n");
        for err in &latency_result.errors {
            report.push_str(&format!("  ❌ {}\n", err));
        }
    }

    CompileResult::Ok {
        program: expanded,
        report,
        specs,
        errors: latency_result.errors.iter().map(|e| ChannelError::LatencyViolation {
            location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
            declared_ms: 0, actual_ms: 0, detail: e.clone(),
        }).collect::<Vec<_>>(),
    }
}

/// 编译结果：AST + 报告 + TaskSpec + 结构化错误
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
            CompileResult::Err(e) => write!(f, "Compile error: {}", e),
            CompileResult::Ok { program, report, specs, errors } => {
                writeln!(f, "Compiled {} statements", program.statements.len())?;
                write!(f, "{}", report)?;
                for err in errors {
                    write!(f, "{}", err)?;
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

/// LLM 扩展：遍历 AST，遇到 llm_prompt=Some 的函数则调用 LlmEngine
fn expand_llm(prog: &Program) -> Program {
    let mut stmts = Vec::new();
    for stmt in &prog.statements {
        if let Stmt::Fn { name, params, return_type, effect, capability, llm_prompt, confidence: _, cognitive_loop, governance, latency, timeout, throughput, body: _, async_, pub_ } = stmt {
            if let Some(prompt) = llm_prompt.clone() {
                // 调用 LLM 引擎生成代码
                let r_gen = llm::LlmEngine::process_directive(&prompt, Some(name));
                // 如果生成的语句中有 Fn，提取其 body 作为当前函数的 body；否则用生成语句本身
                let new_body = if !r_gen.statements.is_empty() && matches!(&r_gen.statements[0], Stmt::Fn { .. }) {
                    match &r_gen.statements[0] {
                        Stmt::Fn { body, .. } => body.clone(),
                        _ => vec![],
                    }
                } else {
                    r_gen.statements
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
    Program { statements: stmts, modules: Vec::new(), uses: Vec::new(), package_manifest: None, macros: Vec::new(), derive_attrs: Vec::new() }
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
            CompileResult::Ok { program, report, specs, errors } => {
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
            CompileResult::Ok { program, specs, errors, report } => {
                assert_eq!(program.statements.len(), 1, "one function");
                assert_eq!(specs.len(), 1, "one TaskSpec");
                // 验证 TaskSpec 的正确保留
                assert_eq!(specs[0].fn_id, "sensor_read");
                // 验证报告包含所有通道
                assert!(report.contains("@ io"), "report shows effect");
                assert!(report.contains("@ cpu"), "report shows capability");
                assert!(report.contains("loop(perceive)"), "report shows cognitive loop");
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
                assert!(report.contains("@ verified"),
                    "report should show confidence @ verified, got: {}", report);
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
                let has_latency = errors.iter().any(|e| matches!(e, ChannelError::LatencyViolation { .. }));
                assert!(has_latency, "at least one LatencyViolation error");
            }
            CompileResult::Err(e) => panic!("compile failed: {}", e),
        }
        // Display 输出应包含延迟违规
        let display = format!("{}", result);
        assert!(display.contains("延迟违规") || display.contains("Latency"), "display should mention latency");
    }

    #[test]
    fn test_e2e_syntax_error_returns_err() {
        let src = "fn broken( { return } ";
        let result = compile_with_llm(src);
        assert!(matches!(result, CompileResult::Err(_)), "broken syntax should return Err");
    }

    #[test]
    fn test_e2e_empty_program() {
        let src = "";
        let result = compile_with_llm(src);
        match result {
            CompileResult::Ok { program, specs, errors, report } => {
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
}
