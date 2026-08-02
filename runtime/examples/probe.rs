fn main() {
    let src = "fn main() @ pure @ cpu { return sqrt(16.0) }";
    match dalin_runtime::interpreter::run_source(src) {
        Ok(r) => println!("OK: {:?}", r),
        Err(e) => println!("ERR: {}", e),
    }
    // 带 main() 调用
    let src2 = "fn main() @ pure @ cpu { return sqrt(16.0) }\nmain()";
    match dalin_runtime::interpreter::run_source(src2) {
        Ok(r) => println!("OK2: {:?}", r),
        Err(e) => println!("ERR2: {}", e),
    }
}
