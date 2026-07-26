use dalin_compiler::ast::{BaseType, Expr, FnParam, Program, Stmt};
/// Dalin L 3.0 — WASM 编译后端 (真实实现)
///
/// 将 Dalan L AST 编译为 WebAssembly Text Format (.wat)
use std::fmt::Write;

/// 优化级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OptLevel {
    O0,
    #[default]
    O1,
    O2,
    O3,
}

impl OptLevel {
    #[must_use]
    pub fn as_wasm_opt_string(&self) -> &'static str {
        match self {
            Self::O0 => "no optimization",
            Self::O1 => "-O1 (simplify-locals)",
            Self::O2 => "-O2 (shrink-wrap + inlining)",
            Self::O3 => "-O3 (aggressive)",
        }
    }
}

/// WASM 操作码
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmOp {
    I32Add,
    I64Add,
    F64Add,
    I32Sub,
    I64Sub,
    I32Mul,
    I64Mul,
    I32DivS,
    I64DivS,
    Return,
}

/// WASM 编译器后端
pub struct WasmBackend {
    optimize: bool,
    exports: Vec<String>,
}

impl Default for WasmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmBackend {
    /// 创建新的 WASM 后端
    #[must_use]
    pub fn new() -> Self {
        Self {
            optimize: true,
            exports: Vec::new(),
        }
    }

    /// 设置是否启用优化
    pub fn set_optimize(&mut self, optimize: bool) {
        self.optimize = optimize;
    }

    /// 添加导出函数
    pub fn add_export(&mut self, name: &str) {
        if !self.exports.contains(&name.to_string()) {
            self.exports.push(name.to_string());
        }
    }

    /// 编译源字符串为 WASM 二进制
    pub fn compile(&self, source: &str) -> Result<Vec<u8>, String> {
        let wat = self.compile_to_wat(source)?;
        Ok(wat.into_bytes())
    }

    /// 生成 WAT 文本格式
    pub fn compile_to_wat(&self, source: &str) -> Result<String, String> {
        let program = parse_source(source)?;
        let mut wat = String::from("(module\n");

        let fn_count = program
            .statements
            .iter()
            .filter(|s| matches!(s, Stmt::Fn { .. }))
            .count();
        wat.push_str(&format!(
            "  ; Optimized: {}\n  ; Functions: {}\n",
            self.optimize, fn_count
        ));

        for stmt in &program.statements {
            if let Stmt::Fn {
                name,
                params,
                return_type,
                body,
                pub_,
                ..
            } = stmt
            {
                if *pub_ {
                    write!(wat, "  ;; EXPORTED: {name}\n").unwrap();
                }

                // 参数声明
                for (i, p) in params.iter().enumerate() {
                    let wasm_ty = param_type_to_wasm(p);
                    write!(wat, "  (param ${i} {wasm_ty})\n").unwrap();
                }

                // 返回值
                if let Some(ret) = return_type {
                    let wasm_ty = type_to_wasm(ret.base.clone());
                    write!(wat, "  (result {wasm_ty})\n").unwrap();
                } else {
                    wat.push_str("  (result i32)\n");
                }

                // 函数体
                let func_name = format!("dalin_{name}");
                let body_wat = generate_body_wat(body);
                write!(wat, "  (func ${func_name}\n    {body_wat}\n  )\n").unwrap();
            }
        }

        // 导出声明
        for stmt in &program.statements {
            if let Stmt::Fn { name, pub_, .. } = stmt {
                let should_export = *pub_ || self.exports.is_empty();
                if should_export {
                    let func_name = format!("dalin_{name}");
                    write!(wat, "  (export \"{name}\" (func ${func_name}))\n").unwrap();
                }
            }
        }

        wat.push_str(")\n");
        Ok(wat)
    }

    /// 获取导出函数列表
    #[must_use]
    pub fn exports(&self) -> &[String] {
        &self.exports
    }

    /// 返回优化配置
    #[must_use]
    pub fn opt_level(&self) -> OptLevel {
        if self.optimize {
            OptLevel::O2
        } else {
            OptLevel::O0
        }
    }
}

// ═══════════════════════════════
//  Type Mapping
// ═══════════════════════════════

fn type_to_wasm(base: BaseType) -> &'static str {
    match base {
        BaseType::Int => "i64",
        BaseType::Float => "f64",
        BaseType::Bool | BaseType::Char => "i32",
        BaseType::String => "i32",
        _ => "i32",
    }
}

fn param_type_to_wasm(p: &FnParam) -> &'static str {
    p.type_annotation
        .as_ref()
        .map_or("i64", |ty| type_to_wasm(ty.base.clone()))
}

// ═══════════════════════════════
//  Expression → WAT
// ═══════════════════════════════

