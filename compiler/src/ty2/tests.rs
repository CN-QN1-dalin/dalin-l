/// 七通道类型系统 — 测试套件
///
/// 从原 ty2.rs (2661 行) 中提取，按功能分组。

use super::super::ast::{Expr, Program, Stmt};
use super::*;

// Time constraint helper needs direct import since parse_time_constraint moved to submodule
use super::time_constraint::parse_time_constraint;

// ── Effect Channel ──

#[test]
fn test_effect_lattice() {
    assert!(Effect::Pure.leq(&Effect::Async));
    assert!(Effect::Pure.leq(&Effect::Spawn));
    assert!(Effect::Io.leq(&Effect::Async));
    assert!(!Effect::Async.leq(&Effect::Pure));
    assert!(!Effect::Spawn.leq(&Effect::Io));
}

#[test]
fn test_effect_join() {
    assert_eq!(Effect::join(&Effect::Pure, &Effect::Io), Some(Effect::Io));
    assert_eq!(Effect::join(&Effect::Io, &Effect::Async), Some(Effect::Async));
    assert_eq!(Effect::join(&Effect::Spawn, &Effect::Pure), Some(Effect::Spawn));
    assert_eq!(Effect::join(&Effect::Io, &Effect::Spawn), None);
    assert_eq!(Effect::join(&Effect::Async, &Effect::Spawn), None);
}

// ── Capability Channel ──

#[test]
fn test_capability_lattice() {
    assert!(Capability::Cpu.leq(&Capability::Gpu));
    assert!(Capability::Cpu.leq(&Capability::Sfa));
    assert!(!Capability::Gpu.leq(&Capability::Cpu));
    assert!(!Capability::Sfa.leq(&Capability::Gpu));
}

#[test]
fn test_capability_join() {
    assert_eq!(Capability::join(&Capability::Cpu, &Capability::Gpu), Some(Capability::Gpu));
    assert_eq!(Capability::join(&Capability::Cpu, &Capability::Sfa), Some(Capability::Sfa));
    assert_eq!(Capability::join(&Capability::Gpu, &Capability::Gpu), Some(Capability::Gpu));
    assert_eq!(Capability::join(&Capability::Gpu, &Capability::Sfa), None);
}

#[test]
fn test_capability_inference_wired() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "remote".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: Some("async".to_string()), capability: Some("net".to_string()), llm_prompt: None,
        confidence: None, cognitive_loop: None, governance: None, latency: None, timeout: None, throughput: None,
        body: vec![], async_: false, pub_: false,
    });
    prog.add(Stmt::Fn {
        name: "local".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: None, capability: Some("sfa".to_string()), llm_prompt: None,
        confidence: None, cognitive_loop: None, governance: None, latency: None, timeout: None, throughput: None,
        body: vec![], async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
    let remote = by_name.get("remote").expect("remote fn present");
    assert_eq!(remote.effect, Some(Effect::Async));
    assert_eq!(remote.capability, Some(Capability::Net));
    let local = by_name.get("local").expect("local fn present");
    assert_eq!(local.effect, Some(Effect::Pure));
    assert_eq!(local.capability, Some(Capability::Sfa));
}

// ── Confidence Channel ──

#[test]
fn test_confidence_lattice() {
    assert!(Confidence::Uncertain.leq(&Confidence::Generated));
    assert!(Confidence::Uncertain.leq(&Confidence::Proven));
    assert!(Confidence::Generated.leq(&Confidence::Inferred));
    assert!(Confidence::Verified.leq(&Confidence::Proven));
    assert!(!Confidence::Proven.leq(&Confidence::Verified));
}

#[test]
fn test_confidence_join() {
    assert_eq!(Confidence::join(&Confidence::Proven, &Confidence::Uncertain), Confidence::Uncertain);
    assert_eq!(Confidence::join(&Confidence::Generated, &Confidence::Inferred), Confidence::Generated);
    assert_eq!(Confidence::join(&Confidence::Verified, &Confidence::Proven), Confidence::Verified);
    assert_eq!(Confidence::join(&Confidence::Proven, &Confidence::Proven), Confidence::Proven);
}

#[test]
fn test_confidence_inference_literals() {
    let mut inf = ConfidenceInferencer::new();
    assert_eq!(inf.infer_expr(&Expr::IntLiteral(42)), Confidence::Proven);
    assert_eq!(inf.infer_expr(&Expr::StringLiteral("hello".into())), Confidence::Proven);
}

#[test]
fn test_confidence_inference_llm_call() {
    let mut inf = ConfidenceInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("llm_generate".into())), args: vec![Expr::StringLiteral("summarize".into())] };
    assert_eq!(inf.infer_expr(&expr), Confidence::Generated);
}

