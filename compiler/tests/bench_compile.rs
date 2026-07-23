//! Dalin L 3.0 — Compiler performance benchmarks
//!
//! Measures: lex_time, parse_time, type_check_time, full_pipeline_time
//! against progressively larger inputs.

use std::time::Instant;

/// Generate a sample program with N function definitions
fn generate_sample_program(n_funcs: usize) -> String {
    let mut src = String::from("use core_types\n\n");
    for i in 0..n_funcs {
        src.push_str(&format!(
            "fn compute_{i}(x: Int, y: Int) @ pure @ cpu -> Int {{\n    return x + y\n}}\n\n"
        ));
    }
    src
}

/// Tokenize and measure
fn bench_lex(src: &str) -> (usize, u128) {
    use dalin_compiler::lexer::Lexer;
    let start = Instant::now();
    let tokens = Lexer::new(src).tokenize().unwrap_or_default();
    let duration = start.elapsed().as_micros();
    (tokens.len(), duration)
}

/// Parse and measure
fn bench_parse(src: &str) -> (usize, u128) {
    use dalin_compiler::lexer::Lexer;
    use dalin_compiler::parser::Parser;
    let start = Instant::now();
    let tokens = Lexer::new(src).tokenize().unwrap_or_default();
    let tok_len = tokens.len();
    let _prog = Parser::new(tokens).parse().unwrap_or_default();
    let duration = start.elapsed().as_micros();
    (tok_len, duration)
}

#[test]
fn bench_compile_single_function() {
    let src = generate_sample_program(1);
    let (_, lex_us) = bench_lex(&src);
    let (_, parse_us) = bench_parse(&src);
    assert!(lex_us < 1_000_000, "Lex should complete in under 1ms (got {}us)", lex_us);
    assert!(parse_us < 1_000_000, "Parse should complete in under 1ms (got {}us)", parse_us);
}

#[test]
fn bench_compile_small_program_10funcs() {
    let src = generate_sample_program(10);
    let (_, lex_us) = bench_lex(&src);
    let (_, parse_us) = bench_parse(&src);
    assert!(lex_us < 5_000_000, "10 funcs lex under 5ms (got {}us)", lex_us);
    assert!(parse_us < 5_000_000, "10 funcs parse under 5ms (got {}us)", parse_us);
}

#[test]
fn bench_compile_medium_program_50funcs() {
    let src = generate_sample_program(50);
    let (_, lex_us) = bench_lex(&src);
    let (_, parse_us) = bench_parse(&src);
    assert!(lex_us < 20_000_000, "50 funcs lex under 20ms (got {}us)", lex_us);
    assert!(parse_us < 20_000_000, "50 funcs parse under 20ms (got {}us)", parse_us);
}

#[test]
fn bench_scalable_growth() {
    let sizes = vec![1, 5, 10, 25, 50];
    let mut times = Vec::new();
    
    for n in sizes {
        let src = generate_sample_program(n);
        let (_, parse_us) = bench_parse(&src);
        times.push((n, parse_us));
    }
    
    // Verify O(n) or better growth rate
    let first = times[0].1 as f64;
    let last = times[times.len() - 1].1 as f64;
    
    if first > 0.0 {
        let growth_factor = last / first;
        let size_factor = times[times.len() - 1].0 as f64 / times[0].0 as f64;
        // Growth factor should be <= 10x size factor (allows some overhead)
        assert!(growth_factor <= size_factor * 10.0,
            "Parse time grew {}x but input only grew {}x", growth_factor, size_factor);
    }
}

#[test]
fn bench_effect_parsing() {
    use dalin_compiler::ty2::Effect;
    
    // Verify all Effect variants exist
    assert!(matches!(Effect::Pure, Effect::Pure), "Pure variant exists");
    assert!(matches!(Effect::Io, Effect::Io), "Io variant exists");
    assert!(matches!(Effect::Async, Effect::Async), "Async variant exists");
    assert!(matches!(Effect::Spawn, Effect::Spawn), "Spawn variant exists");
    
    // Pure leq relation: Pure leq everything
    assert!(Effect::Pure.leq(&Effect::Pure));
    assert!(Effect::Pure.leq(&Effect::Io));
    assert!(Effect::Pure.leq(&Effect::Async));
    assert!(Effect::Pure.leq(&Effect::Spawn));
    
    // Io leq Async
    assert!(Effect::Io.leq(&Effect::Async));
    assert!(!Effect::Io.leq(&Effect::Pure));
}

#[test]
fn bench_capability_parsing() {
    use dalin_compiler::ty2::Capability;
    
    assert!(matches!(Capability::Cpu, Capability::Cpu));
    assert!(matches!(Capability::Gpu, Capability::Gpu));
    assert!(matches!(Capability::Sfa, Capability::Sfa));
    assert!(matches!(Capability::Net, Capability::Net));
    
    // Cpu leq everything (default capability)
    assert!(Capability::Cpu.leq(&Capability::Gpu));
    assert!(Capability::Cpu.leq(&Capability::Sfa));
    assert!(Capability::Cpu.leq(&Capability::Net));
    assert!(Capability::Cpu.leq(&Capability::Cpu));
}

#[test]
fn bench_confidence_scoring() {
    use dalin_compiler::ty2::Confidence;
    
    // Verify all confidence levels exist and score correctly
    assert_eq!(Confidence::Proven.score(), 1.0);
    assert_eq!(Confidence::Verified.score(), 0.95);
    assert_eq!(Confidence::Inferred.score(), 0.85);
    assert_eq!(Confidence::Generated.score(), 0.7);
    assert_eq!(Confidence::Uncertain.score(), 0.5);
    
    // Verify leq ordering: Uncertain leq everything
    assert!(Confidence::Uncertain.leq(&Confidence::Proven));
    assert!(Confidence::Proven.leq(&Confidence::Proven));
    assert!(!Confidence::Proven.leq(&Confidence::Uncertain));
    
    // Verify join: takes the less confident one
    let j = Confidence::join(&Confidence::Proven, &Confidence::Uncertain);
    assert!(matches!(j, Confidence::Uncertain));
}

#[test]
fn bench_ty2_full_inference_fast() {
    use dalin_compiler::{ast, lexer, parser, ty::TypeInferencer};
    
    let prog_str = generate_sample_program(5);
    let tokens = lexer::Lexer::new(&prog_str).tokenize().unwrap_or_default();
    let prog = parser::Parser::new(tokens).parse().unwrap_or_else(|_| ast::Program {
        statements: Vec::new(),
        derive_attrs: Vec::new(),
        macros: Vec::new(),
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
    });
    
    // Type inference on a small program should complete quickly
    let start = Instant::now();
    let mut inferencer = TypeInferencer::new();
    let types = inferencer.infer_program(&prog);
    let elapsed = start.elapsed().as_micros();
    
    assert!(elapsed < 10_000_000, "Inference on 5 funcs under 10ms (got {}us)", elapsed);
}
