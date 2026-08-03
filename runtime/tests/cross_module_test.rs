//! 跨模块调用测试 — 验证 stdlib 模块间调用在运行时真正工作

use dalin_runtime::env::Value;
use dalin_runtime::interpreter::run_source;

fn run_last(src: &str) -> Value {
    let full = format!("{src}\nmain()");
    run_source(&full)
        .unwrap_or_else(|e| panic!("run failed: {e}\nsrc: {full}"))
        .into_iter()
        .last()
        .unwrap_or(Value::None)
}

#[test]
fn cross_module_strings_str_repeat_after_dedup() {
    // str_repeat 原仅定义于 strutil.dal，去重后并入 canonical strings 模块
    let v = run_last("fn main() @ pure @ cpu { return strings::str_repeat(\"ab\", 3) }");
    assert_eq!(v, Value::String("ababab".into()), "str_repeat(ab,3) = ababab");
}

#[test]
fn cross_module_strings_direct() {
    let v = run_last("fn main() @ pure @ cpu { return strings::str_reverse(\"hello\") }");
    assert_eq!(
        v,
        Value::String("olleh".into()),
        "str_reverse(hello) = olleh"
    );
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

/// 命名空间隔离：qualified 调用精确命中 `strings` 模块实现（而非 strutil 转发包装）
#[test]
fn namespace_qualified_hits_strings_module() {
    let v = run_last("fn main() @ pure @ cpu { return strings::str_ends_with(\"hello\", \"lo\") }");
    assert_eq!(
        v,
        Value::Bool(true),
        "strings::str_ends_with(hello, lo) = true"
    );
}

/// 命名空间隔离：qualified 调用精确命中 canonical `strings` 模块实现
#[test]
fn namespace_qualified_strings_resolves() {
    let v =
        run_last("fn main() @ pure @ cpu { return strings::str_starts_with(\"hello\", \"he\") }");
    assert_eq!(
        v,
        Value::Bool(true),
        "strings::str_starts_with(hello, he) = true"
    );
}

/// 命名空间隔离 + 向后兼容：str_ends_with 去重后仅 strings 模块拥有，裸调用确定性命中
/// 裸调用仍可用，确定性命中首个定义模块（strings），不再有静默遮蔽导致的不可预期行为
#[test]
fn namespace_ambiguous_bare_keeps_deterministic_default() {
    let v = run_last("fn main() @ pure @ cpu { return str_ends_with(\"hello\", \"lo\") }");
    assert_eq!(
        v,
        Value::Bool(true),
        "裸 str_ends_with 确定性命中 strings 模块"
    );
}

/// 用户函数优先：用户定义的同名函数遮蔽 stdlib 裸别名（验证解析优先级用户 > stdlib 裸别名）
#[test]
fn user_fn_shadows_stdlib_bare_alias() {
    let src = r#"
        fn str_ends_with(s, suffix) @ pure @ cpu { return true }
        fn main() @ pure @ cpu { return str_ends_with("hello", "lo") }
    "#;
    let v = run_last(src);
    assert_eq!(
        v,
        Value::Bool(true),
        "用户定义 str_ends_with 遮蔽 stdlib 裸别名"
    );
}

/// 向后兼容：唯一归属的 stdlib 裸名仍可裸调用
#[test]
fn bare_unique_stdlib_still_resolves() {
    let v = run_last("fn main() @ pure @ cpu { return str_reverse(\"abcde\") }");
    assert_eq!(
        v,
        Value::String("edcba".into()),
        "裸 str_reverse 仍向后兼容"
    );
}
