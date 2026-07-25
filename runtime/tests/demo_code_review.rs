use dalin_runtime::interpreter::run_source;

#[test]
fn test_code_review_demo() {
    let src = r#"
fn check_naming(line_str, line_num) @ pure @ cpu {
    let issues = []
    let len_line = len(line_str)
    let i = 0
    while i < len_line {
        if line_str[i] == ':' {
            let issues = push(issues, line_num)
            let i = len_line
        }
        let i = i + 1
    }
    return issues
}

fn main() @ pure @ cpu {
    let test_code = "let MyVar: int = 42"
    let issues = check_naming(test_code, 1)
    return len(issues)
}
"#;
    let results = run_source(src).expect("Code review demo should run");
    assert!(!results.is_empty(), "Should produce output");
}