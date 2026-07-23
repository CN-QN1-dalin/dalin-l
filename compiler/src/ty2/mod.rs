/// Dalin L 3.0 — 七通道类型系统 (模块化拆分版)
///
/// 类型 = (值类型) × (效应类型) × (能力类型) × (置信度) × (认知循环) × (治理) × (时间约束)
/// 七通道正交，各自独立做 unification。
///
/// 模块划分：
/// - `lattice.rs` — 偏序格类型定义（Effect / Capability / Confidence / CognitiveLoop / GovernanceLevel）
/// - `confidence_inferencer.rs` — 置信度推断器
/// - `effect_inferencer.rs` — 效应推断器
/// - `capability_inferencer.rs` — 能力推断器
/// - `cognitive_loop_inferencer.rs` — 认知循环推断器
/// - `governance_inferencer.rs` — 治理推断器
/// - `time_constraint.rs` — 时间约束
/// - `mod.rs` — 七通道聚合（SevenChannelInferencer + SevenChannelType + 测试）
pub mod lattice;
pub mod confidence_inferencer;
pub mod effect_inferencer;
pub mod capability_inferencer;
pub mod cognitive_loop_inferencer;
pub mod governance_inferencer;
pub mod time_constraint;

use crate::ast::{BaseType, Program, Stmt, TypeRef};
use std::collections::HashMap;

// Re-export all public types from submodules for backward compatibility
pub use lattice::*;
pub use confidence_inferencer::ConfidenceInferencer;
pub use effect_inferencer::EffectInferencer;
pub use capability_inferencer::CapabilityInferencer;
pub use cognitive_loop_inferencer::CognitiveLoopInferencer;
pub use governance_inferencer::GovernanceInferencer;
pub use time_constraint::{TimeConstraint, TimeConstraintInferencer};

/// 七通道类型（值 × 效应 × 能力 × 置信度 × 认知循环 × 治理 × 时间）
#[derive(Debug, Clone)]
pub struct SevenChannelType {
    pub value: Option<TypeRef>,
    pub effect: Option<Effect>,
    pub capability: Option<Capability>,
    pub confidence: Option<Confidence>,
    pub cognitive_loop: Option<CognitiveLoop>,
    pub governance: Option<GovernanceLevel>,
    pub time_constraint: Option<TimeConstraint>,
}

impl Default for SevenChannelType {
    fn default() -> Self { Self::new() }
}

impl SevenChannelType {
    pub fn new() -> Self {
        Self {
            value: None, effect: None, capability: None,
            confidence: None, cognitive_loop: None,
            governance: None, time_constraint: None,
        }
    }
    pub fn value(typ: TypeRef) -> Self {
        Self { value: Some(typ), ..Self::new() }
    }
}

impl std::fmt::Display for SevenChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(v) = &self.value { write!(f, "{}", v)?; } else { write!(f, "?")?; }
        if let Some(e) = &self.effect { write!(f, " @ {}", e)?; }
        if let Some(c) = &self.capability { write!(f, " @ {}", c)?; }
        if let Some(conf) = &self.confidence { write!(f, " @ {}", conf)?; }
        if let Some(cl) = &self.cognitive_loop { write!(f, " @ loop({})", cl)?; }
        if let Some(g) = &self.governance { write!(f, " @ gov({})", g)?; }
        if let Some(tc) = &self.time_constraint { write!(f, " @ [{}]", tc)?; }
        Ok(())
    }
}

// ═══════════════════════════════════════
//  解析辅助函数（从 AST 注解字符串 → 枚举）
// ═══════════════════════════════════════

pub fn parse_effect(s: &str) -> Effect {
    match s {
        "pure" => Effect::Pure, "io" => Effect::Io,
        "async" => Effect::Async, "spawn" => Effect::Spawn,
        _ => Effect::Pure,
    }
}

pub fn parse_confidence(s: &str) -> Confidence {
    match s {
        "proven" => Confidence::Proven, "verified" => Confidence::Verified,
        "inferred" => Confidence::Inferred, "generated" => Confidence::Generated,
        "uncertain" => Confidence::Uncertain, _ => Confidence::Uncertain,
    }
}