#[test]
fn test_confidence_inference_verify() {
    let mut inf = ConfidenceInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("verify".into())), args: vec![Expr::IntLiteral(42)] };
    assert_eq!(inf.infer_expr(&expr), Confidence::Verified);
}

#[test]
fn test_confidence_check_rejects_low() {
    let mut inf = ConfidenceInferencer::new();
    inf.check(&Confidence::Generated, &Confidence::Verified, "test_location");
    assert!(!inf.errors.is_empty());
    assert!(inf.errors[0].contains("置信度不足"));
}

#[test]
fn test_confidence_check_accepts_high() {
    let mut inf = ConfidenceInferencer::new();
    inf.check(&Confidence::Proven, &Confidence::Generated, "test_location");
    assert!(inf.errors.is_empty());
}

// ── Cognitive Loop Channel ──

#[test]
fn test_cognitive_loop_lattice() {
    assert!(CognitiveLoop::Perceive.leq(&CognitiveLoop::Reason));
    assert!(CognitiveLoop::Perceive.leq(&CognitiveLoop::Loop));
    assert!(CognitiveLoop::Reason.leq(&CognitiveLoop::Decide));
    assert!(CognitiveLoop::Decide.leq(&CognitiveLoop::Act));
    assert!(CognitiveLoop::Act.leq(&CognitiveLoop::Loop));
    assert!(CognitiveLoop::Loop.leq(&CognitiveLoop::Loop));
    assert!(!CognitiveLoop::Loop.leq(&CognitiveLoop::Act));
}

#[test]
fn test_cognitive_loop_join() {
    assert_eq!(CognitiveLoop::join(&CognitiveLoop::Perceive, &CognitiveLoop::Loop), CognitiveLoop::Loop);
    assert_eq!(CognitiveLoop::join(&CognitiveLoop::Reason, &CognitiveLoop::Decide), CognitiveLoop::Decide);
    assert_eq!(CognitiveLoop::join(&CognitiveLoop::Act, &CognitiveLoop::Perceive), CognitiveLoop::Act);
}

#[test]
fn test_cognitive_loop_infer_perceive() {
    let mut inf = CognitiveLoopInferencer::new();
    assert_eq!(inf.infer_expr(&Expr::IntLiteral(42)), CognitiveLoop::Perceive);
    assert_eq!(inf.infer_expr(&Expr::Ident("x".into())), CognitiveLoop::Perceive);
}

#[test]
fn test_cognitive_loop_infer_reason() {
    let mut inf = CognitiveLoopInferencer::new();
    let expr = Expr::BinaryOp { left: Box::new(Expr::IntLiteral(1)), op: "+".to_string(), right: Box::new(Expr::IntLiteral(2)) };
    assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Reason);
}

#[test]
fn test_cognitive_loop_infer_decide() {
    let mut inf = CognitiveLoopInferencer::new();
    let expr = Expr::IfExpr(Box::new(Expr::BoolLiteral(true)), Box::new(Expr::IntLiteral(1)), Box::new(Expr::IntLiteral(0)));
    assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Decide);
}

