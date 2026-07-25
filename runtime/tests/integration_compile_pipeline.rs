//! Dalin L 3.0 — 端到端编译运行集成测试
//!
//! 覆盖完整链路：Lexer → Parser → TypeCheck → Interpret
//! 包括基础表达式、算术运算、控制流、函数调用、struct、内置函数

use dalin_runtime::interpreter::run_source;

// ══════════════════════════════════════════════════════════════
//  基础表达式
// ══════════════════════════════════════════════════════════════

#[test]
fn test_empty_program() {
    let src = "fn main() @ pure @ cpu { return 0 }";
    let result = run_source(src).expect("Empty program should run");
    assert_eq!(result.len(), 1);
}

#[test]
fn test_integer_literal() {
    let src = "fn main() @ pure @ cpu { return 42 }";
    let result = run_source(src).expect("Integer literal should run");
    assert!(!result.is_empty());
}

#[test]
fn test_float_literal() {
    let src = "fn main() @ pure @ cpu { return 3.14 }";
    let result = run_source(src).expect("Float literal should run");
    assert!(!result.is_empty());
}

#[test]
fn test_string_literal() {
    let src = "fn main() @ pure @ cpu { return \"hello\" }";
    let result = run_source(src).expect("String literal should run");
    assert!(!result.is_empty());
}

#[test]
fn test_bool_literal() {
    let src = "fn main() @ pure @ cpu { return true }";
    let result = run_source(src).expect("Bool literal should run");
    assert!(!result.is_empty());
}

#[test]
fn test_list_literal() {
    let src = "fn main() @ pure @ cpu { return [1, 2, 3] }";
    let result = run_source(src).expect("List literal should run");
    assert!(!result.is_empty());
}

// ══════════════════════════════════════════════════════════════
//  算术运算
// ══════════════════════════════════════════════════════════════

#[test]
fn test_integer_addition() {
    let src = "fn add(a, b) @ pure @ cpu { return a + b }
fn main() @ pure @ cpu { return add(2, 3) }";
    let result = run_source(src).expect("Addition should run");
    assert!(!result.is_empty());
}

#[test]
fn test_integer_subtraction() {
    let src = "fn sub(a, b) @ pure @ cpu { return a - b }
fn main() @ pure @ cpu { return sub(10, 4) }";
    let result = run_source(src).expect("Subtraction should run");
    assert!(!result.is_empty());
}

#[test]
fn test_float_multiplication() {
    let src = "fn mul(a, b) @ pure @ cpu { return a * b }
fn main() @ pure @ cpu { return mul(2.5, 4.0) }";
    let result = run_source(src).expect("Float multiplication should run");
    assert!(!result.is_empty());
}

// ══════════════════════════════════════════════════════════════
//  控制流
// ══════════════════════════════════════════════════════════════

#[test]
fn test_if_else_true() {
    let src = "fn main() @ pure @ cpu {
    if true { return 1 } else { return 0 }
    return -1
}";
    let result = run_source(src).expect("If-else should run");
    assert!(!result.is_empty());
}

#[test]
fn test_while_loop() {
    let src = "fn main() @ pure @ cpu {
    let i = 0
    let sum = 0
    while i < 5 {
        let sum = sum + i
        let i = i + 1
    }
    return sum
}";
    let result = run_source(src).expect("While loop should run");
    assert!(!result.is_empty());
}

#[test]
fn test_nested_if_else() {
    let src = "fn main() @ pure @ cpu {
    let x = 10
    if x > 5 {
        if x > 8 { return 1 } else { return 0 }
    } else { return -1 }
    return -2
}";
    let result = run_source(src).expect("Nested if-else should run");
    assert!(!result.is_empty());
}

// ══════════════════════════════════════════════════════════════
//  函数调用
// ══════════════════════════════════════════════════════════════

#[test]
fn test_simple_function_call() {
    let src = "fn greet(name) @ pure @ cpu { return name }
fn main() @ pure @ cpu { return greet(\"world\") }";
    let result = run_source(src).expect("Function call should run");
    assert!(!result.is_empty());
}

#[test]
fn test_multi_param_function() {
    let src = "fn add(x, y, z) @ pure @ cpu { return x + y + z }
fn main() @ pure @ cpu { return add(1, 2, 3) }";
    let result = run_source(src).expect("Multi-param function should run");
    assert!(!result.is_empty());
}

#[test]
fn test_nested_function_calls() {
    let src = "fn double(x) @ pure @ cpu { return x + x }
fn main() @ pure @ cpu { return double(double(3)) }";
    let result = run_source(src).expect("Nested function calls should run");
    assert!(!result.is_empty());
}

#[test]
fn test_fibonacci() {
    let src = "fn fib(n) @ pure @ cpu {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
fn main() @ pure @ cpu { return fib(10) }";
    let result = run_source(src).expect("Fibonacci should run");
    assert!(!result.is_empty());
}

// ══════════════════════════════════════════════════════════════
//  Struct
// ══════════════════════════════════════════════════════════════

#[test]
fn test_struct_definition_and_construction() {
    let src = "struct Point { x: int, y: int }
fn make_point(x, y) @ pure @ cpu { return Point(x, y) }
fn main() @ pure @ cpu { return make_point(3, 4) }";
    let result = run_source(src).expect("Struct should run");
    assert!(!result.is_empty());
}

#[test]
fn test_struct_field_access() {
    let src = "struct Point { x: int, y: int }
fn main() @ pure @ cpu {
    let p = Point(10, 20)
    return p.x
}";
    let result = run_source(src).expect("Struct field access should run");
    assert!(!result.is_empty());
}

// ══════════════════════════════════════════════════════════════
//  内置函数
// ══════════════════════════════════════════════════════════════

#[test]
fn test_builtin_len_array() {
    let src = "fn main() @ pure @ cpu { return len([1, 2, 3, 4, 5]) }";
    let result = run_source(src).expect("len on array should run");
    assert!(!result.is_empty());
}

#[test]
fn test_builtin_push() {
    let src = "fn main() @ pure @ cpu {
    let arr = [1, 2, 3]
    let arr = push(arr, 4)
    return len(arr)
}";
    let result = run_source(src).expect("push should run");
    assert!(!result.is_empty());
}

#[test]
fn test_builtin_print() {
    let src = "fn main() @ pure @ cpu { print(\"hello\"); return 0 }";
    let result = run_source(src).expect("print should run");
    assert!(!result.is_empty());
}

#[test]
fn test_builtin_abs() {
    let src = "fn main() @ pure @ cpu { return abs(-5) }";
    let result = run_source(src).expect("abs should run");
    assert!(!result.is_empty());
}