pub fn parse_capability(s: &str) -> Capability {
    match s {
        "cpu" => Capability::Cpu, "gpu" => Capability::Gpu,
        "sfa" => Capability::Sfa, "net" => Capability::Net,
        _ => Capability::Cpu,
    }
}

pub fn parse_cognitive_loop(s: &str) -> CognitiveLoop {
    match s {
        "perceive" => CognitiveLoop::Perceive, "reason" => CognitiveLoop::Reason,
        "decide" => CognitiveLoop::Decide, "act" => CognitiveLoop::Act,
        "loop" => CognitiveLoop::Loop, _ => CognitiveLoop::Perceive,
    }
}

pub fn parse_governance(s: &str) -> GovernanceLevel {
    match s {
        "prepare" => GovernanceLevel::Prepare, "suggest" => GovernanceLevel::Suggest,
        "approve" => GovernanceLevel::Approve, "execute" => GovernanceLevel::Execute,
        _ => GovernanceLevel::Prepare,
    }
}

// ═══════════════════════════════════════
//  七通道聚合推断器
// ═══════════════════════════════════════

#[derive(Debug)]
pub struct SevenChannelInferencer {
    pub value: super::ty::TypeInferencer,
    pub effect: EffectInferencer,
    pub capability: CapabilityInferencer,
    pub confidence: ConfidenceInferencer,
    pub cognitive_loop: CognitiveLoopInferencer,
    pub governance: GovernanceInferencer,
    pub time_constraint: TimeConstraintInferencer,
    pub results: Vec<(String, SevenChannelType)>,
}

impl Default for SevenChannelInferencer {
    fn default() -> Self { Self::new() }
}

impl SevenChannelInferencer {
    pub fn new() -> Self {
        Self {
            value: super::ty::TypeInferencer::new(),
            effect: EffectInferencer::new(),
            capability: CapabilityInferencer::new(),
            confidence: ConfidenceInferencer::new(),
            cognitive_loop: CognitiveLoopInferencer::new(),
            governance: GovernanceInferencer::new(),
            time_constraint: TimeConstraintInferencer::new(),
            results: Vec::new(),
        }
    }

    pub fn infer_program(&mut self, prog: &Program) {
        let value_types: HashMap<String, TypeRef> = self
            .value
            .infer_program(prog)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for stmt in &prog.statements {
            if let Stmt::Fn {
                name, effect, capability, confidence,
                cognitive_loop, governance, latency, timeout, throughput, body, async_, ..
            } = stmt
            {
                let eff = effect
                    .as_deref()
                    .map(parse_effect)
                    .unwrap_or_else(|| if *async_ { Effect::Async } else { Effect::Pure });
                let cap = capability.as_deref().map(parse_capability).unwrap_or(Capability::Cpu);
                let conf = confidence.as_deref().map(parse_confidence);
                let cl = cognitive_loop.as_deref().map(parse_cognitive_loop);
                let gov = governance.as_deref().map(parse_governance);
                let tc = {
                    let mut t = TimeConstraint::new();
                    if let Some(l) = latency {
                        t.latency_ms = l.trim_end_matches("ms").parse::<u64>().ok();
                    }
                    if let Some(to) = timeout {
                        t.timeout_ms = if to.ends_with("ms") {
                            to.trim_end_matches("ms").parse::<u64>().ok()
                        } else {
                            to.trim_end_matches("s").parse::<u64>().ok().map(|x| x * 1000)
                        };
                    }
                    if let Some(tp) = throughput {
                        t.throughput = tp.trim_end_matches("/s").parse::<u64>().ok();
                    }
                    t
                };

                self.capability.fn_annotations.insert(name.clone(), cap.clone());

                let value_typ = value_types.get(name).cloned();
                let (val, seven_conf) = if let Some(vt) = value_typ {
                    (Some(vt), conf)
                } else {
                    (Some(TypeRef::new(BaseType::Func)), conf)
                };

                self.results.push((
                    name.clone(),
                    SevenChannelType {
                        value: val,
                        effect: Some(eff.clone()),
                        capability: Some(cap.clone()),
                        confidence: seven_conf,
                        cognitive_loop: cl.clone(),
                        governance: gov.clone(),
                        time_constraint: if tc.latency_ms.is_some() || tc.timeout_ms.is_some() || tc.throughput.is_some() {
                            Some(tc.clone())
                        } else {
                            None
                        },
                    },
                ));

                // Body-level channel checks
                self.walk_body_and_check(body, name, &eff, &cap, &cl, &gov, &tc);
            }
        }
    }