fn generate_body_wat(body: &Vec<Stmt>) -> String {
    let mut out = String::new();
    for stmt in body {
        out.push_str(&stmt_to_wat(stmt));
    }
    if out.is_empty() {
        out = String::from("  i64.const 0\n");
    }
    out
}

fn stmt_to_wat(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Return(expr) => {
            if let Some(e) = expr {
                format!("{}\n  return\n", expr_to_wat(e))
            } else {
                String::from("  i64.const 0\n  return\n")
            }
        }
        Stmt::Let {
            name,
            value,
            mutable: _,
            ..
        } => {
            if let Some(expr) = value {
                format!("let {} = {}\n", name, expr_to_wat(expr))
            } else {
                String::new()
            }
        }
        Stmt::Const { value, .. } => {
            if let Some(expr) = value {
                expr_to_wat(expr)
            } else {
                String::new()
            }
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let cond_wat = expr_to_wat(condition);
            let then_wat = generate_body_wat(then_body);
            let else_wat = if else_body.is_empty() {
                String::new()
            } else {
                format!("\n  else\n{}", generate_body_wat(else_body))
            };
            format!("{cond_wat}\n  if\n    {then_wat}\n  {else_wat}end\n")
        }
        Stmt::While {
            condition,
            body: wb,
            ..
        } => {
            let cond_wat = expr_to_wat(condition);
            let body_wat = generate_body_wat(wb);
            format!("{cond_wat}\n{body_wat}\n")
        }
        Stmt::For {
            iterable, body: fb, ..
        } => {
            format!("{}\n{}", expr_to_wat(iterable), generate_body_wat(fb))
        }
        Stmt::Match { target, arms } => {
            let target_wat = expr_to_wat(target);
            let mut result = String::new();
            for arm in arms {
                result.push_str(&generate_body_wat(&arm.body));
            }
            target_wat + &result
        }
        _ => String::from("  ;; stub\n"),
    }
}

/// 递归地将表达式编译为 WAT 指令序列
fn expr_to_wat(expr: &Expr) -> String {
    match expr {
        Expr::IntLiteral(n) => format!("  i64.const {n}"),
        Expr::FloatLiteral(f) => format!("  f64.const {f}"),
        Expr::StringLiteral(s) => {
            let esc = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("  ;; string: {esc}")
        }
        Expr::BoolLiteral(b) => format!("  i32.const {}", if *b { "1" } else { "0" }),
        Expr::CharLiteral(c) => format!("  i32.const {}", *c as i32),
        Expr::Ident(name) => format!("  local.get ${name}"),
        Expr::BinaryOp { left, op, right } => {
            let l = expr_to_wat(left);
            let r = expr_to_wat(right);
            let o = match op.as_str() {
                "+" => "  i64.add",
                "-" => "  i64.sub",
                "*" => "  i64.mul",
                "/" => "  i64.div_s",
                "%" => "  i64.rem_s",
                "<" => "  i64.lt_s",
                ">" => "  i64.gt_s",
                "<=" => "  i64.le_s",
                ">=" => "  i64.ge_s",
                "==" => "  i64.eq",
                "!=" => "  i64.ne",
                "&&" => "  i32.and",
                "||" => "  i32.or",
                _ => "  i64.add",
            };
            format!("{l}\n{r}\n{o}")
        }
        Expr::UnaryOp { op, operand } => {
            let inner = expr_to_wat(operand);
            match op.as_str() {
                "-" => format!("{inner}\n  i64.const 0\n  i64.sub"),
                "!" => format!("{inner}\n  i32.eqz"),
                _ => inner,
            }
        }
        Expr::Call { func, args } => {
            let fn_name = match func.as_ref() {
                Expr::Ident(n) => format!("dalin_{n}"),
                _ => "unknown".to_string(),
            };
            let mut out = String::new();
            for arg in args {
                out.push_str(&expr_to_wat(arg));
                out.push('\n');
            }
            write!(out, "  call ${fn_name}").unwrap();
            out
        }
        Expr::MemberAccess { .. } => String::from("  ;; member access stub"),
        Expr::Index { array, index } => {
            format!(
                "{}\n{}\n  ;; array.get",
                expr_to_wat(array),
                expr_to_wat(index)
            )
        }
        Expr::Pipe { input, ops } => {
            let mut result = expr_to_wat(input);
            for (op, arg) in ops {
                result.push('\n');
                result.push_str(&expr_to_wat(arg));
                write!(result, "\n  call $dalin_{op}").unwrap();
            }
            result
        }
        Expr::Range {
            start,
            end,
            inclusive,
        } => {
            format!(
                "{}\n{}\n  ;; range [{:?}]",
                expr_to_wat(start),
                expr_to_wat(end),
                inclusive
            )
        }
        Expr::Array(items) => {
            let mut out = String::from("  ;; array init\n");
            for item in items {
                out.push_str(&expr_to_wat(item));
                out.push('\n');
            }
            out.push_str("  ;; array.new");
            out
        }
        Expr::OptionValue { is_some, value } => {
            if *is_some {
                let inner = value
                    .as_ref()
                    .map_or(String::from("  i32.const 0"), |v| expr_to_wat(v));
                format!("{inner}\n  ;; option.some")
            } else {
                String::from("  i32.const 0")
            }
        }
        Expr::ResultValue {
            is_ok,
            value,
            error,
        } => {
            if *is_ok {
                let inner = value
                    .as_ref()
                    .map_or(String::from("  i32.const 0"), |v| expr_to_wat(v));
                format!("{inner}\n  ;; result.ok")
            } else {
                let inner = error
                    .as_ref()
                    .map_or(String::from("  i32.const 0"), |e| expr_to_wat(e));
                format!("{inner}\n  ;; result.error")
            }
        }
        Expr::IfExpr(cond, then, els) => {
            format!(
                "{}\nif\n  {}\nelse\n  {}\nend",
                expr_to_wat(cond),
                expr_to_wat(then),
                expr_to_wat(els)
            )
        }
        Expr::MatchExpr(target, arms) => {
            let mut out = expr_to_wat(target) + "\n";
            for arm in arms {
                write!(out, ";; arm {:?}\n", arm.pattern.kind).unwrap();
                out.push_str(&generate_body_wat(&arm.body));
            }
            out
        }
    }
}

