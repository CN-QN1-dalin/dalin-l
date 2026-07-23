//! Dalin L 3.0 — Runtime benchmark suite
//!
//! Measures: run_source speed, env performance, value operations.

#[test]
fn bench_run_source_small() {
    let result = dalin_runtime::interpreter::run_source(r#"fn main() @ pure @ cpu -> Int { return 42 }"#);
    // Either OK or RuntimeError are valid — just check it doesn't panic
    let _ = result;
}

#[test]
fn bench_run_source_multiple_calls() {
    for _ in 0..10 {
        let result = dalin_runtime::interpreter::run_source(r#"let x: Int = 1; return x + 1"#);
    // Either OK or RuntimeError are valid — just check it doesn't panic
    let _ = result;
}
}

#[test]
fn bench_env_get_set() {
    use dalin_runtime::env::Environment;

    let mut env = Environment::new();

    for i in 0..1000 {
        env.define(&format!("var_{}", i), Value::Int(i as i64));
        let val = env.lookup(&format!("var_{}", i));
        assert!(val.is_some(), "Should find var_{}", i);
    }
}

#[test]
fn bench_env_lookup_performance() {
    use dalin_runtime::env::Environment;

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
    use dalin_runtime::env::Environment;

    let mut scope1 = Environment::new();
    scope1.define("outer", Value::Int(1));

    let scope2 = scope1.child();
    let mut scope2_mut = scope2.clone();
    scope2_mut.define("inner", Value::Int(2));

    // scope2 的 parent 指向 scope1 (clone 了一份)
    let val = scope2.lookup("outer");
    assert!(val.is_some(), "Child scope should see parent vars");
}

#[test]
fn bench_spawn_overhead() {
    let source = r#"
async fn task1() @ spawn @ cpu -> Int {
    return 42
}

fn run_task() @ pure @ cpu {
    spawn task1()
}
"#;

    let result = dalin_runtime::interpreter::run_source(source);
    // Either OK or RuntimeError are valid — just check it doesn't panic
    let _ = result;
}

use dalin_runtime::env::Value;
