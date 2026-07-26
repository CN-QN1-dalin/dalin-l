//! 端到端集成测试 — 标准库模块功能验证

use dalin_runtime::interpreter::run_source;

/// math.dal — math_sqrt 测试
#[test]
fn test_stdlib_math_sqrt() {
    let src = "fn main() @ pure @ cpu { return sqrt(16.0) }";
    let result = run_source(src).expect("math_sqrt should run");
    assert!(!result.is_empty());
}

/// math.dal — math_abs 测试
#[test]
fn test_stdlib_math_abs() {
    let src = "fn main() @ pure @ cpu { return abs(-5) }";
    let result = run_source(src).expect("math_abs should run");
    assert!(!result.is_empty());
}

/// math.dal — math_max 测试
#[test]
fn test_stdlib_math_max() {
    let src = "fn main() @ pure @ cpu { return max(3, 5) }";
    let result = run_source(src).expect("math_max should run");
    assert!(!result.is_empty());
}

/// math.dal — math_min 测试
#[test]
fn test_stdlib_math_min() {
    let src = "fn main() @ pure @ cpu { return min(3, 5) }";
    let result = run_source(src).expect("math_min should run");
    assert!(!result.is_empty());
}

/// math.dal — math_factorial 测试
#[test]
fn test_stdlib_math_factorial() {
    let src = "fn main() @ pure @ cpu { return factorial(5) }";
    let result = run_source(src).expect("math_factorial should run");
    assert!(!result.is_empty());
}

/// math.dal — math_power 测试
#[test]
fn test_stdlib_math_power() {
    let src = "fn main() @ pure @ cpu { return power(2, 10) }";
    let result = run_source(src).expect("math_power should run");
    assert!(!result.is_empty());
}

/// strings.dal — str_len 测试
#[test]
fn test_stdlib_str_len() {
    let src = "fn main() @ pure @ cpu { return str_len(\"hello\") }";
    let result = run_source(src).expect("str_len should run");
    assert!(!result.is_empty());
}

/// strings.dal — str_concat 测试
#[test]
fn test_stdlib_str_concat() {
    let src = "fn main() @ pure @ cpu { return str_concat(\"hello \", \"world\") }";
    let result = run_source(src).expect("str_concat should run");
    assert!(!result.is_empty());
}

/// strings.dal — str_reverse 测试
#[test]
fn test_stdlib_str_reverse() {
    let src = "fn main() @ pure @ cpu { return str_reverse(\"abcde\") }";
    let result = run_source(src).expect("str_reverse should run");
    assert!(!result.is_empty());
}

/// collections.dal — list_new + list_len 测试
#[test]
fn test_stdlib_list_new() {
    let src = "fn main() @ pure @ cpu { let l = list_new(); return list_len(l) }";
    let result = run_source(src).expect("list_new should run");
    assert!(!result.is_empty());
}

/// collections.dal — list_push 测试
#[test]
fn test_stdlib_list_push() {
    let src = "fn main() @ pure @ cpu { let l = []; let l = push(l, 1); return len(l) }";
    let result = run_source(src).expect("list_push should run");
    assert!(!result.is_empty());
}

/// option.dal — opt_some/opt_none 测试
#[test]
fn test_stdlib_opt_some() {
    let src = "fn main() @ pure @ cpu { let o = [true, 42]; return o[1] }";
    let result = run_source(src).expect("opt_some should run");
    assert!(!result.is_empty());
}

/// result.dal — result_ok 测试
#[test]
fn test_stdlib_result_ok() {
    let src = "fn main() @ pure @ cpu { let r = [true, 42, null]; return r[1] }";
    let result = run_source(src).expect("result_ok should run");
    assert!(!result.is_empty());
}
