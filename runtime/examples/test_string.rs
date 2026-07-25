use dalin_runtime::interpreter::run_source;

fn main() {
    // Test string concat in print  
    let src = r#"
fn main() @ pure @ cpu {
    let name = "Monitor"
    let header = "=== " + name + " ==="
    print(header)
    return 0
}
"#;
    match run_source(src) {
        Ok(results) => println!("String concat OK: {:?}", results),
        Err(e) => println!("String concat FAILED: {}", e),
    }
    
    // Test printing numbers via str builtin
    let src2 = r#"
fn main() @ pure @ cpu {
    let val = 42
    let s = str(val)
    let label = "Result: " + s
    print(label)
    return 0
}
"#;
    match run_source(src2) {
        Ok(results) => println!("str+concat OK: {:?}", results),
        Err(e) => println!("str+concat FAILED: {}", e),
    }
    
    // Test float output with precision via str
    let src3 = r#"
fn main() @ pure @ cpu {
    let x = 95.0
    let s = str(x)
    let msg = "CPU at " + s + "%"
    print(msg)
    return int(1)
}
"#;
    match run_source(src3) {
        Ok(results) => println!("Float display test OK: {:?}", results),
        Err(e) => println!("Float display test FAILED: {}", e),
    }
}
