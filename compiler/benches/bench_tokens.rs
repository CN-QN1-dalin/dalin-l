use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dalin_compiler::{lexer, parser};

pub fn bench_lex_simple(c: &mut Criterion) {
    let src = "let x: int = 42; let y: string = \"hello\"; let z: bool = true;";
    c.bench_function("lex_simple", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let _ = lex.tokenize();
        })
    });
}

pub fn bench_lex_100_fns(c: &mut Criterion) {
    let mut src = String::new();
    for i in 0..100 {
        src.push_str(&format!("fn func_{}(a: int, b: int) -> int {{ return a + b; }}\n", i));
    }
    c.bench_function("lex_100_fns", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(&src));
            let _ = lex.tokenize();
        })
    });
}

pub fn bench_lex_throughput(c: &mut Criterion) {
    let src = "let x: int = 42; fn add(a: int, b: int) -> int { return a + b; }";
    c.bench_function("lex_throughput", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                let mut lex = lexer::Lexer::new(black_box(src));
                let _ = lex.tokenize();
            }
            start.elapsed()
        })
    });
}

pub fn bench_parse_simple(c: &mut Criterion) {
    let src = "fn add(a: int, b: int) -> int { a + b }";
    let mut lex = lexer::Lexer::new(src);
    let tokens = lex.tokenize().unwrap();
    c.bench_function("parse_simple", |b| {
        b.iter(|| {
            let mut pp = parser::Parser::new(black_box(tokens.clone()));
            let _ = pp.parse();
        })
    });
}

pub fn bench_parse_100_fns(c: &mut Criterion) {
    let mut src = String::new();
    for i in 0..100 {
        src.push_str(&format!("fn func_{}(a: int, b: int) -> int {{ return a + b; }}\n", i));
    }
    let mut lex = lexer::Lexer::new(&src);
    let tokens = lex.tokenize().unwrap();
    c.bench_function("parse_100_fns", |b| {
        b.iter(|| {
            let mut pp = parser::Parser::new(black_box(tokens.clone()));
            let _ = pp.parse();
        })
    });
}

criterion_group!(benches, bench_lex_simple, bench_lex_100_fns, bench_lex_throughput, bench_parse_simple, bench_parse_100_fns);
criterion_main!(benches);