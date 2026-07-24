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
            let prog = p.parse();
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
            let prog = p.parse();
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
            let prog = p.parse();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_fib_recursive(c: &mut Criterion) {
    // Recursive fibonacci — stresses call stack and return machinery
    let src = "\
fn fib(n: int) -> int {
    if n <= 1 { return n; }
    return fib(n - 1) + fib(n - 2);
}";
    c.bench_function("interp_fib_recursive", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_deep_call_chain(c: &mut Criterion) {
    // Deep non-recursive call chain: f0→f1→f2→...→f9
    let mut src = String::new();
    for i in 0..10 {
        if i < 9 {
            src.push_str(&format!(
                "fn f{i}(x: int) -> int {{ return f{}(x + 1); }}\n",
                i + 1
            ));
        } else {
            src.push_str("fn f9(x: int) -> int { return x; }\n");
        }
    }
    src.push_str("fn main() -> int { return f0(0); }\n");
    c.bench_function("interp_deep_call_chain", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(&src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_array_ops(c: &mut Criterion) {
    // Array creation and iteration
    let src = "\
fn sum_arr(arr: array<int>) -> int {
    let mut total = 0;
    let mut i = 0;
    let n = len(arr);
    while i < n {
        total = total + arr[i];
        i = i + 1;
    }
    return total;
}";
    c.bench_function("interp_array_ops", |b| {
        b.iter(|| {
            let mut lex = lexer::Lexer::new(black_box(src));
            let tokens = lex.tokenize().unwrap();
            let mut p = parser::Parser::new(tokens);
            let prog = p.parse();
            let mut interp = Interpreter::new();
            let _ = interp.interpret(&prog);
        })
    });
}

pub fn bench_interp_compile_throughput(c: &mut Criterion) {
    // Measure raw compile+interpret throughput for many small programs
    let src = "fn add(a: int, b: int) -> int { return a + b; }";
    c.bench_function("interp_compile_throughput", |b| {
        b.iter_custom(|iters| {
            let start = std::time::Instant::now();
            for _ in 0..iters {
                let mut lex = lexer::Lexer::new(black_box(src));
                let tokens = lex.tokenize().unwrap();
                let mut p = parser::Parser::new(tokens);
                let prog = p.parse();
                let mut interp = Interpreter::new();
                let _ = interp.interpret(&prog);
            }
            start.elapsed()
        })
    });
}

criterion_group!(
    benches,
    bench_interp_compile_and_run_simple,
    bench_interp_binary_ops,
    bench_interp_loops,
    bench_interp_fib_recursive,
    bench_interp_deep_call_chain,
    bench_interp_array_ops,
    bench_interp_compile_throughput,
);
criterion_main!(benches);
