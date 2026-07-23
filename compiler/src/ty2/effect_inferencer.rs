use crate::ast::Expr;

#[derive(Debug)]
pub struct EffectInferencer { pub errors: Vec<String> }

impl Default for EffectInferencer { fn default() -> Self { Self::new() } }

impl EffectInferencer {
    pub fn new() -> Self { Self { errors: Vec::new() } }

    pub fn infer_expr(&mut self, expr: &Expr) -> super::lattice::Effect {
        use crate::ast::Expr;
        use crate::ast::Stmt;
        use super::lattice::Effect;
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) | Expr::Ident(_)
            | Expr::Array(_) | Expr::Range { .. } | Expr::OptionValue { .. } | Expr::ResultValue { .. } => Effect::Pure,
            Expr::BinaryOp { .. } | Expr::UnaryOp { .. } => Effect::Pure,
            Expr::Call { func, args } => {
                let mut eff = Effect::Pure;
                for a in args { let ae = self.infer_expr(a); eff = Effect::join(&eff, &ae).unwrap_or(Effect::Async); }
                if let Expr::Ident(name) = func.as_ref()
                    && (name == "println" || name == "print")
                {
                    eff = Effect::join(&eff, &Effect::Io).unwrap_or(Effect::Io);
                }
                eff
            }
            Expr::IfExpr(_, t, e) => {
                let te = self.infer_expr(t); let ee = self.infer_expr(e);
                Effect::join(&te, &ee).unwrap_or(Effect::Async)
            }
            Expr::MatchExpr(_, arms) => {
                let mut eff = Effect::Pure;
                for arm in arms {
                    for s in &arm.body {
                        if let Stmt::Expr(e) = s { eff = Effect::join(&eff, &self.infer_expr(e)).unwrap_or(Effect::Async); }
                    }
                }
                eff
            }
            _ => Effect::Pure,
        }
    }

    pub fn check(&mut self, context: &super::lattice::Effect, expr_eff: &super::lattice::Effect, location: &str) {
        if !expr_eff.leq(context) {
            self.errors.push(format!(
                "效应违规: {} 需要 {}，但上下文要求 {}", location, expr_eff, context
            ));
        }
    }
}
