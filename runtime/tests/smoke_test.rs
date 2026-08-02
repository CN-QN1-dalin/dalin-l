//! Dalin L 3.0 — 冒烟测试（Smoke Test）
//!
//! 目的：验证核心运行时能力端到端可用，不仅"能解析"，还要"求值正确"。
//! 覆盖：算术/布尔/比较、变量绑定、函数调用、控制流、集合、stdlib 调用、
//!       递归、错误路径。
//!
//! 与 test_stdlib_real_impl.rs 的区别：本套件做**值断言**（验证返回值正确），
//! 而非仅断言"不 panic"。

use dalin_runtime::env::Value;
use dalin_runtime::interpreter::run_source;

/// 执行源码并取出 main() 的返回值。
/// run_source 逐语句执行：fn 声明注册后返回 none，main() 调用才产生结果。
/// 因此追加 `main()` 调用并取最后一个返回值。
fn run_and_get_first(src: &str) -> Value {
    let full = format!("{src}\nmain()");
    let results =
        run_source(&full).unwrap_or_else(|e| panic!("run_source failed: {e}\nsrc: {full}"));
    results.into_iter().last().unwrap_or(Value::None)
}

// ═══════════════════════════════════════════════════════
//  1. 算术求值
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_arith_basic() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return 1 + 2 * 3 }");
    assert_eq!(v, Value::Int(7), "1 + 2*3 = 7");
}

#[test]
fn smoke_arith_precedence_parens() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return (1 + 2) * 3 }");
    assert_eq!(v, Value::Int(9), "(1+2)*3 = 9");
}

#[test]
fn smoke_arith_division() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return 10 / 4 }");
    assert_eq!(v, Value::Int(2), "integer division truncates");
}

#[test]
fn smoke_arith_float() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return 3.5 + 1.5 }");
    assert_eq!(v, Value::Float(5.0), "3.5+1.5 = 5.0");
}

// ═══════════════════════════════════════════════════════
//  2. 布尔与比较
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_bool_logic() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return true && !false }");
    assert_eq!(v, Value::Bool(true), "true && !false = true");
}

#[test]
fn smoke_bool_or() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return false || true }");
    assert_eq!(v, Value::Bool(true), "false || true = true");
}

#[test]
fn smoke_comparison() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return 5 > 3 }");
    assert_eq!(v, Value::Bool(true), "5 > 3 = true");
}

#[test]
fn smoke_comparison_eq() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return 2 + 2 == 4 }");
    assert_eq!(v, Value::Bool(true), "2+2 == 4 = true");
}

// ═══════════════════════════════════════════════════════
//  3. 变量绑定
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_let_binding() {
    let v = run_and_get_first("fn main() @ pure @ cpu { let x = 42; return x }");
    assert_eq!(v, Value::Int(42), "let x = 42; return x");
}

#[test]
fn smoke_let_reassignment() {
    let v = run_and_get_first("fn main() @ pure @ cpu { let x = 1; let x = x + 1; return x }");
    assert_eq!(v, Value::Int(2), "rebind x = x + 1");
}

#[test]
fn smoke_const_binding() {
    let v = run_and_get_first("fn main() @ pure @ cpu { const C = 10; return C * 2 }");
    assert_eq!(v, Value::Int(20), "const C = 10; C*2");
}

// ═══════════════════════════════════════════════════════
//  4. 函数定义与调用
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_fn_call() {
    let src = "fn add(a, b) @ pure @ cpu { return a + b }
fn main() @ pure @ cpu { return add(3, 4) }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(7), "add(3,4) = 7");
}

#[test]
fn smoke_fn_multiple_args() {
    let src = "fn f(a, b, c) @ pure @ cpu { return a * b + c }
fn main() @ pure @ cpu { return f(2, 3, 1) }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(7), "f(2,3,1) = 2*3+1 = 7");
}

#[test]
fn smoke_fn_early_return() {
    let src = "fn first() @ pure @ cpu { return 1 }
fn main() @ pure @ cpu { return first() }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(1), "first() = 1");
}

