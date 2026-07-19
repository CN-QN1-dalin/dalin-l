/// Dalin L 3.0 — 延迟验证器
///
/// 编译器阶段：对每个声明了 `@latency(Nms)` 的函数，验证其调用链总延迟 ≤ Nms。
/// 这是 Phase D 时序契约的编译期检查——函数调用链延迟上界分析。
///
/// 验证规则：
///   1. 函数 A 声明 @latency(50ms)
///   2. A 调用 B、C
///   3. B 声明 @latency(10ms)，C 声明 @latency(30ms)
///   4. A 的自身开销 + B 的延迟 + C 的延迟 = 10 + 30 + 5(overhead) = 45ms ≤ 50ms ✓
///   5. 如果 > 50ms，编译器报错：`延迟违规: A 声明 50ms，但调用链累计 70ms`
use crate::ast::{Expr, Program, Stmt};
use std::collections::HashMap;

/// 延迟验证结果
#[derive(Debug, Clone)]
pub struct LatencyVerificationResult {
    /// 所有延迟违规错误
    pub errors: Vec<String>,
    /// 每个函数的推断延迟（从注解 + 调用链分析得出）
    pub fn_latencies: HashMap<String, u64>,
}

impl Default for LatencyVerificationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyVerificationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            fn_latencies: HashMap::new(),
        }
    }
}

/// 延迟验证器：校验函数调用链延迟是否满足 @latency 声明
pub struct LatencyVerifier;

impl LatencyVerifier {
    /// 验证整个 Program 的延迟约束
    /// 1) 收集所有函数的 @latency 声明
    /// 2) 对每个函数，分析其 body 中的调用链
    /// 3) 校验调用链总延迟 ≤ 声明延迟
    pub fn verify(prog: &Program) -> LatencyVerificationResult {
        let mut result = LatencyVerificationResult::new();

        // 第一步：收集所有函数的延迟声明
        let mut declared: HashMap<String, u64> = HashMap::new();
        for stmt in &prog.statements {
            if let Stmt::Fn { name, latency, .. } = stmt
                && let Some(lat_str) = latency
            {
                // 解析 "50ms" → 50
                if let Ok(ms) = lat_str.trim_end_matches("ms").parse::<u64>() {
                    declared.insert(name.clone(), ms);
                    result.fn_latencies.insert(name.clone(), ms);
                }
            }
        }

        // 第二步：对每个声明了 @latency 的函数，分析调用链
        for stmt in &prog.statements {
            if let Stmt::Fn {
                name,
                latency,
                body,
                ..
            } = stmt
                && let Some(lat_str) = latency
                && let Ok(limit) = lat_str.trim_end_matches("ms").parse::<u64>()
            {
                let chain_latency = Self::analyze_call_chain(body, &declared);
                if chain_latency > limit {
                    result.errors.push(format!(
                        "延迟违规: {} 声明 {}ms，但调用链累计 {}ms（超限 {}ms）",
                        name,
                        limit,
                        chain_latency,
                        chain_latency - limit
                    ));
                }
            }
        }

        result
    }

    /// 分析函数体的调用链总延迟
    /// 遍历 body 中所有函数调用表达式，查表取得被调函数的延迟，求和。
    /// 自身开销默认 5ms。
    fn analyze_call_chain(body: &[Stmt], declared: &HashMap<String, u64>) -> u64 {
        let mut total = 5u64; // 自身基础开销 5ms
        for stmt in body {
            total += Self::stmt_latency(stmt, declared);
        }
        total
    }

