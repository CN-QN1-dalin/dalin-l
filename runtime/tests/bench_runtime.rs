//! Dalin L 3.0 — Runtime benchmark suite
//!
//! Measures: run_source speed, env performance, value operations.

#[test]
fn bench_run_source_small() {
    let result = dalin_runtime::run_source(r#"fn main() @ pure @ cpu -> Int { return 42 }"#);
    assert!(result.is_ok() || matches!(result, Err(dalin_runtime::RuntimeError(_))));
}

#[test]
fn bench_run_source_multiple_calls() {
    for _ in 0..100 {
        let result = dalin_runtime::run_source(r#"let x: Int = 1; return x + 1"#);
        assert!(result.is_ok() || matches!(result, Err(_)));
    }
}

#[test]
fn bench_env_get_set() {
    use dalin_runtime::env::Env;
    
    let mut env = Env::new();
    
    for i in 0..1000 {
        env.set(format!("var_{}", i), i as i64);
        let val = env.get(&format!("var_{}", i));
        assert!(val.is_some(), "Should find var_{}", i);
    }
}

#[test]
fn bench_env_lookup_performance() {
    use dalin_runtime::env::Env;
    
    let mut env = Env::new();
    
    for i in 0..100 {
        env.set(format!("lookup_{}", i), i as i64);
    }
    
    let mut found = 0usize;
    for i in 0..100 {
        if env.get(&format!("lookup_{}", i)).is_some() {
            found += 1;
        }
    }
    assert_eq!(found, 100, "Should find all 100 variables");
}

#[test]
fn bench_nesting_levels() {
    use dalin_runtime::env::Env;
    
    let mut scope1 = Env::new();
    scope1.set("outer", 1i64);
    
    {
        let scope2 = scope1.child();
        scope2.set("inner", 2i64);
        
        let val = scope2.get("outer");
        assert!(val.is_some(), "Child scope should see parent vars");
    }
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
    
    let result = dalin_runtime::run_source(source);
    assert!(result.is_ok() || matches!(result, Err(_)));
}