#[test]
fn test_cognitive_loop_infer_act() {
    let mut inf = CognitiveLoopInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("do_something".into())), args: vec![] };
    assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Act);
}

#[test]
fn test_cognitive_loop_infer_loop_sfa() {
    let mut inf = CognitiveLoopInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("sfa_encode".into())), args: vec![] };
    assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Loop);
}

#[test]
fn test_cognitive_loop_check_rejects() {
    let mut inf = CognitiveLoopInferencer::new();
    inf.check(&CognitiveLoop::Perceive, &CognitiveLoop::Act, "test");
    assert!(!inf.errors.is_empty());
    assert!(inf.errors[0].contains("认知循环违规"));
}

#[test]
fn test_cognitive_loop_check_accepts() {
    let mut inf = CognitiveLoopInferencer::new();
    inf.check(&CognitiveLoop::Loop, &CognitiveLoop::Reason, "test");
    assert!(inf.errors.is_empty());
}

// ── Governance Channel ──

#[test]
fn test_governance_lattice() {
    assert!(GovernanceLevel::Prepare.leq(&GovernanceLevel::Suggest));
    assert!(GovernanceLevel::Prepare.leq(&GovernanceLevel::Execute));
    assert!(GovernanceLevel::Suggest.leq(&GovernanceLevel::Approve));
    assert!(GovernanceLevel::Approve.leq(&GovernanceLevel::Execute));
    assert!(!GovernanceLevel::Execute.leq(&GovernanceLevel::Approve));
}

#[test]
fn test_governance_join() {
    assert_eq!(GovernanceLevel::join(&GovernanceLevel::Prepare, &GovernanceLevel::Execute), GovernanceLevel::Execute);
    assert_eq!(GovernanceLevel::join(&GovernanceLevel::Suggest, &GovernanceLevel::Approve), GovernanceLevel::Approve);
}

#[test]
fn test_governance_infer_prepare() {
    let mut inf = GovernanceInferencer::new();
    assert_eq!(inf.infer_expr(&Expr::IntLiteral(42)), GovernanceLevel::Prepare);
    assert_eq!(inf.infer_expr(&Expr::Ident("x".into())), GovernanceLevel::Prepare);
}

#[test]
fn test_governance_infer_suggest() {
    let mut inf = GovernanceInferencer::new();
    let expr = Expr::BinaryOp { left: Box::new(Expr::IntLiteral(1)), op: "+".to_string(), right: Box::new(Expr::IntLiteral(2)) };
    assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Suggest);
}

#[test]
fn test_governance_infer_approve() {
    let mut inf = GovernanceInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("charge".into())), args: vec![] };
    assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Approve);
}

#[test]
fn test_governance_infer_execute() {
    let mut inf = GovernanceInferencer::new();
    let expr = Expr::Call { func: Box::new(Expr::Ident("deploy".into())), args: vec![] };
    assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Execute);
}

#[test]
fn test_governance_check_rejects() {
    let mut inf = GovernanceInferencer::new();
    inf.check(&GovernanceLevel::Suggest, &GovernanceLevel::Execute, "test");
    assert!(!inf.errors.is_empty());
    assert!(inf.errors[0].contains("治理违规"));
}

#[test]
fn test_governance_check_accepts() {
    let mut inf = GovernanceInferencer::new();
    inf.check(&GovernanceLevel::Execute, &GovernanceLevel::Suggest, "test");
    assert!(inf.errors.is_empty());
}

// ── Time Constraint ──

#[test]
fn test_time_constraint_meet_both_some() {
    let a = TimeConstraint { latency_ms: Some(50), timeout_ms: Some(5000), throughput: Some(100) };
    let b = TimeConstraint { latency_ms: Some(100), timeout_ms: Some(3000), throughput: Some(200) };
    let m = TimeConstraint::meet(&a, &b);
    assert_eq!(m.latency_ms, Some(50));
    assert_eq!(m.timeout_ms, Some(3000));
    assert_eq!(m.throughput, Some(100));
}

