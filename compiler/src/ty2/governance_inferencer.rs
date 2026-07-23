use crate::ast::Expr;
use super::lattice::GovernanceLevel;

#[derive(Debug)]
pub struct GovernanceInferencer { pub errors: Vec<String> }

impl Default for GovernanceInferencer { fn default() -> Self { Self::new() } }

impl GovernanceInferencer {
    pub fn new() -> Self { Self { errors: Vec::new() } }

    pub fn infer_expr(&mut self, expr: &Expr) -> GovernanceLevel {
        use crate::ast::Expr;
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) | Expr::Ident(_)
            | Expr::Array(_) | Expr::Range { .. } | Expr::OptionValue { .. } | Expr::ResultValue { .. } => GovernanceLevel::Prepare,
            Expr::BinaryOp { .. } | Expr::UnaryOp { .. } | Expr::IfExpr { .. } | Expr::MatchExpr { .. } => GovernanceLevel::Suggest,
            Expr::Call { func, args } => {
                if let Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        "write" | "delete" | "update" | "charge" | "pay" | "send_money"
                        | "delete_user" | "modify_permissions" => GovernanceLevel::Approve,
                        "execute" | "deploy" | "shutdown" | "format" | "exec" => GovernanceLevel::Execute,
                        _ => args.iter().map(|a| self.infer_expr(a)).reduce(|a, b| GovernanceLevel::join(&a, &b)).unwrap_or(GovernanceLevel::Prepare),
                    }
                } else {
                    GovernanceLevel::Suggest
                }
            }
            _ => GovernanceLevel::Prepare,
        }
    }

    pub fn check(&mut self, required: &GovernanceLevel, actual: &GovernanceLevel, location: &str) {
        if !actual.leq(required) {
            self.errors.push(format!(
                "治理违规: {} 需要 {} 权限，但当前只有 {}", location, actual, required
            ));
        }
    }
}