    /// 遍历函数体，对每个表达式做全通道检查
    #[allow(clippy::too_many_arguments)]
    fn walk_body_and_check(
        &mut self, body: &[Stmt], fn_name: &str,
        declared_effect: &Effect, declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        for stmt in body {
            self.walk_stmt_for_check(
                stmt, fn_name, declared_effect, declared_capability,
                declared_cognitive_loop, declared_governance, declared_time_constraint,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_stmt_for_check(
        &mut self, stmt: &Stmt, fn_name: &str,
        declared_effect: &Effect, declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        match stmt {
            Stmt::Expr(expr) => {
                self.walk_expr_check(expr, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            Stmt::Return(Some(expr)) => {
                self.walk_expr_check(expr, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            Stmt::Let { value: Some(expr), .. } | Stmt::Const { value: Some(expr), .. } => {
                self.walk_expr_check(expr, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            Stmt::If { condition, then_body, else_body } => {
                self.walk_expr_check(condition, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                for s in then_body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
                for s in else_body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
            }
            Stmt::While { condition, body } => {
                self.walk_expr_check(condition, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                for s in body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.walk_expr_check(iterable, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                for s in body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
            }
            Stmt::Assert { condition, .. } => {
                self.walk_expr_check(condition, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            Stmt::Match { target, arms } => {
                self.walk_expr_check(target, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                for arm in arms {
                    for s in &arm.body {
                        self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                            declared_cognitive_loop, declared_governance, declared_time_constraint);
                    }
                }
            }
            Stmt::TryCatch { try_body, catch_body, .. } => {
                for s in try_body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
                for s in catch_body {
                    self.walk_stmt_for_check(s, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_expr_check(
        &mut self, expr: &crate::ast::Expr, fn_name: &str,
        declared_effect: &Effect, declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        let location = fn_name.to_string();

        let expr_eff = self.effect.infer_expr(expr);
        self.effect.check(declared_effect, &expr_eff, &location);

        let expr_cap = self.capability.infer_expr(expr);
        self.capability.check(declared_capability, &expr_cap, &location);

        if let Some(declared_cl) = declared_cognitive_loop {
            let expr_cl = self.cognitive_loop.infer_expr(expr);
            self.cognitive_loop.check(declared_cl, &expr_cl, &location);
        }

        if let Some(declared_gov) = declared_governance {
            let expr_gov = self.governance.infer_expr(expr);
            self.governance.check(declared_gov, &expr_gov, &location);
        }

        if declared_time_constraint.latency_ms.is_some() || declared_time_constraint.timeout_ms.is_some() || declared_time_constraint.throughput.is_some() {
            let inferred_tc = self.time_constraint.infer_expr(expr);
            if !inferred_tc.satisfies(declared_time_constraint) {
                self.time_constraint.check(&inferred_tc, declared_time_constraint, &location);
            }
        }

        match expr {
            crate::ast::Expr::BinaryOp { left, right, .. } => {
                self.walk_expr_check(left, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                self.walk_expr_check(right, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            crate::ast::Expr::UnaryOp { operand, .. } => {
                self.walk_expr_check(operand, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            crate::ast::Expr::Call { args, .. } => {
                for arg in args {
                    self.walk_expr_check(arg, fn_name, declared_effect, declared_capability,
                        declared_cognitive_loop, declared_governance, declared_time_constraint);
                }
            }
            crate::ast::Expr::IfExpr(cond, then_e, else_e) => {
                self.walk_expr_check(cond, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                self.walk_expr_check(then_e, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
                self.walk_expr_check(else_e, fn_name, declared_effect, declared_capability,
                    declared_cognitive_loop, declared_governance, declared_time_constraint);
            }
            _ => {}
        }
    }

    pub fn print_report(&self) -> String {
        let mut lines = vec!["\n=== Seven-Channel Type Report ===".to_string()];
        lines.push("\nInferred Types:".into());
        for (name, tct) in &self.results {
            lines.push(format!("  {}: {}", name, tct));
        }
        if !self.effect.errors.is_empty() {
            lines.push("\nEffect Errors:".into());
            for err in &self.effect.errors { lines.push(format!("  {}", err)); }
        }
        if !self.capability.errors.is_empty() {
            lines.push("\nCapability Errors:".into());
            for err in &self.capability.errors { lines.push(format!("  {}", err)); }
        }
        if !self.confidence.errors.is_empty() {
            lines.push("\nConfidence Errors:".into());
            for err in &self.confidence.errors { lines.push(format!("  {}", err)); }
        }
        if !self.cognitive_loop.errors.is_empty() {
            lines.push("\nCognitive Loop Errors:".into());
            for err in &self.cognitive_loop.errors { lines.push(format!("  {}", err)); }
        }
        if !self.governance.errors.is_empty() {
            lines.push("\nGovernance Errors:".into());
            for err in &self.governance.errors { lines.push(format!("  {}", err)); }
        }
        if !self.time_constraint.errors.is_empty() {
            lines.push("\nTime Constraint Errors:".into());
            for err in &self.time_constraint.errors { lines.push(format!("  {}", err)); }
        }
        if self.value.errors.is_empty() && self.effect.errors.is_empty() && self.capability.errors.is_empty()
            && self.confidence.errors.is_empty() && self.cognitive_loop.errors.is_empty()
            && self.governance.errors.is_empty() && self.time_constraint.errors.is_empty()
        {
            lines.push("\nNo type errors!".into());
        }
        lines.push(String::new());
        lines.join("\n")
    }

    pub fn has_errors(&self) -> bool {
        !self.value.errors.is_empty() || !self.effect.errors.is_empty()
            || !self.capability.errors.is_empty() || !self.confidence.errors.is_empty()
            || !self.cognitive_loop.errors.is_empty() || !self.governance.errors.is_empty()
            || !self.time_constraint.errors.is_empty()
    }

    pub fn collect_errors(&self) -> Vec<crate::error::ChannelError> {
        let mut errs = Vec::new();
        for e in &self.effect.errors {
            errs.push(crate::error::ChannelError::EffectViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(), required: "".into(), detail: e.clone(),
            });
        }
        for e in &self.capability.errors {
            errs.push(crate::error::ChannelError::CapabilityViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(), required: "".into(), detail: e.clone(),
            });
        }
        for e in &self.confidence.errors {
            errs.push(crate::error::ChannelError::ConfidenceViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                actual: "".into(), required: "".into(), detail: e.clone(),
            });
        }
        for e in &self.cognitive_loop.errors {
            errs.push(crate::error::ChannelError::CognitiveLoopViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(), required: "".into(), detail: e.clone(),
            });
        }
        for e in &self.governance.errors {
            errs.push(crate::error::ChannelError::GovernanceViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                required: "".into(), actual: "".into(), detail: e.clone(),
            });
        }
        for e in &self.time_constraint.errors {
            errs.push(crate::error::ChannelError::LatencyViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                declared_ms: 0, actual_ms: 0, detail: e.clone(),
            });
        }
        for e in &self.value.errors {
            errs.push(crate::error::ChannelError::TypeError {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                message: e.message.clone(),
            });
        }
        errs
    }
}

// ── Tests moved to dedicated test modules ──
#[cfg(test)]
mod tests;
