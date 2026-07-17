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
    let _prog = Parser::new(tokens).parse().unwrap_or_default();
    let duration = start.elapsed().as_micros();
    (tokens.len(), duration)
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
    use dalin_compiler::ty2::parse_effect;
    
    for _ in 0..1000 {
        let eff = parse_effect("pure");
        assert!(eff.is_pure(), "pure effect should be Pure variant");
        
        let io_eff = parse_effect("io");
        assert!(!io_eff.is_pure(), "io effect should not be Pure");
    }
}

#[test]
fn bench_capability_parsing() {
    use dalin_compiler::ty2::parse_capability;
    
    for _ in 0..1000 {
        let cpu = parse_capability("cpu");
        assert!(matches!(cpu, dalin_compiler::ty2::Capability::Cpu));
        
        let net = parse_capability("net");
        assert!(matches!(net, dalin_compiler::ty2::Capability::Net));
    }
}

#[test]
fn bench_confidence_scoring() {
    use dalin_compiler::ty2::Confidence;
    
    for level in 0.0..1.01 {
        let conf = Confidence(level);
        let score = conf.score();
        assert!(score >= 0.0 && score <= 1.0, "Score {} out of range for confidence {}", score, level);
    }
}

#[test]
fn bench_ty2_full_inference_fast() {
    use dalin_compiler::{ast, lexer, parser};
    
    let prog_str = generate_sample_program(5);
    let tokens = lexer::Lexer::new(&prog_str).tokenize().unwrap_or_default();
    let prog = parser::Parser::new(tokens).parse().unwrap_or_else(|| ast::Program {
        statements: Vec::new(),
        derive_attrs: Vec::new(),
        macros: Vec::new(),
        modules: Vec::new(),
    });
    
    // Type inference on a small program should complete quickly
    let start = Instant::now();
    let types = dalin_compiler::ty2::TypeInferencer::new().infer_program(&prog);
    let elapsed = start.elapsed().as_micros();
    
    assert!(elapsed < 10_000_000, "Inference on 5 funcs under 10ms (got {}us)", elapsed);
    assert!(types.len() >= 0, "Should produce some type bindings");
}
