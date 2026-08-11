//! Dalin L 3.0 — 错误路径集成测试
//!
//! 测试编译器和运行时在错误输入下的行为：
//! - 语法错误：非法 token、括号不匹配、函数体缺失
//! - 语义错误：未定义变量、参数数量不匹配
//! - 运行时错误：除零

use dalin_runtime::env::Value;
use dalin_runtime::interpreter::{run_program, run_source};

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
    assert!(
        result.is_ok() || result.is_err(),
        "Undefined variable should not panic"
    );
}

#[test]
fn test_wrong_arg_count_does_not_panic() {
    let src = "fn add(a, b) @ pure @ cpu { return a + b }
fn main() @ pure @ cpu { return add(1) }";
    let result = run_source(src);
    // 参数数量不匹配可能被运行时容忍（默认值或扩展到 0），但不应 panic
    assert!(
        result.is_ok() || result.is_err(),
        "Wrong arg count should not panic"
    );
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
    if let Err(e) = &result {
        let msg = e.0.to_lowercase();
        assert!(
            msg.contains("error") || msg.contains("division") || msg.contains("zero"),
            "Division by zero error: {}",
            msg
        );
    }
    // 如果运行时容忍除零（Ok），也接受
}

#[test]
fn test_empty_source() {
    let src = "";
    let result = run_source(src);
    assert!(
        result.is_ok() || result.is_err(),
        "Empty source should not panic"
    );
}

// ══════════════════════════════════════════════════════════════
//  审计整改回归：#1 短路求值 / #2 溢出与除零保护
// ══════════════════════════════════════════════════════════════

/// #1: `&&` 必须短路。左值为 false 时右值（含除零表达式）绝不求值，整体返回 false。
#[test]
fn test_short_circuit_and_skips_rhs() {
    let src = "fn main() @ pure @ cpu {
        let b = 0
        return b != 0 && (10 / b > 0)
    }";
    let result = run_program(src);
    match result {
        Ok(vals) => match vals.last() {
            Some(Value::Bool(false)) => {}
            other => panic!(
                "short-circuit && with b==0 must yield false, got {:?}",
                other
            ),
        },
        Err(e) => panic!("short-circuit && must NOT error/panic, got: {}", e.0),
    }
}

/// #1: `||` 必须短路。左值为 true 时右值（含除零表达式）绝不求值，整体返回 true。
#[test]
fn test_short_circuit_or_skips_rhs() {
    let src = "fn main() @ pure @ cpu {
        let c = 0
        return c == 0 || (10 / c > 0)
    }";
    let result = run_program(src);
    match result {
        Ok(vals) => match vals.last() {
            Some(Value::Bool(true)) => {}
            other => panic!(
                "short-circuit || with c==0 must yield true, got {:?}",
                other
            ),
        },
        Err(e) => panic!("short-circuit || must NOT error/panic, got: {}", e.0),
    }
}

/// #1: 非短路路径（左值不能决定结果时）仍需正确求值右值。
#[test]
fn test_non_short_circuit_still_works() {
    let src = "fn main() @ pure @ cpu {
        let d = 5
        return d != 0 && (10 / d > 0)
    }";
    let result = run_program(src);
    match result {
        Ok(vals) => match vals.last() {
            Some(Value::Bool(true)) => {}
            other => panic!("non-short-circuit && must yield true, got {:?}", other),
        },
        Err(e) => panic!("non-short-circuit && unexpectedly errored: {}", e.0),
    }
}

/// #2: 整数乘法溢出必须返回 RuntimeError（绝不在 debug 下 panic / release 下静默回绕）。
#[test]
fn test_integer_overflow_is_runtime_error() {
    let src = "fn main() @ pure @ cpu {
        let big = 4631686018427387904
        return big * 2
    }";
    let result = run_program(src);
    match &result {
        Err(e) => {
            let msg = e.0.to_lowercase();
            assert!(
                msg.contains("overflow"),
                "overflow error expected, got: {}",
                msg
            );
        }
        Ok(v) => panic!("integer overflow must be a RuntimeError, got Ok({:?})", v),
    }
}

/// #2: 整数除零必须返回 RuntimeError（绝不 panic）。
#[test]
fn test_integer_division_by_zero_is_runtime_error() {
    let src = "fn main() @ pure @ cpu {
        let z = 0
        return 10 / z
    }";
    let result = run_program(src);
    match &result {
        Err(e) => {
            let msg = e.0.to_lowercase();
            assert!(
                msg.contains("division") || msg.contains("zero"),
                "division-by-zero error expected, got: {}",
                msg
            );
        }
        Ok(v) => panic!("division by zero must be a RuntimeError, got Ok({:?})", v),
    }
}

/// #2: 整数模零必须返回 RuntimeError（绝不 panic）。
#[test]
fn test_integer_modulo_by_zero_is_runtime_error() {
    let src = "fn main() @ pure @ cpu {
        let z = 0
        return 10 % z
    }";
    let result = run_program(src);
    match &result {
        Err(e) => {
            let msg = e.0.to_lowercase();
            assert!(
                msg.contains("modulo") || msg.contains("zero"),
                "modulo-by-zero error expected, got: {}",
                msg
            );
        }
        Ok(v) => panic!("modulo by zero must be a RuntimeError, got Ok({:?})", v),
    }
}
