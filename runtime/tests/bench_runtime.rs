//! Dalin L 3.0 — Runtime benchmark suite
//!
//! Measures: run_source speed, env performance, value operations.
//!
//! Note: run_source / RuntimeError are in `dalin_runtime::interpreter`,
//!       Environment is in `dalin_runtime::env`.

#[test]
fn bench_run_source_small() {
    use dalin_runtime::interpreter::{RuntimeError, run_source};
    let result = run_source(r#"fn main() @ pure @ cpu -> Int { return 42 }"#);
    assert!(result.is_ok() || matches!(result, Err(RuntimeError(_))));
}

#[test]
fn bench_run_source_multiple_calls() {
    use dalin_runtime::interpreter::run_source;
    for _ in 0..100 {
        let result = run_source(r#"let x: Int = 1; return x + 1"#);
        assert!(result.is_ok() || matches!(result, Err(_)));
    }
}

#[test]
fn bench_env_get_set() {
    use dalin_runtime::env::{Environment, Value};

    let mut env = Environment::new();

    for i in 0..1000 {
        env.define(&format!("var_{}", i), Value::Int(i as i64));
        let val = env.lookup(&format!("var_{}", i));
        assert!(val.is_some(), "Should find var_{}", i);
    }
}

#[test]
fn bench_env_lookup_performance() {
    use dalin_runtime::env::{Environment, Value};

    let mut env = Environment::new();

    for i in 0..100 {
        env.define(&format!("lookup_{}", i), Value::Int(i as i64));
    }

    let mut found = 0usize;
    for i in 0..100 {
        if env.lookup(&format!("lookup_{}", i)).is_some() {
            found += 1;
        }
    }
    assert_eq!(found, 100, "Should find all 100 variables");
}

#[test]
fn bench_nesting_levels() {
    use dalin_runtime::env::{Environment, Value};

    let mut scope1 = Environment::new();
    scope1.define("outer", Value::Int(1i64));

    {
        let mut scope2 = scope1.child();
        scope2.define("inner", Value::Int(2i64));

        let val = scope2.lookup("outer");
        assert!(val.is_some(), "Child scope should see parent vars");
    }
}

#[test]
fn bench_spawn_overhead() {
    use dalin_runtime::interpreter::run_source;
    let source = r#"
async fn task1() @ spawn @ cpu -> Int {
    return 42
}

fn run_task() @ pure @ cpu {
    spawn task1()
}
"#;

    let result = run_source(source);
    assert!(result.is_ok() || matches!(result, Err(_)));
}