#[test]
fn test_time_constraint_meet_none() {
    let a = TimeConstraint { latency_ms: Some(50), timeout_ms: None, throughput: None };
    let b = TimeConstraint { latency_ms: None, timeout_ms: Some(5000), throughput: None };
    let m = TimeConstraint::meet(&a, &b);
    assert_eq!(m.latency_ms, Some(50));
    assert_eq!(m.timeout_ms, Some(5000));
    assert_eq!(m.throughput, None);
}

#[test]
fn test_time_constraint_satisfies_latency() {
    let actual = TimeConstraint { latency_ms: Some(30), timeout_ms: None, throughput: None };
    let required = TimeConstraint { latency_ms: Some(50), ..TimeConstraint::new() };
    assert!(actual.satisfies(&required));
    let actual2 = TimeConstraint { latency_ms: Some(60), ..actual.clone() };
    assert!(!actual2.satisfies(&required));
}

#[test]
fn test_time_constraint_satisfies_throughput() {
    let actual = TimeConstraint { throughput: Some(200), ..TimeConstraint::new() };
    let required = TimeConstraint { throughput: Some(100), ..TimeConstraint::new() };
    assert!(actual.satisfies(&required));
    let actual2 = TimeConstraint { throughput: Some(50), ..actual.clone() };
    assert!(!actual2.satisfies(&required));
}

#[test]
fn test_parse_time_latency() {
    let tc = parse_time_constraint("latency", "50ms");
    assert_eq!(tc.latency_ms, Some(50));
}

#[test]
fn test_parse_time_timeout_seconds() {
    let tc = parse_time_constraint("timeout", "5s");
    assert_eq!(tc.timeout_ms, Some(5000));
}

#[test]
fn test_parse_time_throughput() {
    let tc = parse_time_constraint("throughput", "100/s");
    assert_eq!(tc.throughput, Some(100));
}

#[test]
fn test_time_constraint_inferencer_check() {
    let mut inf = TimeConstraintInferencer::new();
    let actual = TimeConstraint { latency_ms: Some(60), ..TimeConstraint::new() };
    let required = TimeConstraint { latency_ms: Some(50), ..TimeConstraint::new() };
    inf.check(&actual, &required, "test_fn");
    assert!(!inf.errors.is_empty());
    assert!(inf.errors[0].contains("时间约束违规"));
}

// ── Annotation Parsing ──

fn parse_fn_annotations(src: &str) -> (Option<String>, Option<String>) {
    use super::super::lexer::Lexer;
    use super::super::parser::Parser;
    let mut lex = Lexer::new(src);
    let toks = lex.tokenize().expect("lex ok");
    let prog = Parser::new(toks).parse();
    for stmt in &prog.statements {
        if let Stmt::Fn { effect, capability, .. } = stmt {
            return (effect.clone(), capability.clone());
        }
    }
    panic!("no fn found in source: {}", src);
}

fn parse_fn_all_annotations(src: &str) -> (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) {
    use super::super::lexer::Lexer;
    use super::super::parser::Parser;
    let mut lex = Lexer::new(src);
    let toks = lex.tokenize().expect("lex ok");
    let prog = Parser::new(toks).parse();
    for stmt in &prog.statements {
        if let Stmt::Fn { effect, capability, cognitive_loop, governance, latency, timeout, throughput, .. } = stmt {
            return (effect.clone(), capability.clone(), cognitive_loop.clone(), governance.clone(), latency.clone(), timeout.clone(), throughput.clone());
        }
    }
    panic!("no fn found in source: {}", src);
}

#[test]
fn test_single_annotation_tagged_as_capability() {
    let (effect, cap) = parse_fn_annotations("fn encode(x) @ sfa { return x }");
    assert_eq!(effect, None);
    assert_eq!(cap, Some("sfa".to_string()));
}

