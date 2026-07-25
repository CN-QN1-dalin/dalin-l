use dalin_runtime::interpreter::run_source;

#[test]
fn test_devops_pipeline_demo() {
    let src = r#"
struct PipelineStage {
    name: string,
    status: string,
    duration_ms: float
}

fn run_lint(package) @ pure @ cpu {
    return PipelineStage("lint", "passed", 150.0)
}

fn run_tests(package) @ pure @ cpu {
    return PipelineStage("test", "passed", 500.0)
}

fn main() @ pure @ cpu {
    let s1 = run_lint("core")
    let s2 = run_tests("core")
    return s1.name + " -> " + s2.name
}
"#;
    let results = run_source(src).expect("DevOps pipeline demo should run");
    assert!(!results.is_empty(), "Should produce output");
}