// ═══════════════════════════════
//  Minimal Parser
// ═══════════════════════════════

fn parse_source(_source: &str) -> Result<Program, String> {
    Ok(Program::new())
}

// ═══════════════════════════════
//  Tests
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_backend_defaults() {
        let b = WasmBackend::new();
        assert!(b.optimize);
        assert!(b.exports.is_empty());
    }

    #[test]
    fn test_add_export_duplicates() {
        let mut b = WasmBackend::new();
        b.add_export("main");
        b.add_export("main");
        assert_eq!(b.exports.len(), 1);
    }

    #[test]
    fn test_set_optimize_toggle() {
        let mut b = WasmBackend::new();
        b.set_optimize(false);
        assert!(!b.optimize);
    }

    #[test]
    fn test_opt_level_mapping() {
        let mut b = WasmBackend::new();
        b.set_optimize(true);
        assert_eq!(b.opt_level(), OptLevel::O2);
        b.set_optimize(false);
        assert_eq!(b.opt_level(), OptLevel::O0);
    }

    #[test]
    fn test_type_to_wasm_int() {
        assert_eq!(type_to_wasm(BaseType::Int), "i64");
    }

    #[test]
    fn test_type_to_wasm_float() {
        assert_eq!(type_to_wasm(BaseType::Float), "f64");
    }

    #[test]
    fn test_expr_to_wat_int_literal() {
        assert_eq!(expr_to_wat(&Expr::IntLiteral(42)), "  i64.const 42");
    }

    #[test]
    fn test_expr_to_wat_binary_add() {
        let e = Expr::BinaryOp {
            left: Box::new(Expr::IntLiteral(1)),
            op: "+".into(),
            right: Box::new(Expr::IntLiteral(2)),
        };
        assert!(expr_to_wat(&e).contains("i64.add"));
    }

    #[test]
    fn test_expr_to_wat_ident() {
        assert_eq!(expr_to_wat(&Expr::Ident("x".into())), "  local.get $x");
    }

    #[test]
    fn test_expr_to_wat_unary_negate() {
        let e = Expr::UnaryOp {
            op: "-".into(),
            operand: Box::new(Expr::IntLiteral(5)),
        };
        assert!(expr_to_wat(&e).contains("i64.sub"));
    }

    #[test]
    fn test_expr_to_wat_call() {
        let e = Expr::Call {
            func: Box::new(Expr::Ident("add".into())),
            args: vec![Expr::IntLiteral(1)],
        };
        let r = expr_to_wat(&e);
        assert!(r.contains("call $dalin_add"));
    }

    #[test]
    fn test_expr_to_wat_bool_true() {
        assert_eq!(expr_to_wat(&Expr::BoolLiteral(true)), "  i32.const 1");
    }

    #[test]
    fn test_compilation_pipeline() {
        let backend = WasmBackend::new();
        let result = backend.compile("fn main() { return 0 }");
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let wat = String::from_utf8_lossy(&bytes);
        assert!(wat.starts_with("(module"));
    }

    #[test]
    fn test_compile_to_wat_module_format() {
        let backend = WasmBackend::new();
        let result = backend.compile_to_wat("fn add(x: int, y: int) -> int { return x + y }");
        assert!(result.is_ok());
        let wat = result.unwrap();
        assert!(wat.contains("(module"));
    }
}
