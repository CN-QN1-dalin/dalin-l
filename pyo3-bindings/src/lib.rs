//! Dalin L 3.0 — Python bindings (pyo3)
//!
//! 通过 pyo3 将 Dalin L 的编译器、运行时暴露给 Python 调用，
//! 供 DalinX V8 认知架构直接集成控制平面能力。

use pyo3::prelude::*;

use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use dalin_runtime::interpreter;

/// 将 Dalin L 源代码编译为 AST 的 JSON 表示。
#[pyfunction]
fn compile(source: &str) -> PyResult<String> {
    let mut lex = Lexer::new(source);
    let tokens = lex
        .tokenize()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PySyntaxError, _>(format!("词法错误: {e}")))?;
    let mut parser = Parser::new(tokens);
    let prog = parser
        .parse()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PySyntaxError, _>(format!("语法错误: {e}")))?;
    let names: Vec<String> = prog.statements.iter().map(|s| format!("{s:?}")).collect();
    Ok(serde_json::json!({
        "ok": true,
        "stmt_count": names.len(),
        "statements": names,
    })
    .to_string())
}

/// 在三通道类型系统中查询能力格的偏序关系。
/// capability_a ≤ capability_b 表示 b 的能力覆盖 a。
#[pyfunction]
fn capability_leq(a: &str, b: &str) -> bool {
    let cap_a = parse_capability(a);
    let cap_b = parse_capability(b);
    cap_a <= cap_b
}

/// 将 Dalin L 能力字符串转换为内部枚举值。
fn parse_capability(s: &str) -> u8 {
    match s.to_lowercase().as_str() {
        "cpu" => 0,
        "gpu" => 1,
        "sfa" => 2,
        "net" => 3,
        _ => 0,
    }
}

/// 将 Dalin L 置信度字符串转换为内部枚举值。
#[pyfunction]
fn parse_confidence(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "proven" => "proven",
        "verified" => "verified",
        "inferred" => "inferred",
        "generated" => "generated",
        "uncertain" => "uncertain",
        _ => "uncertain",
    }
    .to_string()
}

/// 执行 Dalin L 源代码并返回结果。
#[pyfunction]
fn run(source: &str) -> PyResult<String> {
    match interpreter::run_source(source) {
        Ok(values) => {
            let strs: Vec<String> = values.iter().map(|v| format!("{v}")).collect();
            Ok(serde_json::json!({
                "ok": true,
                "results": strs,
            })
            .to_string())
        }
        Err(e) => Ok(serde_json::json!({
            "ok": false,
            "error": e.to_string(),
        })
        .to_string()),
    }
}

/// 执行 Dalin L 源代码并返回带任务树的结果。
#[pyfunction]
fn run_with_tree(source: &str) -> PyResult<String> {
    match interpreter::run_source_with_tree(source) {
        Ok(tree_json) => Ok(tree_json),
        Err(e) => Ok(serde_json::json!({
            "ok": false,
            "error": e.to_string(),
        })
        .to_string()),
    }
}

/// Python 模块初始化。
#[pymodule]
fn dalin_pyo3(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(capability_leq, m)?)?;
    m.add_function(wrap_pyfunction!(parse_confidence, m)?)?;
    m.add_function(wrap_pyfunction!(run, m)?)?;
    m.add_function(wrap_pyfunction!(run_with_tree, m)?)?;
    m.add("__version__", "0.2.0")?;
    m.add("__doc__", "Dalin L 3.0 — Python bindings for DalinX V8")?;
    Ok(())
}