    /// 计算一条语句的延迟贡献
    fn stmt_latency(stmt: &Stmt, declared: &HashMap<String, u64>) -> u64 {
        match stmt {
            Stmt::Expr(expr) => Self::expr_latency(expr, declared),
            Stmt::Return(Some(expr)) => Self::expr_latency(expr, declared),
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond = Self::expr_latency(condition, declared);
                let then_lat: u64 = then_body
                    .iter()
                    .map(|s| Self::stmt_latency(s, declared))
                    .sum();
                let else_lat: u64 = else_body
                    .iter()
                    .map(|s| Self::stmt_latency(s, declared))
                    .sum();
                cond + then_lat.max(else_lat) // 取分支中的最大延迟（最坏情况）
            }
            Stmt::While { condition, body } => {
                let cond = Self::expr_latency(condition, declared);
                let body_lat: u64 = body.iter().map(|s| Self::stmt_latency(s, declared)).sum();
                cond + body_lat
            }
            Stmt::For { iterable, body, .. } => {
                let cond = Self::expr_latency(iterable, declared);
                let body_lat: u64 = body.iter().map(|s| Self::stmt_latency(s, declared)).sum();
                cond + body_lat
            }
            Stmt::Let {
                value: Some(expr), ..
            }
            | Stmt::Const {
                value: Some(expr), ..
            } => Self::expr_latency(expr, declared),
            _ => 1, // 其他语句（assert、return、use 等）开销 1ms
        }
    }

    /// 计算一个表达式的延迟贡献
    fn expr_latency(expr: &Expr, declared: &HashMap<String, u64>) -> u64 {
        match expr {
            // 字面量、标识符 → 0ms
            Expr::IntLiteral(_)
            | Expr::FloatLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_)
            | Expr::CharLiteral(_)
            | Expr::Ident(_)
            | Expr::Array(_)
            | Expr::Range { .. }
            | Expr::OptionValue { .. }
            | Expr::ResultValue { .. } => 0,

            // 二元/一元运算 → 1ms
            Expr::BinaryOp { left, right, .. } => {
                Self::expr_latency(left, declared) + 1 + Self::expr_latency(right, declared)
            }
            Expr::UnaryOp { operand, .. } => 1 + Self::expr_latency(operand, declared),

            // 条件表达式 → 条件 + 分支中的较大者
            Expr::IfExpr(cond, then_e, else_e) => {
                Self::expr_latency(cond, declared)
                    + Self::expr_latency(then_e, declared).max(Self::expr_latency(else_e, declared))
            }

            // 函数调用 → 查被调函数延迟声明
            Expr::Call { func, args } => {
                let args_lat: u64 = args.iter().map(|a| Self::expr_latency(a, declared)).sum();
                if let Expr::Ident(name) = func.as_ref() {
                    // 查找声明的延迟
                    let fn_lat = declared.get(name.as_str()).copied().unwrap_or(10); // 未声明 → 默认 10ms
                    args_lat + fn_lat
                } else {
                    args_lat + 10 // 间接调用 → 默认 10ms
                }
            }

            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Program, Stmt};

    fn fn_stmt(name: &str, latency: Option<&str>, body: Vec<Stmt>) -> Stmt {
        Stmt::Fn {
            name: name.to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: None,
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: latency.map(|s| s.to_string()),
            timeout: None,
            throughput: None,
            body,
            async_: false,
            pub_: false,
        }
    }

    fn call_expr(name: &str) -> Expr {
        Expr::Call {
            func: Box::new(Expr::Ident(name.to_string())),
            args: vec![],
        }
    }

    #[test]
    fn test_no_latency_no_errors() {
        let mut prog = Program::new();
        prog.add(fn_stmt("f", None, vec![]));
        let result = LatencyVerifier::verify(&prog);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_simple_satisfies() {
        let mut prog = Program::new();
        prog.add(fn_stmt(
            "f",
            Some("30ms"),
            vec![Stmt::Expr(Box::new(call_expr("g")))],
        ));
        prog.add(fn_stmt("g", Some("10ms"), vec![]));
        let result = LatencyVerifier::verify(&prog);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_latency_violation() {
        let mut prog = Program::new();
        prog.add(fn_stmt(
            "f",
            Some("20ms"),
            vec![Stmt::Expr(Box::new(call_expr("g")))],
        ));
        prog.add(fn_stmt("g", Some("50ms"), vec![])); // 50ms > 20ms
        let result = LatencyVerifier::verify(&prog);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("延迟违规"));
    }

    #[test]
    fn test_multi_call_chain() {
        let mut prog = Program::new();
        // f latency(30ms): calls g(10ms) + h(10ms) + overhead(5ms) = 25ms ≤ 30ms ✓
        prog.add(fn_stmt(
            "f",
            Some("30ms"),
            vec![
                Stmt::Expr(Box::new(call_expr("g"))),
                Stmt::Expr(Box::new(call_expr("h"))),
            ],
        ));
        prog.add(fn_stmt("g", Some("10ms"), vec![]));
        prog.add(fn_stmt("h", Some("10ms"), vec![]));
        let result = LatencyVerifier::verify(&prog);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_if_branch_max_path() {
        let mut prog = Program::new();
        // if 分支：取两条路径中的最大延迟（最坏情况）
        let body = vec![Stmt::If {
            condition: Box::new(Expr::BoolLiteral(true)),
            then_body: vec![Stmt::Expr(Box::new(call_expr("g")))], // 10ms
            else_body: vec![
                Stmt::Expr(Box::new(call_expr("h"))),
                Stmt::Expr(Box::new(call_expr("h"))),
            ], // 10+10=20ms
        }];
        prog.add(fn_stmt("f", Some("30ms"), body));
        prog.add(fn_stmt("g", Some("10ms"), vec![]));
        prog.add(fn_stmt("h", Some("10ms"), vec![]));
        let result = LatencyVerifier::verify(&prog);
        // overhead(5) + cond(0) + max(then 10, else 20) = 25 ≤ 30 ✓
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_deep_chain_violation() {
        let mut prog = Program::new();
        // overhead(5) + g(10) + h(20) + i(30) = 65 > 50 ✗
        prog.add(fn_stmt(
            "f",
            Some("50ms"),
            vec![
                Stmt::Expr(Box::new(call_expr("g"))),
                Stmt::Expr(Box::new(call_expr("h"))),
                Stmt::Expr(Box::new(call_expr("i"))),
            ],
        ));
        prog.add(fn_stmt("g", Some("10ms"), vec![]));
        prog.add(fn_stmt("h", Some("20ms"), vec![]));
        prog.add(fn_stmt("i", Some("30ms"), vec![]));
        let result = LatencyVerifier::verify(&prog);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("超限"));
    }

    #[test]
    fn test_no_latency_declared_default_10ms() {
        let mut prog = Program::new();
        // h 没有 @latency，默认 10ms
        prog.add(fn_stmt(
            "f",
            Some("20ms"),
            vec![Stmt::Expr(Box::new(call_expr("h")))],
        ));
        prog.add(fn_stmt("h", None, vec![]));
        let result = LatencyVerifier::verify(&prog);
        // overhead(5) + h(10) = 15 ≤ 20 ✓
        assert!(result.errors.is_empty());
    }
}
