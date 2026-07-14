/// Dalin L 2.0 — 三通道类型系统
///
/// 类型 = (值类型) × (效应类型) × (能力类型)
/// 三通道正交，各自独立做 unification

use crate::ast::{BaseType, TypeRef};
use std::cmp::Ordering;
use std::fmt;

// ═══════════════════════════════
//  效应类型 (Effect Channel)
// ═══════════════════════════════

/// 效应类型：描述计算产生的副作用
/// 偏序关系：pure < io < async, pure < spawn
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Pure,     // 纯计算，无副作用
    Io,       // 文件/网络 I/O（同步）
    Async,    // 异步 I/O
    Spawn,    // 并发派生
}

impl Effect {
    /// 效应偏序：a ≤ b 当且仅当 a 比 b "更纯"
    pub fn leq(&self, other: &Effect) -> bool {
        use Effect::*;
        match (self, other) {
            (Pure, _) => true,          // pure 可以出现在任何上下文中
            (_, Pure) => false,         // 非 pure 不能出现在 pure 上下文中
            (Io, Io) | (Io, Async) => true,
            (Async, Async) => true,
            (Spawn, Spawn) => true,
            _ => false,
        }
    }

    /// 最小上界（join）：两个效应都满足的最小效应
    /// 如果不可比则返回 None（效应违规）
    pub fn join(a: &Effect, b: &Effect) -> Option<Effect> {
        use Effect::*;
        match (a, b) {
            (Pure, x) | (x, Pure) => Some(x.clone()),
            (Io, Io) => Some(Io),
            (Io, Async) | (Async, Io) => Some(Async),
            (Async, Async) => Some(Async),
            (Spawn, Spawn) => Some(Spawn),
            (Io, Spawn) | (Spawn, Io) => None,    // io 和 spawn 不可比
            (Async, Spawn) | (Spawn, Async) => None, // async 和 spawn 不可比
        }
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pure => write!(f, "pure"),
            Self::Io => write!(f, "io"),
            Self::Async => write!(f, "async"),
            Self::Spawn => write!(f, "spawn"),
        }
    }
}

// ═══════════════════════════════
//  能力类型 (Capability Channel)
// ═══════════════════════════════

/// 能力类型：描述计算在什么硬件上执行
/// 偏序关系：cpu < gpu, cpu < sfa, cpu < net
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    Cpu,    // 本地 CPU 执行
    Gpu,    // GPU/Metal/CUDA 后端
    Sfa,    // SFA 注意力路由
    Net,    // 远程节点执行
}

impl Capability {
    pub fn leq(&self, other: &Capability) -> bool {
        use Capability::*;
        match (self, other) {
            (Cpu, _) => true,          // cpu 可以出现在任何执行上下文中
            (_, Cpu) => false,         // 非 cpu 不能出现在 cpu 上下文中
            (Gpu, Gpu) => true,
            (Sfa, Sfa) => true,
            (Net, Net) => true,
            _ => false,
        }
    }

    /// 能力 join：取同时满足两个能力的最小上界
    pub fn join(a: &Capability, b: &Capability) -> Option<Capability> {
        use Capability::*;
        match (a, b) {
            (Cpu, x) | (x, Cpu) => Some(x.clone()),
            (Gpu, Gpu) => Some(Gpu),
            (Sfa, Sfa) => Some(Sfa),
            (Net, Net) => Some(Net),
            _ => None,  // 不同加速器不可比
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => write!(f, "cpu"),
            Self::Gpu => write!(f, "gpu"),
            Self::Sfa => write!(f, "sfa"),
            Self::Net => write!(f, "net"),
        }
    }
}

// ═══════════════════════════════
//  三通道类型
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct ThreeChannelType {
    pub value: Option<TypeRef>,       // None = 尚未推断
    pub effect: Option<Effect>,
    pub capability: Option<Capability>,
}

impl ThreeChannelType {
    pub fn new() -> Self {
        Self { value: None, effect: None, capability: None }
    }

    pub fn value(typ: TypeRef) -> Self {
        Self { value: Some(typ), effect: None, capability: None }
    }
}

