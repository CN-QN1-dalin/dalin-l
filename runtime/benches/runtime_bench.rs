use criterion::{Criterion, criterion_group, criterion_main};
use dalin_runtime::interpreter;

fn bench_interpreter(c: &mut Criterion) {
    c.bench_function("interpreter_fibonacci", |b| {
        b.iter(|| {
            let result = interpreter::run_source(
                "
                fn fib(n) @ pure @ cpu {
                    if n <= 1 { return n }
                    return fib(n - 1) + fib(n - 2)
                }
                let result = fib(15)
                ",
            );
            assert!(result.is_ok());
        });
    });

    c.bench_function("interpreter_simple_ops", |b| {
        b.iter(|| {
            let result = interpreter::run_source(
                "
                let a = 1 + 2 * 3
                let b = a - 4
                let c = b * 2
                ",
            );
            assert!(result.is_ok());
        });
    });
}

criterion_group!(benches, bench_interpreter);
criterion_main!(benches);
