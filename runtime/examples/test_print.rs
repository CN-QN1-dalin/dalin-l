use dalin_runtime::interpreter::run_source;

fn main() {
    // Test print behavior - check if stdout actually flows through
    let src = r#"
fn main() @ pure @ cpu {
    print("Step 1")
    print("Step 2")
    let x = 42
    return x
}
"#;
    let results = run_source(src);
    println!("=== Test Result: {:?} ===", results);
}
