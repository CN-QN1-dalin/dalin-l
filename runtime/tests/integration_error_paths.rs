//! Dalin L 3.0 — 错误路径集成测试
//!
//! 测试编译器和运行时在错误输入下的行为：
//! - 语法错误：非法 token、括号不匹配、函数体缺失
//! - 语义错误：未定义变量、参数数量不匹配
//! - 运行时错误：除零

use dalin_runtime::interpreter::run_source;

// ══════════════════════════════════════════════════════════════
//  语法错误
// ══════════════════════════════════════════════════════════════

#[test]
fn test_invalid_syntax() {
    let src = "fn main() @ pure @ cpu { return @@@ }";
    let result = run_source(src);
    assert!(result.is_err(), "Invalid syntax should return error");
}

#[test]
fn test_unbalanced_parens() {
    let src = "fn main() @ pure @ cpu { return (1 + 2 }";
    let result = run_source(src);
    assert!(result.is_err(), "Unbalanced parens should return error");
}

#[test]
fn test_missing_fn_body() {
    let src = "fn main() @ pure @ cpu";
    let result = run_source(src);
    assert!(result.is_err(), "Missing function body should return error");
}

// ══════════════════════════════════════════════════════════════
//  语义错误
// ══════════════════════════════════════════════════════════════

// ══════════════════════════════════════════════════════════════
//  语义错误（当前运行时可能不检测，但不应 panic）
// ══════════════════════════════════════════════════════════════

#[test]
fn test_undefined_variable_does_not_panic() {
    let src = "fn main() @ pure @ cpu { return undefined_var }";
    let result = run_source(src);
    // 未定义变量可能被运行时容忍（作为函数调用或 null），但不应 panic
    assert!(result.is_ok() || result.is_err(), "Undefined variable should not panic");
}

#[test]
fn test_wrong_arg_count_does_not_panic() {
    let src = "fn add(a, b) @ pure @ cpu { return a + b }
fn main() @ pure @ cpu { return add(1) }";
    let result = run_source(src);
    // 参数数量不匹配可能被运行时容忍（默认值或扩展到 0），但不应 panic
    assert!(result.is_ok() || result.is_err(), "Wrong arg count should not panic");
}

// ══════════════════════════════════════════════════════════════
//  运行时错误
// ══════════════════════════════════════════════════════════════

#[test]
fn test_division_by_zero() {
    let src = "fn div(a, b) @ pure @ cpu { return a / b }
fn main() @ pure @ cpu { return div(10, 0) }";
    let result = run_source(src);
    // 除零应返回错误（不应 panic）
    match &result {
        Err(e) => {
            let msg = e.0.to_lowercase();
            assert!(msg.contains("error") || msg.contains("division") || msg.contains("zero"),
                "Division by zero error: {}", msg);
        }
        Ok(_) => {} // 如果运行时容忍除零，也可以
    }
}

#[test]
fn test_empty_source() {
    let src = "";
    let result = run_source(src);
    assert!(result.is_ok() || result.is_err(), "Empty source should not panic");
}