#[test]
fn test_async_fn_sugar_with_capability_annotation() {
    let (effect, cap) = parse_fn_annotations("async fn fetch(url) @ net { return \"x\" }");
    assert_eq!(effect, Some("async".to_string()));
    assert_eq!(cap, Some("net".to_string()));
}

#[test]
fn test_reversed_order_annotations() {
    let (effect, cap) = parse_fn_annotations("fn f(x) @ sfa @ async { return x }");
    assert_eq!(effect, Some("async".to_string()));
    assert_eq!(cap, Some("sfa".to_string()));
}

#[test]
fn test_cognitive_loop_annotation_parsed() {
    let (_, _, cl, gov, _, _, _) = parse_fn_all_annotations("fn f(x) @ perceive { return x }");
    assert_eq!(cl, Some("perceive".to_string()));
    assert_eq!(gov, None);
}

#[test]
fn test_governance_annotation_parsed() {
    let (_, _, _, gov, _, _, _) = parse_fn_all_annotations("fn f(x) @ gov(approve) { return x }");
    assert_eq!(gov, Some("approve".to_string()));
}

#[test]
fn test_mixed_cognitive_and_governance() {
    let (_, _, cl, gov, _, _, _) = parse_fn_all_annotations("fn f(x) @ decide @ gov(execute) @ io { return x }");
    assert_eq!(cl, Some("decide".to_string()));
    assert_eq!(gov, Some("execute".to_string()));
}

#[test]
fn test_cognitive_loop_with_llm_and_effect() {
    let (effect, _, cl, _, _, _, _) = parse_fn_all_annotations("fn f(x) @ reason @ pure @ llm(\"analyze data\") { return x }");
    assert_eq!(cl, Some("reason".to_string()));
    assert_eq!(effect, Some("pure".to_string()));
}

#[test]
fn test_governance_rejects_invalid_level() {
    use super::super::lexer::Lexer;
    use super::super::parser::Parser;
    let mut lex = Lexer::new("fn f(x) @ gov(invalid) { return x }");
    let toks = lex.tokenize().expect("lex ok");
    let mut parser = Parser::new(toks);
    let _prog = parser.parse();
    let recovered = parser.recovered();
    assert!(!recovered.is_empty(), "should have at least one parse error");
    let found = recovered.iter().any(|e| e.message.contains("Unknown governance level"));
    assert!(found, "parse error should mention 'Unknown governance level', got: {:?}", recovered);
}

#[test]
fn test_parser_latency_annotation() {
    let (_, _, _, _, latency, timeout, throughput) = parse_fn_all_annotations("fn f(x) @ latency(50ms) { return x }");
    assert_eq!(latency, Some("50ms".to_string()));
    assert_eq!(timeout, None);
    assert_eq!(throughput, None);
}

#[test]
fn test_parser_all_time_annotations() {
    let (_, _, _, _, latency, timeout, throughput) = parse_fn_all_annotations("fn f(x) @ latency(30ms) @ timeout(5s) @ throughput(100/s) { return x }");
    assert_eq!(latency, Some("30ms".to_string()));
    assert_eq!(timeout, Some("5s".to_string()));
    assert_eq!(throughput, Some("100/s".to_string()));
}

#[test]
fn test_parser_time_with_effect_and_capability() {
    let (effect, cap, _, _, latency, _, _) = parse_fn_all_annotations("fn f(x) @ io @ sfa @ latency(50ms) { return x }");
    assert_eq!(effect, Some("io".to_string()));
    assert_eq!(cap, Some("sfa".to_string()));
    assert_eq!(latency, Some("50ms".to_string()));
}

// ── Body-level Violation Detection ──

#[allow(dead_code)] // helper for body-level violation tests
fn make_body_with_call(called_fn: &str) -> Vec<Stmt> {
    vec![Stmt::Expr(Box::new(Expr::Call { func: Box::new(Expr::Ident(called_fn.to_string())), args: vec![] }))]
}

