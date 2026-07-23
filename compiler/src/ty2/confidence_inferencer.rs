use crate::ast::Expr;

#[derive(Debug)]
pub struct ConfidenceInferencer { pub errors: Vec<String> }

impl Default for ConfidenceInferencer { fn default() -> Self { Self::new() } }

impl ConfidenceInferencer {
    pub fn new() -> Self { Self { errors: Vec::new() } }

    /// 推断表达式的置信度。字面量→Proven，标识符→Uncertain（被 SevenChannelInferencer 覆盖），
    /// 函数调用→查内置映射。
    pub fn infer_expr(&mut self, expr: &Expr) -> super::lattice::Confidence {
        use crate::ast::Expr;
        use crate::ast::Stmt;
        use super::lattice::Confidence;
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) => Confidence::Proven,
            Expr::Range { .. } => Confidence::Proven,
            Expr::OptionValue { .. } => Confidence::Inferred,
            Expr::ResultValue { .. } => Confidence::Inferred,
            Expr::Array(items) => items.iter().map(|e| self.infer_expr(e)).reduce(|a, b| Confidence::join(&a, &b)).unwrap_or(Confidence::Proven),
            Expr::Ident(_) => Confidence::Uncertain,
            Expr::BinaryOp { left, right, .. } => {
                let l = self.infer_expr(left); let r = self.infer_expr(right);
                Confidence::join(&l, &r)
            }
            Expr::UnaryOp { operand, .. } => self.infer_expr(operand),
            Expr::Call { func, args } => {
                let mut conf = Confidence::Proven;
                for a in args { let ac = self.infer_expr(a); conf = Confidence::join(&conf, &ac); }
                if let Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        "llm_generate" | "llm_complete" | "llm_embed" => conf = Confidence::join(&conf, &Confidence::Generated),
                        "verify" | "validate" | "check" => conf = Confidence::join(&conf, &Confidence::Verified),
                        "prove" | "formal_verify" => conf = Confidence::join(&conf, &Confidence::Proven),
                        _ => conf = Confidence::join(&conf, &Confidence::Inferred),
                    }
                } else {
                    conf = Confidence::join(&conf, &Confidence::Inferred);
                }
                conf
            }
            Expr::IfExpr(_, t, e) => {
                let tc = self.infer_expr(t); let ec = self.infer_expr(e);
                Confidence::join(&tc, &ec)
            }
            Expr::MatchExpr(_, arms) => {
                let mut conf = Confidence::Proven;
                for arm in arms {
                    for s in &arm.body {
                        if let Stmt::Expr(e) = s { conf = Confidence::join(&conf, &self.infer_expr(e)); }
                    }
                }
                conf
            }
            _ => Confidence::Uncertain,
        }
    }

    pub fn check(&mut self, actual: &super::lattice::Confidence, required: &super::lattice::Confidence, location: &str) {
        if !required.leq(actual) {
            self.errors.push(format!(
                "置信度不足: {} 需要 {}，但实际只有 {}（score: {} < {})",
                location, required, actual, actual.score(), required.score()
            ));
        }
    }
}
