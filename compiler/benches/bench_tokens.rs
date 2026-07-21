use criterion::{Criterion, black_box, criterion_group, criterion_main};
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
        src.push_str(&format!(
            "fn func_{}(a: int, b: int) -> int {{ return a + b; }}\n",
            i
        ));
    }
    c.bench_function("lex_100_fns", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(&src));
            let _ = lex.tokenize();
        })
    });
}

pub fn bench_lex_500_fns(c: &mut Criterion) {
    let mut src = String::new();
    for i in 0..500 {
        src.push_str(&format!(
            "fn func_{i}(a: int, b: float, c: string) -> int {{ return a + int(b); }}\n"
        ));
    }
    c.bench_function("lex_500_fns", |b| {
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
        src.push_str(&format!(
            "fn func_{}(a: int, b: int) -> int {{ return a + b; }}\n",
            i
        ));
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

pub fn bench_parse_nested_exprs(c: &mut Criterion) {
    // Deeply nested arithmetic: ((((1+2)+3)+4)+...+20)
    let mut src = String::from("1 + 2");
    for i in 3..=50 {
        src = format!("({src}) + {i}");
    }
    let src = format!("let x: int = {src};");
    let mut lex = lexer::Lexer::new(&src);
    let tokens = lex.tokenize().unwrap();
    c.bench_function("parse_nested_exprs", |b| {
        b.iter(|| {
            let mut pp = parser::Parser::new(black_box(tokens.clone()));
            let _ = pp.parse();
        })
    });
}

pub fn bench_parse_generic_fn(c: &mut Criterion) {
    let src = "fn map<T: Iterable, U>(data: array<T>, f: fn(T) -> U) @ pure @ cpu -> array<U> { \
               return map_inner(data, f); \
               }";
    let mut lex = lexer::Lexer::new(src);
    let tokens = lex.tokenize().unwrap();
    c.bench_function("parse_generic_fn", |b| {
        b.iter(|| {
            let mut pp = parser::Parser::new(black_box(tokens.clone()));
            let _ = pp.parse();
        })
    });
}

pub fn bench_lex_large_program(c: &mut Criterion) {
    // Simulate a realistic multi-function program
    let mut src = String::new();
    src.push_str("use std::math;\nuse std::collections;\n\n");
    for i in 0..200 {
        src.push_str(&format!(
            "fn process_{i}(input: array<int>) @ pure @ cpu @ proven -> array<int> {{\n\
             \x20   let mut result = [];\n\
             \x20   let n = len(input);\n\
             \x20   let mut idx = 0;\n\
             \x20   while idx < n {{\n\
             \x20       let val = input[idx];\n\
             \x20       if val > 0 {{ push(result, val * 2); }}\n\
             \x20       idx = idx + 1;\n\
             \x20   }}\n\
             \x20   return result;\n\
             }}\n\n"
        ));
    }
    c.bench_function("lex_large_program", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(&src));
            let _ = lex.tokenize();
        })
    });
}

criterion_group!(
    benches,
    bench_lex_simple,
    bench_lex_100_fns,
    bench_lex_500_fns,
    bench_lex_throughput,
    bench_lex_large_program,
    bench_parse_simple,
    bench_parse_100_fns,
    bench_parse_nested_exprs,
    bench_parse_generic_fn,
);
criterion_main!(benches);