#[test]
fn test_body_walk_detects_effect_violation() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "bad".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: Some("pure".to_string()), capability: None, llm_prompt: None, confidence: None,
        cognitive_loop: None, governance: None, latency: None, timeout: None, throughput: None,
        body: vec![Stmt::Expr(Box::new(Expr::Call { func: Box::new(Expr::Ident("println".to_string())), args: vec![Expr::StringLiteral("hello".to_string())] }))],
        async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    assert!(!inf.effect.errors.is_empty(), "Expected effect violation: pure fn body calls println (IO)");
}

#[test]
fn test_body_walk_detects_cognitive_loop_violation() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "bad_cl".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: None, capability: None, llm_prompt: None, confidence: None,
        cognitive_loop: Some("perceive".to_string()), governance: None, latency: None, timeout: None, throughput: None,
        body: vec![Stmt::Expr(Box::new(Expr::Call { func: Box::new(Expr::Ident("do_something".to_string())), args: vec![] }))],
        async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    assert!(!inf.cognitive_loop.errors.is_empty(), "Expected cognitive loop violation");
}

#[test]
fn test_body_walk_detects_governance_violation() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "bad_gov".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: None, capability: None, llm_prompt: None, confidence: None,
        cognitive_loop: None, governance: Some("prepare".to_string()),
        latency: None, timeout: None, throughput: None,
        body: vec![Stmt::Expr(Box::new(Expr::Call { func: Box::new(Expr::Ident("deploy".to_string())), args: vec![] }))],
        async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    assert!(!inf.governance.errors.is_empty(), "Expected governance violation");
}

#[test]
fn test_six_channel_inference_from_ast() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "sensor_read".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: Some("io".to_string()), capability: Some("cpu".to_string()), llm_prompt: None, confidence: None,
        cognitive_loop: Some("perceive".to_string()), governance: Some("prepare".to_string()),
        latency: None, timeout: None, throughput: None, body: vec![], async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
    let sensor = by_name.get("sensor_read").expect("sensor_read present");
    assert_eq!(sensor.effect, Some(Effect::Io));
    assert_eq!(sensor.capability, Some(Capability::Cpu));
    assert_eq!(sensor.cognitive_loop, Some(CognitiveLoop::Perceive));
    assert_eq!(sensor.governance, Some(GovernanceLevel::Prepare));
}

#[test]
fn test_six_channel_with_governance_execute() {
    let mut prog = Program::new();
    prog.add(Stmt::Fn {
        name: "deploy_fn".to_string(), type_params: vec![], params: vec![], return_type: None,
        effect: Some("spawn".to_string()), capability: Some("net".to_string()), llm_prompt: None, confidence: None,
        cognitive_loop: Some("act".to_string()), governance: Some("execute".to_string()),
        latency: None, timeout: None, throughput: None, body: vec![], async_: false, pub_: false,
    });
    let mut inf = SevenChannelInferencer::new();
    inf.infer_program(&prog);
    let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
    let deploy = by_name.get("deploy_fn").expect("deploy_fn present");
    assert_eq!(deploy.effect, Some(Effect::Spawn));
    assert_eq!(deploy.capability, Some(Capability::Net));
    assert_eq!(deploy.cognitive_loop, Some(CognitiveLoop::Act));
    assert_eq!(deploy.governance, Some(GovernanceLevel::Execute));
}

#[test]
fn test_confidence_annotation_parsed() {
    let (effect, cap, _, _, _, _, _) = parse_fn_all_annotations("fn f(x) @ proven { return x }");
    assert_eq!(effect, None);
    assert_eq!(cap, None);
}

#[test]
fn test_confidence_annotation_with_pure() {
    let (effect, cap, _, _, _, _, _) = parse_fn_all_annotations("fn f(x) @ proven @ pure { return x }");
    assert_eq!(effect, Some("pure".to_string()));
    assert_eq!(cap, None);
}
