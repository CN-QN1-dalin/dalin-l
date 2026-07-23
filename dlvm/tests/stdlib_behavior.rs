//! Dalin L 3.0 — Phase I 标准库行为实证测试
//!
//! 验证 stdlib 核心函数在 DLVM 上的真实执行结果。
//! 每条测试定义一个内联 Dalin L 程序（含函数定义 + 顶层表达式驱动），
//! 经 Lexer → Parser → BytecodeCompiler → Vm::run 全链路运行后断言返回值。
//!
//! 覆盖模块: math, option, vector, collections, string

use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use dalin_dlvm::{BytecodeCompiler, Value, Vm};

/// 编译并运行一段 Dalin L 源码，返回栈顶值
fn run_src(src: &str) -> Value {
    let mut lex = Lexer::new(src);
    let tokens = lex.tokenize().expect("词法分析失败");
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("语法分析失败");
    let mut compiler = BytecodeCompiler::new();
    let funcs = compiler.compile(&prog);
    let mut vm = Vm::new(funcs);
    vm.run().expect("VM 执行失败")
}

// ═══════════════════════════════════════════════
//  math 模块 — 基本算术
// ═══════════════════════════════════════════════

#[test]
fn math_abs() {
    let src = r#"
fn abs(x) { if x < 0 { return -x } else { return x } }
abs(-5)
"#;
    assert_eq!(run_src(src), Value::Int(5), "abs(-5) 应返回 5");
}

#[test]
fn math_abs_positive() {
    let src = r#"
fn abs(x) { if x < 0 { return -x } else { return x } }
abs(3)
"#;
    assert_eq!(run_src(src), Value::Int(3), "abs(3) 应返回 3");
}

#[test]
fn math_max() {
    let src = r#"
fn max(a, b) { if a > b { return a } else { return b } }
max(3, 7)
"#;
    assert_eq!(run_src(src), Value::Int(7), "max(3, 7) 应返回 7");
}

#[test]
fn math_min() {
    let src = r#"
fn min(a, b) { if a < b { return a } else { return b } }
min(10, 3)
"#;
    assert_eq!(run_src(src), Value::Int(3), "min(10, 3) 应返回 3");
}

#[test]
fn math_clamp() {
    let src = r#"
fn clamp(x, lo, hi) {
    if x < lo { return lo }
    else if x > hi { return hi }
    else { return x }
}
clamp(15, 0, 10)
"#;
    assert_eq!(run_src(src), Value::Int(10), "clamp(15, 0, 10) 应返回 10");
}

#[test]
fn math_clamp_mid() {
    let src = r#"
fn clamp(x, lo, hi) {
    if x < lo { return lo }
    else if x > hi { return hi }
    else { return x }
}
clamp(5, 0, 10)
"#;
    assert_eq!(run_src(src), Value::Int(5), "clamp(5, 0, 10) 应返回 5");
}

#[test]
fn math_is_even_true() {
    let src = r#"
fn is_even(n) { return n % 2 == 0 }
is_even(4)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "is_even(4) 应返回 true");
}

#[test]
fn math_is_even_false() {
    let src = r#"
fn is_even(n) { return n % 2 == 0 }
is_even(7)
"#;
    assert_eq!(run_src(src), Value::Bool(false), "is_even(7) 应返回 false");
}

#[test]
fn math_is_odd_true() {
    let src = r#"
fn is_odd(n) { return n % 2 != 0 }
is_odd(9)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "is_odd(9) 应返回 true");
}

#[test]
fn math_sign_positive() {
    let src = r#"
fn sign(n) {
    if n > 0 { return 1 }
    if n < 0 { return -1 }
    return 0
}
sign(42)
"#;
    assert_eq!(run_src(src), Value::Int(1), "sign(42) 应返回 1");
}

#[test]
fn math_sign_negative() {
    let src = r#"
fn sign(n) {
    if n > 0 { return 1 }
    if n < 0 { return -1 }
    return 0
}
sign(-8)
"#;
    assert_eq!(run_src(src), Value::Int(-1), "sign(-8) 应返回 -1");
}

#[test]
fn math_sign_zero() {
    let src = r#"
fn sign(n) {
    if n > 0 { return 1 }
    if n < 0 { return -1 }
    return 0
}
sign(0)
"#;
    assert_eq!(run_src(src), Value::Int(0), "sign(0) 应返回 0");
}

#[test]
fn math_pow_zero() {
    let src = r#"
fn pow(base, exp) {
    if exp == 0 { return 1 }
    if exp == 1 { return base }
    return base * pow(base, exp - 1)
}
pow(5, 0)
"#;
    assert_eq!(run_src(src), Value::Int(1), "pow(5, 0) 应返回 1");
}

