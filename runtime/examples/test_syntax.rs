use dalin_runtime::interpreter::run_source;

fn main() {
    // Quick test: struct construction + field access + array indexing + basic math
    let src = r#"
struct MetricsPoint {
    cpu: float,
    mem: float,
    err: float
}

fn make_point(c, m, e) @ pure @ cpu {
    return MetricsPoint(c, m, e)
}

fn main() @ pure @ cpu {
    let p = make_point(65.0, 55.0, 2.0)
    let high = 90.0
    let is_high = p.cpu > high
    return 0
}
"#;
    match run_source(src) {
        Ok(results) => println!("Basic struct test OK: {:?}", results),
        Err(e) => println!("Basic struct test FAILED: {}", e),
    }
    
    // Test array indexing + loops + if/while + boolean logic
    let src2 = r#"
fn main() @ pure @ cpu {
    let arr = [1, 2, 3, 4, 5]
    let total = 0
    let i = 0
    while i < 5 {
        let v = arr[i]
        let total = total + v
        let i = i + 1
    }
    let half = total / 2
    return half
}
"#;
    match run_source(src2) {
        Ok(results) => println!("Array+loop test OK: {:?}", results),
        Err(e) => println!("Array+loop test FAILED: {}", e),
    }
    
    // Test match expression with enum variants
    let src3 = r#"
enum Color { Red, Green, Blue }

fn get_color_name(c) @ pure @ cpu {
    match c {
        Red => return 3
        Green => return 5
        Blue => return 4
        _ => return 0
    }
}

fn main() @ pure @ cpu {
    let color = Green
    let n = get_color_name(color)
    return n
}
"#;
    match run_source(src3) {
        Ok(results) => println!("Match+enum test OK: {:?}", results),
        Err(e) => println!("Match+enum test FAILED: {}", e),
    }
    
    // Test nested function calls (struct inside function call)
    let src4 = r#"
struct Point {
    x: float,
    y: float
}

fn make_point(px, py) @ pure @ cpu {
    return Point(px, py)
}

fn dist_sq(p) @ pure @ cpu {
    return p.x * p.x + p.y * p.y
}

fn main() @ pure @ cpu {
    let p = make_point(3.0, 4.0)
    let d = dist_sq(p)
    return int(d)
}
"#;
    match run_source(src4) {
        Ok(results) => println!("Nested call test OK: {:?}", results),
        Err(e) => println!("Nested call test FAILED: {}", e),
    }
    
    // Test len builtin on array
    let src5 = r#"
fn main() @ pure @ cpu {
    let items = [10, 20, 30, 40, 50]
    let n = len(items)
    return n
}
"#;
    match run_source(src5) {
        Ok(results) => println!("Len test OK: {:?}", results),
        Err(e) => println!("Len test FAILED: {}", e),
    }
    
    // Test if/else
    let src6 = r#"
fn sign_of(x) @ pure @ cpu {
    if x > 0 {
        return 1
    } else {
        if x < 0 {
            return -1
        } else {
            return 0
        }
    }
}

fn main() @ pure @ cpu {
    let s = sign_of(-42)
    return s
}
"#;
    match run_source(src6) {
        Ok(results) => println!("If/else test OK: {:?}", results),
        Err(e) => println!("If/else test FAILED: {}", e),
    }
    
    println!("\nAll syntax tests complete!");
}
