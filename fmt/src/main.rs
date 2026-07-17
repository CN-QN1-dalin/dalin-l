#![allow(clippy::all)]
//! Dalin L 代码格式化器（dalin-fmt）
//!
//! 读取源码文件，输出格式化后的代码。
//! 用法：`dalin-fmt file.dalin`

use std::fs;
use std::io::{self, Read};

use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;

fn format_source(src: &str) -> Result<String, String> {
    let mut lex = Lexer::new(src);
    let tokens = lex.tokenize().map_err(|e| format!("lex error: {e}"))?;
    let prog = Parser::new(tokens).parse().map_err(|e| format!("parse error: {e}"))?;

    let mut out = String::new();
    let indent = 0u8;

    for stmt in &prog.statements {
        let line = format_stmt(stmt, indent);
        out.push_str(&line);
        out.push('\n');
    }

    Ok(out)
}

fn format_stmt(stmt: &dalin_compiler::ast::Stmt, indent: u8) -> String {
    let pad = "    ".repeat(indent as usize);
    match stmt {
        dalin_compiler::ast::Stmt::Let { name, value, .. } => {
            if let Some(v) = value {
                format!("{}let {} = {}", pad, name, format_expr(v, indent))
            } else {
                format!("{}let {}", pad, name)
            }
        }
        dalin_compiler::ast::Stmt::Fn { name, params, body, .. } => {
            let params_str: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            let body_str: String = body.iter()
                .map(|s| format_stmt(s, indent + 1))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{}fn {}({}) {{\n{}\n{}}}", pad, name, params_str.join(", "), body_str, pad)
        }
        dalin_compiler::ast::Stmt::Return(val) => {
            match val {
                Some(v) => format!("{}return {}", pad, format_expr(v, indent)),
                None => format!("{}return", pad),
            }
        }
        dalin_compiler::ast::Stmt::Expr(e) => {
            format!("{}{}", pad, format_expr(e, indent))
        }
        _ => format!("{}?stmt", pad),
    }
}

fn format_expr(expr: &dalin_compiler::ast::Expr, _indent: u8) -> String {
    match expr {
        dalin_compiler::ast::Expr::IntLiteral(n) => format!("{n}"),
        dalin_compiler::ast::Expr::FloatLiteral(f) => format!("{f}"),
        dalin_compiler::ast::Expr::StringLiteral(s) => format!("\"{s}\""),
        dalin_compiler::ast::Expr::BoolLiteral(b) => format!("{b}"),
        dalin_compiler::ast::Expr::Ident(name) => name.clone(),
        dalin_compiler::ast::Expr::BinaryOp { left, op, right } => {
            format!("{} {} {}", format_expr(left, _indent), op, format_expr(right, _indent))
        }
        dalin_compiler::ast::Expr::Call { func, args } => {
            let args_str: Vec<String> = args.iter().map(|a| format_expr(a, _indent)).collect();
            format!("{}({})", format_expr(func, _indent), args_str.join(", "))
        }
        dalin_compiler::ast::Expr::IfExpr(cond, then, else_) => {
            format!("if {} {{ {} }} else {{ {} }}",
                format_expr(cond, _indent),
                format_expr(then, _indent),
                format_expr(else_, _indent))
        }
        _ => "?expr".to_string(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = if args.len() > 1 {
        match fs::read_to_string(&args[1]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error reading {}: {}", args[1], e);
                std::process::exit(1);
            }
        }
    } else {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).unwrap();
        buf
    };

    match format_source(&src) {
        Ok(formatted) => print!("{formatted}"),
        Err(e) => {
            eprintln!("format error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_let_stmt() {
        let result = format_source("let x=42").unwrap();
        assert!(result.contains("let x = 42"));
    }

    #[test]
    fn format_fn_def() {
        let result = format_source("fn add(a,b){return a+b}").unwrap();
        assert!(result.contains("fn add(a, b) {"));
        assert!(result.contains("return a + b"));
    }
}