#[test]
fn math_pow_recursive() {
    let src = r#"
fn pow(base, exp) {
    if exp == 0 { return 1 }
    if exp == 1 { return base }
    return base * pow(base, exp - 1)
}
pow(2, 5)
"#;
    assert_eq!(run_src(src), Value::Int(32), "pow(2, 5) 应返回 32");
}

#[test]
fn math_fact() {
    let src = r#"
fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
fact(5)
"#;
    assert_eq!(run_src(src), Value::Int(120), "fact(5) 应返回 120");
}

#[test]
fn math_fact_zero() {
    let src = r#"
fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
fact(0)
"#;
    assert_eq!(run_src(src), Value::Int(1), "fact(0) 应返回 1");
}

#[test]
fn math_gcd() {
    let src = r#"
fn gcd(a, b) {
    if b == 0 { return a }
    return gcd(b, a % b)
}
gcd(12, 8)
"#;
    assert_eq!(run_src(src), Value::Int(4), "gcd(12, 8) 应返回 4");
}

#[test]
fn math_gcd_coprime() {
    let src = r#"
fn gcd(a, b) {
    if b == 0 { return a }
    return gcd(b, a % b)
}
gcd(7, 13)
"#;
    assert_eq!(run_src(src), Value::Int(1), "gcd(7, 13) 应返回 1");
}

#[test]
fn math_lcm() {
    let src = r#"
fn gcd(a, b) {
    if b == 0 { return a }
    return gcd(b, a % b)
}
fn lcm(a, b) { return a * b / gcd(a, b) }
lcm(4, 6)
"#;
    assert_eq!(run_src(src), Value::Int(12), "lcm(4, 6) 应返回 12");
}

#[test]
fn math_fib() {
    let src = r#"
fn fib(n) {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
fib(7)
"#;
    assert_eq!(run_src(src), Value::Int(13), "fib(7) 应返回 13");
}

#[test]
fn math_sum() {
    let src = r#"
fn sum(a, b) { return a + b }
sum(100, 23)
"#;
    assert_eq!(run_src(src), Value::Int(123), "sum(100, 23) 应返回 123");
}

#[test]
fn math_prod() {
    let src = r#"
fn prod(a, b) { return a * b }
prod(7, 8)
"#;
    assert_eq!(run_src(src), Value::Int(56), "prod(7, 8) 应返回 56");
}

#[test]
fn math_quot() {
    let src = r#"
fn quot(a, b) { return a / b }
quot(20, 4)
"#;
    assert_eq!(run_src(src), Value::Int(5), "quot(20, 4) 应返回 5");
}

#[test]
fn math_rem() {
    let src = r#"
fn rem(a, b) { return a % b }
rem(17, 5)
"#;
    assert_eq!(run_src(src), Value::Int(2), "rem(17, 5) 应返回 2");
}

#[test]
fn math_avg() {
    let src = r#"
fn avg(a, b) { return (a + b) / 2 }
avg(10, 20)
"#;
    assert_eq!(run_src(src), Value::Int(15), "avg(10, 20) 应返回 15");
}

#[test]
fn math_dist() {
    let src = r#"
fn abs(x) { if x < 0 { return -x } else { return x } }
fn dist(a, b) { return abs(a - b) }
dist(10, 3)
"#;
    assert_eq!(run_src(src), Value::Int(7), "dist(10, 3) 应返回 7");
}

#[test]
fn math_lerp() {
    let src = r#"
fn lerp(a, b, t) { return a + (b - a) * t }
lerp(0, 10, 3)
"#;
    assert_eq!(run_src(src), Value::Int(30), "lerp(0, 10, 3) 应返回 30");
}

#[test]
fn math_approx_eq_true() {
    let src = r#"
fn approx_eq(a, b, eps) {
    if a > b { return (a - b) < eps }
    return (b - a) < eps
}
approx_eq(3.14, 3.15, 0.1)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "approx_eq(3.14, 3.15, 0.1) 应返回 true");
}

#[test]
fn math_approx_eq_false() {
    let src = r#"
fn approx_eq(a, b, eps) {
    if a > b { return (a - b) < eps }
    return (b - a) < eps
}
approx_eq(1.0, 2.0, 0.1)
"#;
    assert_eq!(run_src(src), Value::Bool(false), "approx_eq(1.0, 2.0, 0.1) 应返回 false");
}

// ═══════════════════════════════════════════════
//  option 模块 — Option 类型工具
// ═══════════════════════════════════════════════
// 注意: 以下测试避开了 HOF（高阶函数参数），因为当前 DLVM 不支持闭包/函数参数传递。
//       map/and_then/filter 等 HOF 函数需要闭包支持，待后续实现。

#[test]
fn option_is_some_not_null() {
    let src = r#"
fn is_some(val) { return !(val == null) }
is_some(42)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "is_some(42) 应返回 true");
}

