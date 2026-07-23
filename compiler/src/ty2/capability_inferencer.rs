use crate::ast::{Expr, Stmt};
use std::collections::HashMap;
use super::lattice::Capability;

#[derive(Debug)]
pub struct CapabilityInferencer {
    pub errors: Vec<String>,
    /// 函数名 → 能力标注（由 SevenChannelInferencer 填充）
    pub fn_annotations: HashMap<String, Capability>,
}

impl Default for CapabilityInferencer { fn default() -> Self { Self::new() } }

impl CapabilityInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new(), fn_annotations: HashMap::new() }
    }

    pub fn infer_expr(&mut self, expr: &Expr) -> Capability {
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) | Expr::Ident(_)
            | Expr::Array(_) | Expr::Range { .. } | Expr::OptionValue { .. } | Expr::ResultValue { .. } => Capability::Cpu,
            Expr::BinaryOp { .. } | Expr::UnaryOp { .. } => Capability::Cpu,
            Expr::Call { func, args } => {
                let mut cap = Capability::Cpu;
                for a in args { let ac = self.infer_expr(a); cap = Capability::join(&cap, &ac).unwrap_or(Capability::Cpu); }
                if let Expr::Ident(name) = func.as_ref() {
                    let fn_cap = self.builtin_capability(name).or_else(|| self.fn_annotations.get(name).cloned()).unwrap_or(Capability::Cpu);
                    cap = Capability::join(&cap, &fn_cap).unwrap_or(Capability::Cpu);
                }
                cap
            }
            Expr::IfExpr(_, t, e) => {
                let tc = self.infer_expr(t); let ec = self.infer_expr(e);
                Capability::join(&tc, &ec).unwrap_or(Capability::Cpu)
            }
            Expr::MatchExpr(_, arms) => {
                let mut cap = Capability::Cpu;
                for arm in arms {
                    for s in &arm.body {
                        if let Stmt::Expr(e) = s { cap = Capability::join(&cap, &self.infer_expr(e)).unwrap_or(Capability::Cpu); }
                    }
                }
                cap
            }
            _ => Capability::Cpu,
        }
    }

    pub fn check(&mut self, context: &Capability, expr_cap: &Capability, location: &str) {
        if !expr_cap.leq(context) {
            self.errors.push(format!(
                "能力违规: {} 需要 {:?}，但上下文只允许 {:?}", location, expr_cap, context
            ));
        }
    }

    fn builtin_capability(&self, name: &str) -> Option<Capability> {
        match name {
            "sfa_encode" | "sfa_query" | "sfa_attend" => Some(Capability::Sfa),
            n if n.starts_with("gpu_") => Some(Capability::Gpu),
            n if n.starts_with("net_") => Some(Capability::Net),
            _ => None,
        }
    }
}