// ═══════════════════════════════════════════════════════
//  5. 控制流
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_if_then() {
    let v = run_and_get_first("fn main() @ pure @ cpu { if true { return 1 } else { return 2 } }");
    assert_eq!(v, Value::Int(1), "if true -> 1");
}

#[test]
fn smoke_if_else() {
    let v = run_and_get_first("fn main() @ pure @ cpu { if false { return 1 } else { return 2 } }");
    assert_eq!(v, Value::Int(2), "if false -> else 2");
}

#[test]
fn smoke_while_loop() {
    let src = "fn main() @ pure @ cpu {
        let i = 0
        while i < 5 {
            let i = i + 1
        }
        return i
    }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(5), "while loop 0->5");
}

#[test]
fn smoke_for_range() {
    let src = "fn main() @ pure @ cpu {
        let sum = 0
        for i in 0..5 {
            let sum = sum + i
        }
        return sum
    }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(10), "sum 0..5 = 0+1+2+3+4 = 10");
}

#[test]
fn smoke_nested_control_flow() {
    let src = "fn main() @ pure @ cpu {
        let total = 0
        for i in 0..3 {
            if i % 2 == 0 {
                let total = total + i
            }
        }
        return total
    }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(2), "evens in 0..3 summed = 0+2 = 2");
}

// ═══════════════════════════════════════════════════════
//  6. 集合与索引
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_array_index() {
    let src = "fn main() @ pure @ cpu { let a = [10, 20, 30]; return a[1] }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(20), "a[1] = 20");
}

#[test]
fn smoke_array_concat() {
    let src = "fn main() @ pure @ cpu { let a = [1, 2]; let b = a + [3]; return b[2] }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(3), "b[2] = 3");
}

#[test]
fn smoke_string_concat() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return \"foo\" + \"bar\" }");
    assert_eq!(
        v,
        Value::String("foobar".into()),
        "\"foo\"+\"bar\" = \"foobar\""
    );
}

// ═══════════════════════════════════════════════════════
//  7. stdlib 真实调用（值断言）
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_stdlib_math() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return sqrt(16.0) }");
    match v {
        Value::Float(x) => {
            let diff = (x - 4.0).abs();
            assert!(diff < 1e-6, "sqrt(16) ≈ 4.0, got {x}");
        }
        other => panic!("expected Float, got {other:?}"),
    }
}

#[test]
fn smoke_stdlib_strings() {
    let v = run_and_get_first("fn main() @ pure @ cpu { return str_len(\"hello\") }");
    assert_eq!(v, Value::Int(5), "str_len(hello) = 5");
}

#[test]
fn smoke_stdlib_collections() {
    let src =
        "fn main() @ pure @ cpu { let v = vec_new(); let v2 = vec_push(v, 7); return vec_len(v2) }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(1), "vec_len after push = 1");
}

// ═══════════════════════════════════════════════════════
//  8. 递归
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_recursion_factorial() {
    let src = "fn fact(n) @ pure @ cpu {
        if n <= 1 { return 1 }
        return n * fact(n - 1)
    }
fn main() @ pure @ cpu { return fact(5) }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(120), "fact(5) = 120");
}

#[test]
fn smoke_recursion_fibonacci() {
    let src = "fn fib(n) @ pure @ cpu {
        if n < 2 { return n }
        return fib(n - 1) + fib(n - 2)
    }
fn main() @ pure @ cpu { return fib(10) }";
    let v = run_and_get_first(src);
    assert_eq!(v, Value::Int(55), "fib(10) = 55");
}

// ═══════════════════════════════════════════════════════
//  9. 错误路径（非法输入应报错而非 panic）
// ═══════════════════════════════════════════════════════

#[test]
fn smoke_error_invalid_syntax() {
    let r = run_source("fn main() @ pure @ cpu { return @@@ }");
    assert!(r.is_err(), "invalid syntax should error");
}

#[test]
fn smoke_error_unbalanced_parens() {
    let r = run_source("fn main() @ pure @ cpu { return (1 + 2 }");
    assert!(r.is_err(), "unbalanced parens should error");
}

#[test]
fn smoke_error_missing_fn_body() {
    let r = run_source("fn main() @ pure @ cpu");
    assert!(r.is_err(), "missing body should error");
}
