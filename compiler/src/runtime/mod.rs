/// Dalin L 3.0 — Phase E Runtime Execution Engine (mod entry point)
///
/// Modular decomposition of the original 2474-line runtime.rs:
/// - value.rs      : RuntimeValue, RuntimeError, RuntimeResult
/// - env.rs        : FnDef, Environment (scope + function registry)
/// - scheduler.rs  : CognitiveLoopPhase/Machine, GovernanceChecker, TimeMonitor
/// - engine.rs     : Runtime struct + impl, run_compiled, RuntimeEvent
/// - healing.rs    : SelfHealingRuntime, ConfidenceCalibrator, RuntimeSelfEvolution
// Module declarations
pub mod value;
pub mod env;
pub mod scheduler;
pub mod engine;
pub mod healing;

// Re-export all public items for downstream crates
pub use self::value::*;
pub use self::env::*;
pub use self::scheduler::*;
pub use self::engine::*;
pub use self::healing::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, FnParam, Program, Stmt};
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::ty2::GovernanceLevel;

    // ── Test Helpers ──

    fn int_expr(v: i64) -> Expr {
        Expr::IntLiteral(v)
    }

    fn int_value(v: i64) -> RuntimeValue {
        RuntimeValue::Int(v)
    }

    fn ident_expr(name: &str) -> Expr {
        Expr::Ident(name.to_string())
    }

    fn binop(left: Expr, op: &str, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: op.to_string(),
            right: Box::new(right),
        }
    }

    fn simple_fn(
        name: &str,
        params: Vec<&str>,
        body: Vec<Stmt>,
        effect: Option<&str>,
        capability: Option<&str>,
    ) -> Stmt {
        Stmt::Fn {
            name: name.to_string(),
            type_params: vec![],
            params: params
                .into_iter()
                .map(|p| FnParam {
                    name: p.to_string(),
                    type_annotation: None,
                    default: None,
                })
                .collect(),
            return_type: None,
            effect: effect.map(|s| s.to_string()),
            capability: capability.map(|s| s.to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body,
            async_: false,
            pub_: false,
        }
    }

    fn parse(src: &str) -> Program {
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().expect("lex failed");
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    // ── Core Expression Tests ──

    #[test]
    fn test_eval_int_literal() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        assert_eq!(rt.eval_expr(&int_expr(42)).unwrap(), RuntimeValue::Int(42));
    }

    #[test]
    fn test_eval_binary_arith() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        assert_eq!(
            rt.eval_expr(&binop(int_expr(3), "+", int_expr(4))).unwrap(),
            RuntimeValue::Int(7)
        );
        assert_eq!(
            rt.eval_expr(&binop(int_expr(10), "-", int_expr(3)))
                .unwrap(),
            RuntimeValue::Int(7)
        );
        assert_eq!(
            rt.eval_expr(&binop(int_expr(6), "*", int_expr(7))).unwrap(),
            RuntimeValue::Int(42)
        );
        assert_eq!(
            rt.eval_expr(&binop(int_expr(10), "/", int_expr(2)))
                .unwrap(),
            RuntimeValue::Int(5)
        );
    }

    #[test]
    fn test_eval_comparison() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        assert_eq!(
            rt.eval_expr(&binop(int_expr(3), "<", int_expr(4))).unwrap(),
            RuntimeValue::Bool(true)
        );
        assert_eq!(
            rt.eval_expr(&binop(int_expr(5), ">", int_expr(3))).unwrap(),
            RuntimeValue::Bool(true)
        );
        assert_eq!(
            rt.eval_expr(&binop(int_expr(3), "==", int_expr(3)))
                .unwrap(),
            RuntimeValue::Bool(true)
        );
    }

    #[test]
    fn test_eval_ident_undefined() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        let result = rt.eval_expr(&ident_expr("undefined_var"));
        assert!(matches!(result, Err(RuntimeError::UndefinedVariable(_))));
    }

    // ── Let & Return Tests ──

    #[test]
    fn test_let_and_return() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![
                Stmt::Let {
                    name: "x".to_string(),
                    value: Some(Box::new(int_expr(42))),
                    type_annotation: None,
                    mutable: false,
                },
                Stmt::Return(Some(Box::new(ident_expr("x")))),
            ],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);

        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("main", &[]).unwrap();
        assert_eq!(result, RuntimeValue::Int(42));
    }

    // ── If/Else Tests ──

    #[test]
    fn test_if_true() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![Stmt::If {
                condition: Box::new(Expr::BoolLiteral(true)),
                then_body: vec![Stmt::Return(Some(Box::new(int_expr(1))))],
                else_body: vec![Stmt::Return(Some(Box::new(int_expr(2))))],
            }],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);

        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(1));
    }

    // ── Function Call Tests ──

    #[test]
    fn test_fn_call_with_args() {
        let add_fn = simple_fn(
            "add",
            vec!["a", "b"],
            vec![Stmt::Return(Some(Box::new(binop(
                ident_expr("a"),
                "+",
                ident_expr("b"),
            ))))],
            None,
            None,
        );
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![Stmt::Return(Some(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("add".to_string())),
                args: vec![int_expr(3), int_expr(4)],
            })))],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(add_fn);
        prog.add(main_fn);

        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(7));
    }

    // ── While Loop Tests ──

    #[test]
    fn test_while_loop() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![Stmt::While {
                condition: Box::new(Expr::BoolLiteral(true)),
                body: vec![Stmt::Return(Some(Box::new(int_expr(42))))],
            }],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);

        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(42));
    }

    // ── For Loop Tests ──

    #[test]
    fn test_for_loop() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![
                Stmt::For {
                    target: "x".to_string(),
                    iterable: Box::new(Expr::Array(vec![int_expr(10), int_expr(20)])),
                    body: vec![Stmt::Return(Some(Box::new(ident_expr("x"))))],
                },
                Stmt::Return(Some(Box::new(int_expr(0)))),
            ],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);

        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(10));
    }

    // ── Cognitive Loop Tests ──

    #[test]
    fn test_cognitive_loop_advances_phases() {
        let src = "\
fn perceive_fn() @ pure @ cpu @ perceive { return 1 }
fn main() @ pure @ cpu @ decide { return perceive_fn() }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("main", &[]).unwrap();
        assert_eq!(result, RuntimeValue::Int(1));
        assert!(rt.cognitive.phase_history.len() >= 2);
        assert_eq!(rt.cognitive.phase_history[0].1, "main");
        assert_eq!(
            rt.cognitive.phase_history[0].0,
            CognitiveLoopPhase::Deciding
        );
        assert_eq!(rt.cognitive.phase_history[1].1, "perceive_fn");
        assert_eq!(
            rt.cognitive.phase_history[1].0,
            CognitiveLoopPhase::Perceiving
        );
    }

    // ── Governance Tests ──

    #[test]
    fn test_governance_permit_execute() {
        let src = "fn approve_fn() @ pure @ cpu @ gov(approve) { return 1 }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("approve_fn", &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_governance_deny_prepare_to_execute() {
        let src = "fn exec_fn() @ pure @ cpu @ gov(execute) { return 1 }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Prepare);
        rt.load_program(&prog);
        let result = rt.call("exec_fn", &[]);
        assert!(matches!(
            result,
            Err(RuntimeError::GovernanceViolation { .. })
        ));
    }

    // ── Time Constraint Tests ──

    #[test]
    fn test_time_monitor_records_timing() {
        let src = "fn main() @ pure @ cpu { return 42 }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        rt.call("main", &[]).unwrap();
        assert!(!rt.time_monitor.fn_timings.is_empty());
        assert_eq!(rt.time_monitor.fn_timings[0].0, "main");
    }

    // ── Assertion Tests ──

    #[test]
    fn test_assert_passes() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![Stmt::Assert {
                condition: Box::new(Expr::BoolLiteral(true)),
                message: None,
            }],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert!(rt.call("main", &[]).is_ok());
    }

    #[test]
    fn test_assert_fails() {
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![Stmt::Assert {
                condition: Box::new(Expr::BoolLiteral(false)),
                message: Some(Box::new(Expr::StringLiteral("assert msg".to_string()))),
            }],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(main_fn);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("main", &[]);
        assert!(matches!(result, Err(RuntimeError::AssertionFailed { .. })));
    }

    // ── Division By Zero ──

    #[test]
    fn test_division_by_zero() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        let result = rt.eval_expr(&binop(int_expr(5), "/", int_expr(0)));
        assert!(matches!(result, Err(RuntimeError::DivisionByZero)));
    }

    // ── TryCatch Tests ──

    #[test]
    fn test_try_catch_catches_error() {
        let panic_fn = simple_fn(
            "panic_fn",
            vec![],
            vec![Stmt::Assert {
                condition: Box::new(Expr::BoolLiteral(false)),
                message: None,
            }],
            None,
            None,
        );
        let call_panic = Stmt::Expr(Box::new(Expr::Call {
            func: Box::new(Expr::Ident("panic_fn".to_string())),
            args: vec![],
        }));
        let main_fn = simple_fn(
            "main",
            vec![],
            vec![
                Stmt::TryCatch {
                    try_body: vec![call_panic],
                    catch_param: Some("e".to_string()),
                    catch_body: vec![Stmt::Return(Some(Box::new(int_expr(1))))],
                },
                Stmt::Return(Some(Box::new(int_expr(0)))),
            ],
            None,
            None,
        );
        let mut prog = Program::new();
        prog.add(panic_fn);
        prog.add(main_fn);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(1));
    }

    // ── E2E: Full Pipeline → Runtime ──

    #[test]
    fn test_e2e_compile_and_run() {
        let src = "\
fn add(a, b) @ pure @ cpu { return a + b }
fn main() @ pure @ cpu { return add(40, 2) }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("main", &[]).unwrap();
        assert_eq!(result, RuntimeValue::Int(42));
        assert!(rt.events.len() >= 2);
        assert!(matches!(&rt.events[0], RuntimeEvent::FnCall { name, .. } if name == "main"));
    }

    #[test]
    fn test_e2e_cognitive_loop_with_governance() {
        let src = "\
fn sensor() @ io @ cpu @ perceive @ gov(prepare) @ latency(10ms) { return 42 }
fn main() @ pure @ cpu @ decide @ gov(approve) { return sensor() }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("main", &[]);
        assert!(
            result.is_ok(),
            "cognitive+governance should pass with Execute level"
        );
    }

    #[test]
    fn test_e2e_governance_blocked() {
        let src = "\
fn approve_action() @ gov(approve) { return 1 }
fn main() @ gov(suggest) { return approve_action() }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Suggest);
        rt.load_program(&prog);
        let result = rt.call("approve_action", &[]);
        assert!(
            matches!(result, Err(RuntimeError::GovernanceViolation { .. })),
            "Suggest session should not allow calling Approve fn"
        );
    }

    #[test]
    fn test_run_compiled_helper() {
        let src = "fn main() @ pure @ cpu { return 99 }";
        let prog = parse(src);
        let events = run_compiled(&prog, "main").unwrap();
        assert!(!events.is_empty());
        let has_main_return = events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::FnReturn { name, .. } if name == "main"));
        assert!(has_main_return, "should have main return event");
    }

    #[test]
    fn test_cognitive_phase_history() {
        let src = "\
fn sensor() @ perceive { return 1 }
fn reasoner() @ reason { return sensor() }
fn main() @ decide { return reasoner() }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        rt.call("main", &[]).unwrap();
        let phases: Vec<String> = rt
            .cognitive
            .phase_history
            .iter()
            .map(|(p, n, _)| format!("{}:{}", n, p))
            .collect();
        assert!(
            phases.iter().any(|p| p.contains("sensor:perceive")),
            "sensor in perceive phase"
        );
        assert!(
            phases.iter().any(|p| p.contains("reasoner:reason")),
            "reasoner in reason phase"
        );
    }

    #[test]
    fn test_multi_statement_block() {
        let src = "\
fn main() @ pure @ cpu {
    let x = 10
    let y = 32
    return x + y
}";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        assert_eq!(rt.call("main", &[]).unwrap(), RuntimeValue::Int(42));
    }

    #[test]
    fn test_latency_warning_does_not_block() {
        let src = "fn slow() @ latency(1ms) { return 42 }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);
        let result = rt.call("slow", &[]);
        assert_eq!(result.unwrap(), RuntimeValue::Int(42));
    }

    #[test]
    fn test_stack_overflow_protection() {
        let src = "fn main() @ pure @ cpu { return 1 }";
        let prog = parse(src);
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        rt.load_program(&prog);

        rt.current_depth = rt.max_depth;
        let result = rt.call("main", &[]);
        assert!(
            matches!(result, Err(RuntimeError::RuntimePanic(ref msg)) if msg.contains("stack overflow")),
            "Expected stack overflow error, got {:?}",
            result
        );
    }

    #[test]
    fn test_string_concat() {
        let mut rt = Runtime::new(GovernanceLevel::Execute);
        let result = rt.eval_expr(&Expr::BinaryOp {
            left: Box::new(Expr::StringLiteral("Hello, ".to_string())),
            op: "+".to_string(),
            right: Box::new(Expr::StringLiteral("World!".to_string())),
        });
        assert_eq!(
            result.unwrap(),
            RuntimeValue::String("Hello, World!".to_string())
        );
    }

    // ═══════════════════════════════════════════
    //  Phase G: Self-Healing Tests
    // ═══════════════════════════════════════════

    #[test]
    fn test_self_healing_success() {
        let src = "fn main() @ pure @ cpu { return 42 }";
        let prog = parse(src);
        let mut rt = SelfHealingRuntime::new(GovernanceLevel::Execute);
        rt.inner.load_program(&prog);
        let result = rt.call_with_healing("main", &[]).unwrap();
        assert_eq!(result, RuntimeValue::Int(42));
        assert_eq!(rt.recovery_count(), 0);
    }

    #[test]
    fn test_confidence_calibrator_adjusts_up() {
        let mut cal = ConfidenceCalibrator::new(0.05);
        cal.record_outcome("sort_data", 0.85, true);
        cal.record_outcome("sort_data", 0.85, true);
        cal.record_outcome("sort_data", 0.85, true);

        let confidence = cal.calibrated_confidence("sort_data");
        assert!((confidence - 1.0).abs() < 0.01);

        let stats = cal.stats("sort_data").unwrap();
        assert_eq!(stats.0, 3);
        assert!((stats.1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_confidence_calibrator_adjusts_down() {
        let mut cal = ConfidenceCalibrator::new(0.05);
        cal.record_outcome("complex_query", 0.95, true);
        cal.record_outcome("complex_query", 0.95, false);
        cal.record_outcome("complex_query", 0.95, false);

        let confidence = cal.calibrated_confidence("complex_query");
        assert!(confidence < 0.7);

        let stats = cal.stats("complex_query").unwrap();
        assert_eq!(stats.1, (1.0 / 3.0));
    }

    #[test]
    fn test_self_healing_recovers_from_division_by_zero() {
        let mut rt = SelfHealingRuntime::new(GovernanceLevel::Execute);
        rt.recovery_mode = RecoveryMode::RetryWithDefault;

        let _result = rt.call_with_healing("fake_fn", &[int_value(10), int_value(0)]);
        assert!(rt.recovery_log.is_empty());
    }

    #[test]
    fn test_evolution_mock_backend() {
        let mut evolution = RuntimeSelfEvolution::new_mock();
        let result = evolution.evolve("test_fn", "generate fibonacci function");

        assert!(!result.statements.is_empty());
        assert!(!evolution.evolution_log.is_empty());
        assert_eq!(evolution.evolution_log[0].fn_name, "test_fn");
    }
}
