//! 跨模块调用测试 — 验证 stdlib 模块间调用在运行时真正工作

use dalin_runtime::interpreter::run_source;
use dalin_runtime::env::Value;

fn run_last(src: &str) -> Value {
    let full = format!("{src}\nmain()");
    run_source(&full).unwrap_or_else(|e| panic!("run failed: {e}\nsrc: {full}"))
        .into_iter().last().unwrap_or(Value::None)
}

#[test]
fn cross_module_strutil_to_strings() {
    // strutil::str_reverse 转发到 strings::str_reverse
    let v = run_last("fn main() @ pure @ cpu { return strutil::str_reverse(\"abc\") }");
    assert_eq!(v, Value::String("cba".into()), "str_reverse(abc) = cba");
}

#[test]
fn cross_module_strings_direct() {
    let v = run_last("fn main() @ pure @ cpu { return strings::str_reverse(\"hello\") }");
    assert_eq!(v, Value::String("olleh".into()), "str_reverse(hello) = olleh");
}

#[test]
fn string_indexing() {
    let v = run_last("fn main() @ pure @ cpu { let s = \"hello\"; return s[1] }");
    assert_eq!(v, Value::Char('e'), "s[1] = 'e'");
}

#[test]
fn string_concat_with_char() {
    let v = run_last("fn main() @ pure @ cpu { let s = \"ab\"; return s + \"c\" }");
    assert_eq!(v, Value::String("abc".into()), "ab + c = abc");
}
