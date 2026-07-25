use dalin_runtime::interpreter::run_source;

fn main() {
    // Test: return from nested if inside function, check flow
    let src = r#"
fn abs_val(x) @ pure @ cpu {
    if x >= 0 {
        return x
    } else {
        let result = 0 - x
        return result
    }
}

fn main() @ pure @ cpu {
    let a = abs_val(-42)
    let b = abs_val(17)
    let sum = a + b
    return sum
}
"#;
    match run_source(src) {
        Ok(results) => println!("abs_val test: {:?}", results),
        Err(e) => println!("abs_val FAILED: {}", e),
    }
    
    // Test: if/else returns values in a function called from main
    let src2 = r#"
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
    let pos = sign_of(42)
    let neg = sign_of(-42)
    let zero = sign_of(0)
    return pos + neg + zero
}
"#;
    match run_source(src2) {
        Ok(results) => println!("Sign test: {:?}", results),
        Err(e) => println!("Sign FAILED: {}", e),
    }
    
    // Test: match expression with enum variants as return value
    let src3 = r#"
enum Color { Red, Green, Blue }

fn color_to_int(c) @ pure @ cpu {
    match c {
        Red => return 1
        Green => return 2
        Blue => return 3
    }
    return 0
}

fn main() @ pure @ cpu {
    let r = color_to_int(Red)
    let g = color_to_int(Green)
    let b = color_to_int(Blue)
    return r + g + b
}
"#;
    match run_source(src3) {
        Ok(results) => println!("Enum match test: {:?}", results),
        Err(e) => println!("Enum match FAILED: {}", e),
    }
    
    // Test: struct method chain (nested calls)
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
    let p1 = make_point(3.0, 4.0)
    let d = dist_sq(p1)
    return int(d)
}
"#;
    match run_source(src4) {
        Ok(results) => println!("Struct chain test: {:?}", results),
        Err(e) => println!("Struct chain FAILED: {}", e),
    }
    
    // Test: for loop over array
    let src5 = r#"
fn main() @ pure @ cpu {
    let nums = [1, 2, 3, 4, 5]
    let total = 0
    for n in nums {
        let total = total + int(n)
    }
    return total
}
"#;
    match run_source(src5) {
        Ok(results) => println!("For loop test: {:?}", results),
        Err(e) => println!("For loop FAILED: {}", e),
    }
}