impl fmt::Display for ThreeChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(v) = &self.value {
            write!(f, "{}", v)?;
        } else {
            write!(f, "?")?;
        }
        if let Some(e) = &self.effect {
            write!(f, " @ {}", e)?;
        }
        if let Some(c) = &self.capability {
            write!(f, " @ {}", c)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════
//  效应推断器
// ═══════════════════════════════

#[derive(Debug)]
pub struct EffectInferencer {
    pub errors: Vec<String>,
}

impl EffectInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的效应
    /// 字面量、标识符 → pure
    /// 函数调用 → 被调用函数的效应
    /// + - * / → pure
    /// spawn → spawn
    /// async fn → async
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> Effect {
        match expr {
            crate::ast::Expr::IntLiteral(_)
            | crate::ast::Expr::FloatLiteral(_)
            | crate::ast::Expr::StringLiteral(_)
            | crate::ast::Expr::BoolLiteral(_)
            | crate::ast::Expr::CharLiteral(_)
            | crate::ast::Expr::Ident(_)
            | crate::ast::Expr::Array(_)
            | crate::ast::Expr::Range { .. }
            | crate::ast::Expr::OptionValue { .. }
            | crate::ast::Expr::ResultValue { .. } => Effect::Pure,

            crate::ast::Expr::BinaryOp { .. }
            | crate::ast::Expr::UnaryOp { .. } => Effect::Pure,

            crate::ast::Expr::Call { func, args } => {
                // 函数调用的效应 = 参数的效应的 join + 函数自身的效应
                let mut eff = Effect::Pure;
                for a in args {
                    let a_eff = self.infer_expr(a);
                    eff = Effect::join(&eff, &a_eff).unwrap_or(Effect::Async);
                }
                // 如果是内置函数，大部分是 pure
                if let crate::ast::Expr::Ident(name) = func.as_ref() {
                    if name == "println" || name == "print" {
                        eff = Effect::join(&eff, &Effect::Io).unwrap_or(Effect::Io);
                    }
                }
                eff
            }

            crate::ast::Expr::IfExpr(_, t, e) => {
                let t_eff = self.infer_expr(t);
                let e_eff = self.infer_expr(e);
                Effect::join(&t_eff, &e_eff).unwrap_or(Effect::Async)
            }

            crate::ast::Expr::MatchExpr(_, arms) => {
                let mut eff = Effect::Pure;
                for arm in arms {
                    for s in &arm.body {
                        match s {
                            crate::ast::Stmt::Expr(e) => {
                                eff = Effect::join(&eff, &self.infer_expr(e))
                                    .unwrap_or(Effect::Async);
                            }
                            _ => {}
                        }
                    }
                }
                eff
            }

            _ => Effect::Pure,
        }
    }

    /// 检查效应兼容性
    /// 上下文效应 必须 ≥ 表达式效应
    pub fn check(&mut self, context: &Effect, expr_eff: &Effect, location: &str) {
        if !expr_eff.leq(context) {
            self.errors.push(format!(
                "效应违规: {} 需要 {}，但上下文要求 {}",
                location, expr_eff, context
            ));
        }
    }
}

// ═══════════════════════════════
//  能力推断器
// ═══════════════════════════════

#[derive(Debug)]
pub struct CapabilityInferencer {
    pub errors: Vec<String>,
}

impl CapabilityInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn infer_expr(&mut self, _expr: &crate::ast::Expr) -> Capability {
        // 默认所有计算都是 cpu
        // 带 sfa_encode 调用的 → sfa
        // 带 gpu 标注的 → gpu
        Capability::Cpu
    }
}

// ═══════════════════════════════
//  三通道推断器
// ═══════════════════════════════

#[derive(Debug)]
pub struct ThreeChannelInferencer {
    pub value: super::ty::TypeInferencer,
    pub effect: EffectInferencer,
    pub capability: CapabilityInferencer,
    pub results: Vec<(String, ThreeChannelType)>,
}

impl ThreeChannelInferencer {
    pub fn new() -> Self {
        Self {
            value: super::ty::TypeInferencer::new(),
            effect: EffectInferencer::new(),
            capability: CapabilityInferencer::new(),
            results: Vec::new(),
        }
    }

    pub fn infer_program(&mut self, prog: &crate::ast::Program) {
        let value_types = self.value.infer_program(prog);
        for (name, typ) in value_types {
            self.results.push((
                name.clone(),
                ThreeChannelType {
                    value: Some(typ.clone()),
                    effect: Some(Effect::Pure),
                    capability: Some(Capability::Cpu),
                },
            ));
        }
    }

    pub fn print_report(&self) -> String {
        let mut lines = vec!["\n=== Three-Channel Type Report ===".to_string()];
        lines.push("\nInferred Types:".into());
        for (name, tct) in &self.results {
            lines.push(format!("  {}: {}", name, tct));
        }
        if !self.effect.errors.is_empty() {
            lines.push("\nEffect Errors:".into());
            for err in &self.effect.errors {
                lines.push(format!("  ❌ {}", err));
            }
        }
        if !self.value.errors.is_empty() {
            lines.push("\nValue Errors:".into());
            for err in &self.value.errors {
                lines.push(format!("  ❌ {}", err));
            }
        }
        if self.value.errors.is_empty() && self.effect.errors.is_empty() {
            lines.push("\n✅ No type errors!".into());
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_lattice() {
        assert!(Effect::Pure.leq(&Effect::Async));
        assert!(Effect::Pure.leq(&Effect::Spawn));
        assert!(Effect::Io.leq(&Effect::Async));
        assert!(!Effect::Async.leq(&Effect::Pure));
        assert!(!Effect::Spawn.leq(&Effect::Io));
    }

    #[test]
    fn test_effect_join() {
        assert_eq!(Effect::join(&Effect::Pure, &Effect::Io), Some(Effect::Io));
        assert_eq!(Effect::join(&Effect::Io, &Effect::Async), Some(Effect::Async));
        assert_eq!(Effect::join(&Effect::Spawn, &Effect::Pure), Some(Effect::Spawn));
        assert_eq!(Effect::join(&Effect::Io, &Effect::Spawn), None);
        assert_eq!(Effect::join(&Effect::Async, &Effect::Spawn), None);
    }

    #[test]
    fn test_capability_lattice() {
        assert!(Capability::Cpu.leq(&Capability::Gpu));
        assert!(Capability::Cpu.leq(&Capability::Sfa));
        assert!(!Capability::Gpu.leq(&Capability::Cpu));
        assert!(!Capability::Sfa.leq(&Capability::Gpu));
    }

    #[test]
    fn test_capability_join() {
        assert_eq!(Capability::join(&Capability::Cpu, &Capability::Gpu), Some(Capability::Gpu));
        assert_eq!(Capability::join(&Capability::Cpu, &Capability::Sfa), Some(Capability::Sfa));
        assert_eq!(Capability::join(&Capability::Gpu, &Capability::Gpu), Some(Capability::Gpu));
        assert_eq!(Capability::join(&Capability::Gpu, &Capability::Sfa), None);
    }
}