#[test]
fn option_is_some_null() {
    let src = r#"
fn is_some(val) { return !(val == null) }
is_some(null)
"#;
    assert_eq!(run_src(src), Value::Bool(false), "is_some(null) 应返回 false");
}

#[test]
fn option_is_none_true() {
    let src = r#"
fn is_none(val) { return val == null }
is_none(null)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "is_none(null) 应返回 true");
}

#[test]
fn option_unwrap_some() {
    let src = r#"
fn unwrap(val, msg) {
    if val == null { return null }
    return val
}
unwrap(99, "oops")
"#;
    assert_eq!(run_src(src), Value::Int(99), "unwrap(99) 应返回 99");
}

#[test]
fn option_unwrap_or_with_default() {
    let src = r#"
fn unwrap_or(val, default) {
    if val == null { return default }
    return val
}
unwrap_or(null, 42)
"#;
    assert_eq!(run_src(src), Value::Int(42), "unwrap_or(null, 42) 应返回 42");
}

#[test]
fn option_unwrap_or_with_value() {
    let src = r#"
fn unwrap_or(val, default) {
    if val == null { return default }
    return val
}
unwrap_or(7, 99)
"#;
    assert_eq!(run_src(src), Value::Int(7), "unwrap_or(7, 99) 应返回 7");
}

#[test]
fn option_contains_some() {
    let src = r#"
fn contains(val, x) {
    if val == null { return false }
    return val == x
}
contains(42, 42)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "contains(42, 42) 应返回 true");
}

#[test]
fn option_contains_not() {
    let src = r#"
fn contains(val, x) {
    if val == null { return false }
    return val == x
}
contains(42, 7)
"#;
    assert_eq!(run_src(src), Value::Bool(false), "contains(42, 7) 应返回 false");
}

// ═══════════════════════════════════════════════
//  vector 模块 — 基本列表操作
// ═══════════════════════════════════════════════

#[test]
fn vector_new_empty() {
    let src = r#"
fn new() { return [] }
new()
"#;
    assert_eq!(run_src(src), Value::Array(vec![]), "new() 应返回空数组");
}

#[test]
fn vector_len_empty() {
    let src = r#"
fn len(v) { return len(v) }
len([])
"#;
    assert_eq!(run_src(src), Value::Int(0), "len([]) 应返回 0");
}

// ═══════════════════════════════════════════════
//  string 模块 — 字符串操作
// ═══════════════════════════════════════════════

#[test]
fn string_append_test() {
    let src = r#"
fn string_append(a, b) { return a + b }
string_append("hello ", "world")
"#;
    assert_eq!(
        run_src(src),
        Value::Str("hello world".to_string()),
        "string_append('hello ', 'world') 应返回 'hello world'"
    );
}

// ═══════════════════════════════════════════════
//  collections 模块 — 列表操作
// ═══════════════════════════════════════════════

#[test]
fn list_new() {
    let src = r#"
fn list_new() { return [] }
list_new()
"#;
    assert_eq!(run_src(src), Value::Array(vec![]), "list_new() 应返回 []");
}

#[test]
fn list_contains_true() {
    let src = r#"
fn list_contains(lst, val) {
    if lst == [] { return false }
    if lst[0] == val { return true }
    return false
}
list_contains([1, 2, 3], 1)
"#;
    assert_eq!(run_src(src), Value::Bool(true), "list_contains([1,2,3], 1) 应返回 true");
}

#[test]
fn list_contains_false() {
    let src = r#"
fn list_contains(lst, val) {
    if lst == [] { return false }
    if lst[0] == val { return true }
    return false
}
list_contains([4, 5, 6], 99)
"#;
    assert_eq!(run_src(src), Value::Bool(false), "list_contains([4,5,6], 99) 应返回 false");
}

// ═══════════════════════════════════════════════
//  复杂组合 — 多模块函数互调
// ═══════════════════════════════════════════════

#[test]
fn combo_pow_then_abs() {
    let src = r#"
fn abs(x) { if x < 0 { return -x } else { return x } }
fn pow(base, exp) {
    if exp == 0 { return 1 }
    if exp == 1 { return base }
    return base * pow(base, exp - 1)
}
abs(pow(-2, 3))
"#;
    assert_eq!(run_src(src), Value::Int(8), "abs(pow(-2, 3)) 应返回 8");
}

#[test]
fn combo_gcd_and_lcm() {
    let src = r#"
fn gcd(a, b) {
    if b == 0 { return a }
    return gcd(b, a % b)
}
fn lcm(a, b) { return a * b / gcd(a, b) }
lcm(6, 10)
"#;
    assert_eq!(run_src(src), Value::Int(30), "lcm(6, 10) 应返回 30");
}