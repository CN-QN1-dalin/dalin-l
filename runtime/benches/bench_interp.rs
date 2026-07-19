use criterion::{Criterion, black_box, criterion_group, criterion_main};
use dalin_compiler::{lexer, parser};
use dalin_runtime::interpreter::Interpreter;

pub fn bench_interp_compile_and_run_simple(c: &mut Criterion) {
    let src = "fn add(a: int, b: int) -> int { return a + b; }";
    c.bench_function("interp_compile_and_run_simple", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse().unwrap();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_binary_ops(c: &mut Criterion) {
    let src = "\
fn compute(a: int, b: int) -> int {
    let x = a + b;
    let y = x * 2;
    let z = y - 5;
    return z / 3;
}";
    c.bench_function("interp_binary_ops", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse().unwrap();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_loops(c: &mut Criterion) {
    let src = "\
fn sum(n: int) -> int {
    let mut total = 0;
    let mut i = 0;
    while i < n {
        total = total + i;
        i = i + 1;
    }
    return total;
}";
    c.bench_function("interp_loops", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse().unwrap();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

criterion_group!(
    benches,
    bench_interp_compile_and_run_simple,
    bench_interp_binary_ops,
    bench_interp_loops
);
criterion_main!(benches);
