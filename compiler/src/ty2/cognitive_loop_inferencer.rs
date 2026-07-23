use crate::ast::Expr;
use super::lattice::CognitiveLoop;

#[derive(Debug)]
pub struct CognitiveLoopInferencer { pub errors: Vec<String> }

impl Default for CognitiveLoopInferencer { fn default() -> Self { Self::new() } }

impl CognitiveLoopInferencer {
    pub fn new() -> Self { Self { errors: Vec::new() } }

    pub fn infer_expr(&mut self, expr: &Expr) -> CognitiveLoop {
        use crate::ast::Expr;
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) | Expr::Ident(_)
            | Expr::Array(_) | Expr::Range { .. } | Expr::OptionValue { .. } | Expr::ResultValue { .. } => CognitiveLoop::Perceive,
            Expr::BinaryOp { .. } | Expr::UnaryOp { .. } => CognitiveLoop::Reason,
            Expr::IfExpr { .. } | Expr::MatchExpr { .. } => CognitiveLoop::Decide,
            Expr::Call { func, args } => {
                if let Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        "sfa_encode" | "sfa_query" | "sfa_attend" | "llm_infer" => CognitiveLoop::Loop,
                        _ => args.iter().map(|a| self.infer_expr(a)).reduce(|a, b| CognitiveLoop::join(&a, &b)).unwrap_or(CognitiveLoop::Act),
                    }
                } else {
                    CognitiveLoop::Act
                }
            }
            _ => CognitiveLoop::Perceive,
        }
    }

    pub fn check(&mut self, context: &CognitiveLoop, expr_loop: &CognitiveLoop, location: &str) {
        if !expr_loop.leq(context) {
            self.errors.push(format!(
                "认知循环违规: {} 需要 {}，但上下文要求 {}", location, expr_loop, context
            ));
        }
    }
}
