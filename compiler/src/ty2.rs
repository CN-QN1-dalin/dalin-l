#![allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
/// Dalin L 3.0 — 七通道类型系统
///
/// 类型 = (值类型) × (效应类型) × (能力类型)
/// 七通道正交，各自独立做 unification
use crate::ast::{BaseType, Stmt, TypeRef};
use std::collections::HashMap;
use std::fmt;

/// 把 AST 上的效应注解字符串解析为 `Effect` 枚举。
/// 未知/缺失注解安全回落到最严格的 `Pure`（最小权限默认）。
pub fn parse_effect(s: &str) -> Effect {
    match s {
        "pure" => Effect::Pure,
        "io" => Effect::Io,
        "async" => Effect::Async,
        "spawn" => Effect::Spawn,
        _ => Effect::Pure,
    }
}

/// 把 AST 上的能力注解字符串解析为 `Capability` 枚举。
/// 未知/缺失注解安全回落到最通用的 `Cpu`（默认本地执行）。
pub fn parse_confidence(s: &str) -> Confidence {
    match s {
        "proven" => Confidence::Proven,
        "verified" => Confidence::Verified,
        "inferred" => Confidence::Inferred,
        "generated" => Confidence::Generated,
        "uncertain" => Confidence::Uncertain,
        _ => {
            // 带数值的格式：@confidence(>0.9) 将在解析器层面处理
            // 这里只处理名称映射
            Confidence::Uncertain
        }
    }
}

/// 把 AST 上的能力注解字符串解析为 `Capability` 枚举。
/// 未知/缺失注解安全回落到最通用的 `Cpu`（默认本地执行）。
pub fn parse_capability(s: &str) -> Capability {
    match s {
        "cpu" => Capability::Cpu,
        "gpu" => Capability::Gpu,
        "sfa" => Capability::Sfa,
        "net" => Capability::Net,
        _ => Capability::Cpu,
    }
}

// ═══════════════════════════════
//  效应类型 (Effect Channel)
// ═══════════════════════════════

/// 效应类型：描述计算产生的副作用
/// 偏序关系：pure < io < async, pure < spawn
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    Pure,  // 纯计算，无副作用
    Io,    // 文件/网络 I/O（同步）
    Async, // 异步 I/O
    Spawn, // 并发派生
}

impl Effect {
    /// 效应偏序：a ≤ b 当且仅当 a 比 b "更纯"
    pub fn leq(&self, other: &Effect) -> bool {
        use Effect::*;
        match (self, other) {
            (Pure, _) => true,  // pure 可以出现在任何上下文中
            (_, Pure) => false, // 非 pure 不能出现在 pure 上下文中
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
            (Io, Spawn) | (Spawn, Io) => None, // io 和 spawn 不可比
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
    Cpu, // 本地 CPU 执行
    Gpu, // GPU/Metal/CUDA 后端
    Sfa, // SFA 注意力路由
    Net, // 远程节点执行
}

impl Capability {
    pub fn leq(&self, other: &Capability) -> bool {
        use Capability::*;
        match (self, other) {
            (Cpu, _) => true,  // cpu 可以出现在任何执行上下文中
            (_, Cpu) => false, // 非 cpu 不能出现在 cpu 上下文中
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
            _ => None, // 不同加速器不可比
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
//  置信度通道 (Confidence Channel)
// ═══════════════════════════════

/// 置信度类型：描述一个值的可信程度和来源。
/// 偏序关系：Uncertain < Generated < Inferred < Verified < Proven
/// 格最小上界 (join)：取置信度更低的（最保守的估计）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Confidence {
    /// 经过形式化证明或数学验证 (P = 1.0)
    Proven,
    /// 经过人工或自动验证流程确认 (P ~ 0.95)
    Verified,
    /// 从已知事实推导得出 (P ~ 0.85)
    Inferred,
    /// 由 LLM/生成模型产生 (P ~ 0.7)
    Generated,
    /// 未知或无法判断的默认值 (P ~ 0.5)
    Uncertain,
}

impl Confidence {
    /// 置信度偏序：a <= b 表示 a 比 b "更不确定"
    /// Proven > Verified > Inferred > Generated > Uncertain
    pub fn leq(&self, other: &Confidence) -> bool {
        use Confidence::*;
        match (self, other) {
            (Uncertain, _) => true,
            (_, Uncertain) => false,
            (Generated, x)
                if *x == Generated || *x == Inferred || *x == Verified || *x == Proven =>
            {
                true
            }
            (Generated, _) => false,
            (Inferred, x) if *x == Inferred || *x == Verified || *x == Proven => true,
            (Inferred, _) => false,
            (Verified, x) if *x == Verified || *x == Proven => true,
            (Verified, _) => false,
            (Proven, Proven) => true,
            (Proven, _) => false,
        }
    }

    /// 置信度 join：取两个中最不确定的（最保守估计）
    pub fn join(a: &Confidence, b: &Confidence) -> Confidence {
        use Confidence::*;
        let order = |c: &Confidence| -> u8 {
            match c {
                Proven => 4,
                Verified => 3,
                Inferred => 2,
                Generated => 1,
                Uncertain => 0,
            }
        };
        if order(a) <= order(b) {
            a.clone()
        } else {
            b.clone()
        }
    }

    /// 置信度的数值表达（用于报告和接口消费）
    pub fn score(&self) -> f64 {
        match self {
            Confidence::Proven => 1.0,
            Confidence::Verified => 0.95,
            Confidence::Inferred => 0.85,
            Confidence::Generated => 0.7,
            Confidence::Uncertain => 0.5,
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proven => write!(f, "proven"),
            Self::Verified => write!(f, "verified"),
            Self::Inferred => write!(f, "inferred"),
            Self::Generated => write!(f, "generated"),
            Self::Uncertain => write!(f, "uncertain"),
        }
    }
}

// ═══════════════════════════════
//  认知循环类型 (Cognitive Loop Channel)
// ═══════════════════════════════

/// 认知循环五阶：描述一个函数在认知架构中的阶段
/// 偏序关系：Perceive < Reason < Decide < Act < Loop
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CognitiveLoop {
    Perceive, // 感知：接收外部输入（传感器/用户/事件）
    Reason,   // 推理：处理信息、建立因果链
    Decide,   // 决策：基于推理做选择
    Act,      // 行动：执行动作，改变状态
    Loop,     // 闭环：完整认知循环，进入下一轮感知
}

impl CognitiveLoop {
    /// 认知循环偏序
    pub fn leq(&self, other: &CognitiveLoop) -> bool {
        use CognitiveLoop::*;
        match (self, other) {
            (Perceive, _) => true,
            (Reason, x) if *x == Reason || *x == Decide || *x == Act || *x == Loop => true,
            (_, Reason) => false,
            (Decide, x) if *x == Decide || *x == Act || *x == Loop => true,
            (_, Decide) => false,
            (Act, x) if *x == Act || *x == Loop => true,
            (_, Act) => false,
            (Loop, Loop) => true,
            _ => false,
        }
    }

    /// 认知循环 join：取最高阶的
    pub fn join(a: &CognitiveLoop, b: &CognitiveLoop) -> CognitiveLoop {
        use CognitiveLoop::*;
        let order = |c: &CognitiveLoop| -> u8 {
            match c {
                Perceive => 0,
                Reason => 1,
                Decide => 2,
                Act => 3,
                Loop => 4,
            }
        };
        if order(a) >= order(b) {
            a.clone()
        } else {
            b.clone()
        }
    }
}

impl fmt::Display for CognitiveLoop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Perceive => write!(f, "perceive"),
            Self::Reason => write!(f, "reason"),
            Self::Decide => write!(f, "decide"),
            Self::Act => write!(f, "act"),
            Self::Loop => write!(f, "loop"),
        }
    }
}

// ═══════════════════════════════
//  治理通道 (Governance Channel)
// ═══════════════════════════════

/// 治理级别：描述一个函数的操作权限和审批要求
/// 偏序关系：prepare < suggest < approve < execute
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GovernanceLevel {
    Prepare, // 准备：可以查询、验证、准备数据，但不执行实际操作
    Suggest, // 建议：提出建议方案，需要人工确认
    Approve, // 审批：需要上级审批后才能执行
    Execute, // 执行：直接执行（最严格权限）
}

impl GovernanceLevel {
    pub fn leq(&self, other: &GovernanceLevel) -> bool {
        use GovernanceLevel::*;
        match (self, other) {
            (Prepare, _) => true,  // prepare 可以出现在任何上下文
            (_, Prepare) => false, // 非 prepare 不能出现在 prepare 上下文
            (Suggest, x) if *x == Suggest || *x == Approve || *x == Execute => true,
            (_, Suggest) => false,
            (Approve, x) if *x == Approve || *x == Execute => true,
            (_, Approve) => false,
            (Execute, Execute) => true,
            _ => false,
        }
    }

    /// 治理级别 join：取更高权限（更严格）
    pub fn join(a: &GovernanceLevel, b: &GovernanceLevel) -> GovernanceLevel {
        use GovernanceLevel::*;
        let order = |g: &GovernanceLevel| -> u8 {
            match g {
                Prepare => 0,
                Suggest => 1,
                Approve => 2,
                Execute => 3,
            }
        };
        if order(a) >= order(b) {
            a.clone()
        } else {
            b.clone()
        }
    }
}

impl fmt::Display for GovernanceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prepare => write!(f, "prepare"),
            Self::Suggest => write!(f, "suggest"),
            Self::Approve => write!(f, "approve"),
            Self::Execute => write!(f, "execute"),
        }
    }
}

// ═══════════════════════════════
//  时间通道 (Time Channel)
// ═══════════════════════════════

/// 时间约束：描述函数的延迟/超时/吞吐量保证
#[derive(Debug, Clone, PartialEq)]
pub struct TimeConstraint {
    /// 最大延迟（毫秒）
    pub latency_ms: Option<u64>,
    /// 超时时间（毫秒，0 = 无限制）
    pub timeout_ms: Option<u64>,
    /// 吞吐量（请求/秒，0 = 无限制）
    pub throughput: Option<u64>,
}

impl Default for TimeConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeConstraint {
    pub fn new() -> Self {
        Self {
            latency_ms: None,
            timeout_ms: None,
            throughput: None,
        }
    }

    /// 合并两个约束：取最严格的值（最小值）
    pub fn meet(a: &TimeConstraint, b: &TimeConstraint) -> TimeConstraint {
        TimeConstraint {
            latency_ms: match (a.latency_ms, b.latency_ms) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (Some(x), None) => Some(x),
                (None, Some(y)) => Some(y),
                (None, None) => None,
            },
            timeout_ms: match (a.timeout_ms, b.timeout_ms) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (Some(x), None) => Some(x),
                (None, Some(y)) => Some(y),
                (None, None) => None,
            },
            throughput: match (a.throughput, b.throughput) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (Some(x), None) => Some(x),
                (None, Some(y)) => Some(y),
                (None, None) => None,
            },
        }
    }

    /// 检查时间约束是否满足要求
    /// actual 实际约束必须 ≥ required 要求（latency 更小=更好, throughput 更大=更好）
    pub fn satisfies(&self, required: &TimeConstraint) -> bool {
        if let Some(req_lat) = required.latency_ms
            && let Some(act_lat) = self.latency_ms
            && act_lat > req_lat
        {
            return false;
        }
        if let Some(req_timeout) = required.timeout_ms
            && let Some(act_timeout) = self.timeout_ms
            && act_timeout > req_timeout
        {
            return false;
        }
        if let Some(req_tput) = required.throughput
            && let Some(act_tput) = self.throughput
            && act_tput < req_tput
        {
            return false;
        }
        true
    }
}

impl fmt::Display for TimeConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(ms) = self.latency_ms {
            parts.push(format!("latency({}ms)", ms));
        }
        if let Some(ms) = self.timeout_ms {
            parts.push(format!("timeout({}ms)", ms));
        }
        if let Some(t) = self.throughput {
            parts.push(format!("throughput({}/s)", t));
        }
        if parts.is_empty() {
            write!(f, "no-time-constraint")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

/// 从字符串解析时间约束：@latency(50ms) → TimeConstraint { latency_ms: Some(50), ... }
pub fn parse_time_constraint(key: &str, value: &str) -> TimeConstraint {
    let mut tc = TimeConstraint::new();
    match key {
        "latency" => {
            // "50ms" → 50
            tc.latency_ms = value.trim_end_matches("ms").trim().parse::<u64>().ok();
        }
        "timeout" => {
            // "5s" → 5000, "500ms" → 500
            if value.ends_with("s") && !value.ends_with("ms") {
                tc.timeout_ms = value
                    .trim_end_matches("s")
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .map(|x| x * 1000);
            } else {
                tc.timeout_ms = value.trim_end_matches("ms").trim().parse::<u64>().ok();
            }
        }
        "throughput" => {
            // "100/s" → 100
            tc.throughput = value.trim_end_matches("/s").trim().parse::<u64>().ok();
        }
        _ => {}
    }
    tc
}

/// 时间约束推断器
#[derive(Debug)]
pub struct TimeConstraintInferencer {
    pub errors: Vec<String>,
}

impl Default for TimeConstraintInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeConstraintInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的延迟约束（仅为示例，真实延迟需要 QN1 profiling）
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> TimeConstraint {
        match expr {
            // 字面量/运算 → 几微秒
            crate::ast::Expr::IntLiteral(_)
            | crate::ast::Expr::FloatLiteral(_)
            | crate::ast::Expr::StringLiteral(_)
            | crate::ast::Expr::BoolLiteral(_)
            | crate::ast::Expr::CharLiteral(_)
            | crate::ast::Expr::Ident(_)
            | crate::ast::Expr::Array(_)
            | crate::ast::Expr::Range { .. }
            | crate::ast::Expr::OptionValue { .. }
            | crate::ast::Expr::ResultValue { .. }
            | crate::ast::Expr::BinaryOp { .. }
            | crate::ast::Expr::UnaryOp { .. }
            | crate::ast::Expr::IfExpr { .. }
            | crate::ast::Expr::MatchExpr { .. } => TimeConstraint {
                latency_ms: Some(0),
                timeout_ms: None,
                throughput: None,
            },
            // 函数调用 → 需要函数级注解
            crate::ast::Expr::Call { .. } => {
                TimeConstraint {
                    latency_ms: Some(10),
                    timeout_ms: None,
                    throughput: None,
                } // 默认 10ms
            }
            _ => TimeConstraint::new(),
        }
    }

    pub fn check(&mut self, actual: &TimeConstraint, required: &TimeConstraint, location: &str) {
        if !actual.satisfies(required) {
            self.errors.push(format!(
                "时间约束违规: {} 需要 {}，但实际仅 {}",
                location, required, actual
            ));
        }
    }
}

pub fn parse_cognitive_loop(s: &str) -> CognitiveLoop {
    match s {
        "perceive" => CognitiveLoop::Perceive,
        "reason" => CognitiveLoop::Reason,
        "decide" => CognitiveLoop::Decide,
        "act" => CognitiveLoop::Act,
        "loop" => CognitiveLoop::Loop,
        _ => CognitiveLoop::Perceive, // 默认感知
    }
}

pub fn parse_governance(s: &str) -> GovernanceLevel {
    match s {
        "prepare" => GovernanceLevel::Prepare,
        "suggest" => GovernanceLevel::Suggest,
        "approve" => GovernanceLevel::Approve,
        "execute" => GovernanceLevel::Execute,
        _ => GovernanceLevel::Prepare, // 默认准备（最小权限原则）
    }
}

/// 置信度推断器
#[derive(Debug)]
pub struct ConfidenceInferencer {
    pub errors: Vec<String>,
}

impl Default for ConfidenceInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfidenceInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的置信度
    /// 字面量 → Proven（1+1=2 是确定的）
    /// 标识符 → 继承自上下文的置信度
    /// 函数调用 → 被调用函数的置信度与参数置信度的最不确定值的 join
    /// LLM 相关函数 → Generated
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> Confidence {
        match expr {
            // 字面量是确定的
            crate::ast::Expr::IntLiteral(_)
            | crate::ast::Expr::FloatLiteral(_)
            | crate::ast::Expr::StringLiteral(_)
            | crate::ast::Expr::BoolLiteral(_)
            | crate::ast::Expr::CharLiteral(_) => Confidence::Proven,
            // Range/Option/Result/Array 由元素决定
            crate::ast::Expr::Range { .. } => Confidence::Proven,
            crate::ast::Expr::OptionValue { .. } => Confidence::Inferred,
            crate::ast::Expr::ResultValue { .. } => Confidence::Inferred,
            crate::ast::Expr::Array(items) => items
                .iter()
                .map(|e| self.infer_expr(e))
                .reduce(|a, b| Confidence::join(&a, &b))
                .unwrap_or(Confidence::Proven),
            // 标识符 → 上下文不确定（由 SevenChannelInferencer 覆盖）
            crate::ast::Expr::Ident(_) => Confidence::Uncertain,

            // 二元/一元运算 → 参数的 join
            crate::ast::Expr::BinaryOp { left, right, .. } => {
                let l = self.infer_expr(left);
                let r = self.infer_expr(right);
                Confidence::join(&l, &r)
            }
            crate::ast::Expr::UnaryOp { operand, .. } => self.infer_expr(operand),

            // 函数调用
            crate::ast::Expr::Call { func, args } => {
                // 参数置信度的最不确定值
                let mut conf = Confidence::Proven;
                for a in args {
                    let a_c = self.infer_expr(a);
                    conf = Confidence::join(&conf, &a_c);
                }
                // 检查是否是 LLM 相关函数
                if let crate::ast::Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        "llm_generate" | "llm_complete" | "llm_embed" => {
                            conf = Confidence::join(&conf, &Confidence::Generated);
                        }
                        "verify" | "validate" | "check" => {
                            conf = Confidence::join(&conf, &Confidence::Verified);
                        }
                        "prove" | "formal_verify" => {
                            conf = Confidence::join(&conf, &Confidence::Proven);
                        }
                        _ => {
                            conf = Confidence::join(&conf, &Confidence::Inferred);
                        }
                    }
                } else {
                    conf = Confidence::join(&conf, &Confidence::Inferred);
                }
                conf
            }

            // 条件表达式 → 所有分支的 join
            crate::ast::Expr::IfExpr(_, t, e) => {
                let t_c = self.infer_expr(t);
                let e_c = self.infer_expr(e);
                Confidence::join(&t_c, &e_c)
            }
            crate::ast::Expr::MatchExpr(_, arms) => {
                let mut conf = Confidence::Proven;
                for arm in arms {
                    for s in &arm.body {
                        if let crate::ast::Stmt::Expr(e) = s {
                            conf = Confidence::join(&conf, &self.infer_expr(e));
                        }
                    }
                }
                conf
            }

            _ => Confidence::Uncertain,
        }
    }

    /// 检查置信度断言：实际置信度必须 >= 期望置信度
    pub fn check(&mut self, actual: &Confidence, required: &Confidence, location: &str) {
        if !required.leq(actual) {
            self.errors.push(format!(
                "置信度不足: {} 需要 {}，但实际只有 {}（score: {} < {}）",
                location,
                required,
                actual,
                actual.score(),
                required.score()
            ));
        }
    }
}

/// 七通道类型（值 × 效应 × 能力 × 置信度）
#[derive(Debug, Clone)]
pub struct SevenChannelType {
    pub value: Option<TypeRef>, // None = 尚未推断
    pub effect: Option<Effect>,
    pub capability: Option<Capability>,
    pub confidence: Option<Confidence>,
    /// Phase C: 认知循环阶段
    pub cognitive_loop: Option<CognitiveLoop>,
    /// Phase C: 治理级别
    pub governance: Option<GovernanceLevel>,
    /// Phase D: 时间约束
    pub time_constraint: Option<TimeConstraint>,
}

impl Default for SevenChannelType {
    fn default() -> Self {
        Self::new()
    }
}

impl SevenChannelType {
    pub fn new() -> Self {
        Self {
            value: None,
            effect: None,
            capability: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            time_constraint: None,
        }
    }

    pub fn value(typ: TypeRef) -> Self {
        Self {
            value: Some(typ),
            effect: None,
            capability: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            time_constraint: None,
        }
    }
}

impl fmt::Display for SevenChannelType {
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
        if let Some(conf) = &self.confidence {
            write!(f, " @ {}", conf)?;
        }
        if let Some(cl) = &self.cognitive_loop {
            write!(f, " @ loop({})", cl)?;
        }
        if let Some(g) = &self.governance {
            write!(f, " @ gov({})", g)?;
        }
        if let Some(tc) = &self.time_constraint {
            write!(f, " @ [{}]", tc)?;
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

impl Default for EffectInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的效应
    /// 字面量、标识符 → pure
    /// 函数调用 → 被调用函数的效应
    /// + - * / → pure
    /// - `spawn → spawn`
    /// - `async fn → async`
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

            crate::ast::Expr::BinaryOp { .. } | crate::ast::Expr::UnaryOp { .. } => Effect::Pure,

            crate::ast::Expr::Call { func, args } => {
                // 函数调用的效应 = 参数的效应的 join + 函数自身的效应
                let mut eff = Effect::Pure;
                for a in args {
                    let a_eff = self.infer_expr(a);
                    eff = Effect::join(&eff, &a_eff).unwrap_or(Effect::Async);
                }
                // 如果是内置函数，大部分是 pure
                if let crate::ast::Expr::Ident(name) = func.as_ref()
                    && (name == "println" || name == "print")
                {
                    eff = Effect::join(&eff, &Effect::Io).unwrap_or(Effect::Io);
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
                        if let crate::ast::Stmt::Expr(e) = s {
                            eff = Effect::join(&eff, &self.infer_expr(e)).unwrap_or(Effect::Async);
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
    /// 函数名 → 能力标注（由 SevenChannelInferencer 填充）
    pub fn_annotations: HashMap<String, Capability>,
}

impl Default for CapabilityInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityInferencer {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            fn_annotations: HashMap::new(),
        }
    }

    /// 推断表达式的执行能力。
    /// - 字面量 / 标识符 / 基本运算 → Cpu
    /// - 函数调用 → 查标注表或内置映射
    /// - spawn / async 上下文 → 默认 Cpu（效应不决定能力）
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> Capability {
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
            | crate::ast::Expr::ResultValue { .. } => Capability::Cpu,

            crate::ast::Expr::BinaryOp { .. } | crate::ast::Expr::UnaryOp { .. } => Capability::Cpu,

            crate::ast::Expr::Call { func, args } => {
                // 参数能力上界
                let mut cap = Capability::Cpu;
                for a in args {
                    let a_cap = self.infer_expr(a);
                    cap = Capability::join(&cap, &a_cap).unwrap_or(Capability::Cpu);
                }
                // 按函数名查能力
                if let crate::ast::Expr::Ident(name) = func.as_ref() {
                    let fn_cap = self
                        .builtin_capability(name)
                        .or_else(|| self.fn_annotations.get(name).cloned())
                        .unwrap_or(Capability::Cpu);
                    cap = Capability::join(&cap, &fn_cap).unwrap_or(Capability::Cpu);
                }
                cap
            }

            crate::ast::Expr::IfExpr(_, t, e) => {
                let t_cap = self.infer_expr(t);
                let e_cap = self.infer_expr(e);
                Capability::join(&t_cap, &e_cap).unwrap_or(Capability::Cpu)
            }

            crate::ast::Expr::MatchExpr(_, arms) => {
                let mut cap = Capability::Cpu;
                for arm in arms {
                    for s in &arm.body {
                        if let Stmt::Expr(e) = s {
                            cap = Capability::join(&cap, &self.infer_expr(e))
                                .unwrap_or(Capability::Cpu);
                        }
                    }
                }
                cap
            }

            _ => Capability::Cpu,
        }
    }

    /// 检查表达式的推断能力是否在声明的能力上下文中有效。
    /// 如果 expr_cap 的能力大于 context 允许的范围，则记录错误。
    pub fn check(&mut self, context: &Capability, expr_cap: &Capability, location: &str) {
        if !expr_cap.leq(context) {
            self.errors.push(format!(
                "能力违规: {} 需要 {:?}，但上下文只允许 {:?}",
                location, expr_cap, context
            ));
        }
    }

    /// 内置函数 → 能力映射表。
    /// sfa_encode/sfa_query → Sfa, gpu_* → Gpu, net_fetch/net_send → Net,
    /// print/println/assert → Cpu, spawn_task → Cpu
    fn builtin_capability(&self, name: &str) -> Option<Capability> {
        match name {
            "sfa_encode" | "sfa_query" | "sfa_attend" => Some(Capability::Sfa),
            n if n.starts_with("gpu_") => Some(Capability::Gpu),
            n if n.starts_with("net_") => Some(Capability::Net),
            _ => None,
        }
    }
}

// ═══════════════════════════════
//  认知循环推断器 (Cognitive Loop Inferencer)
// ═══════════════════════════════

#[derive(Debug)]
pub struct CognitiveLoopInferencer {
    pub errors: Vec<String>,
}

impl Default for CognitiveLoopInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveLoopInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的认知循环阶段
    /// 感知 → 推理 → 决策 → 行动 → 闭环
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> CognitiveLoop {
        match expr {
            // 感知：字面量/标识符/基本运算 — 接收外部输入
            crate::ast::Expr::IntLiteral(_)
            | crate::ast::Expr::FloatLiteral(_)
            | crate::ast::Expr::StringLiteral(_)
            | crate::ast::Expr::BoolLiteral(_)
            | crate::ast::Expr::CharLiteral(_)
            | crate::ast::Expr::Ident(_)
            | crate::ast::Expr::Array(_)
            | crate::ast::Expr::Range { .. }
            | crate::ast::Expr::OptionValue { .. }
            | crate::ast::Expr::ResultValue { .. } => CognitiveLoop::Perceive,

            // 推理：二元/一元运算
            crate::ast::Expr::BinaryOp { .. } | crate::ast::Expr::UnaryOp { .. } => {
                CognitiveLoop::Reason
            }

            // 决策：条件/模式匹配
            crate::ast::Expr::IfExpr { .. } | crate::ast::Expr::MatchExpr { .. } => {
                CognitiveLoop::Decide
            }

            // 行动：函数调用
            crate::ast::Expr::Call { func, args } => {
                // 如果调用了 sfa/llm 相关函数 → 闭环
                if let crate::ast::Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        "sfa_encode" | "sfa_query" | "sfa_attend" | "llm_infer" => {
                            CognitiveLoop::Loop
                        }
                        _ => {
                            // 参数阶段的最高值
                            args.iter()
                                .map(|a| self.infer_expr(a))
                                .reduce(|a, b| CognitiveLoop::join(&a, &b))
                                .unwrap_or(CognitiveLoop::Act)
                        }
                    }
                } else {
                    CognitiveLoop::Act
                }
            }

            _ => CognitiveLoop::Perceive,
        }
    }

    /// 检查认知循环阶段兼容性
    pub fn check(&mut self, context: &CognitiveLoop, expr_loop: &CognitiveLoop, location: &str) {
        if !expr_loop.leq(context) {
            self.errors.push(format!(
                "认知循环违规: {} 需要 {}，但上下文要求 {}",
                location, expr_loop, context
            ));
        }
    }
}

// ═══════════════════════════════
//  治理推断器 (Governance Inferencer)
// ═══════════════════════════════

#[derive(Debug)]
pub struct GovernanceInferencer {
    pub errors: Vec<String>,
}

impl Default for GovernanceInferencer {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernanceInferencer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// 推断表达式的治理级别
    /// 默认返回 Prepare（最小权限原则）
    pub fn infer_expr(&mut self, expr: &crate::ast::Expr) -> GovernanceLevel {
        match expr {
            // 简单操作 → Prepare（仅查询/准备）
            crate::ast::Expr::IntLiteral(_)
            | crate::ast::Expr::FloatLiteral(_)
            | crate::ast::Expr::StringLiteral(_)
            | crate::ast::Expr::BoolLiteral(_)
            | crate::ast::Expr::CharLiteral(_)
            | crate::ast::Expr::Ident(_)
            | crate::ast::Expr::Array(_)
            | crate::ast::Expr::Range { .. }
            | crate::ast::Expr::OptionValue { .. }
            | crate::ast::Expr::ResultValue { .. } => GovernanceLevel::Prepare,

            // 运算 → Suggest
            crate::ast::Expr::BinaryOp { .. }
            | crate::ast::Expr::UnaryOp { .. }
            | crate::ast::Expr::IfExpr { .. }
            | crate::ast::Expr::MatchExpr { .. } => GovernanceLevel::Suggest,

            // 函数调用 → 根据函数名判定
            crate::ast::Expr::Call { func, args } => {
                if let crate::ast::Expr::Ident(name) = func.as_ref() {
                    match name.as_str() {
                        // 写操作 → Approve（需要审批）
                        "write" | "delete" | "update" | "charge" | "pay" | "send_money"
                        | "delete_user" | "modify_permissions" => GovernanceLevel::Approve,
                        // 高风险操作 → Execute（最严格）
                        "execute" | "deploy" | "shutdown" | "format" | "exec" => {
                            GovernanceLevel::Execute
                        }
                        // 默认：参数的高阶
                        _ => args
                            .iter()
                            .map(|a| self.infer_expr(a))
                            .reduce(|a, b| GovernanceLevel::join(&a, &b))
                            .unwrap_or(GovernanceLevel::Prepare),
                    }
                } else {
                    GovernanceLevel::Suggest
                }
            }

            _ => GovernanceLevel::Prepare,
        }
    }

    /// 检查治理级别兼容性
    pub fn check(&mut self, required: &GovernanceLevel, actual: &GovernanceLevel, location: &str) {
        if !actual.leq(required) {
            self.errors.push(format!(
                "治理违规: {} 需要 {} 权限，但当前只有 {}",
                location, actual, required
            ));
        }
    }
}

// ═══════════════════════════════
//  七通道推断器 (Six-Channel Inferencer)
// ═══════════════════════════════

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
    fn default() -> Self {
        Self::new()
    }
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

    pub fn infer_program(&mut self, prog: &crate::ast::Program) {
        let value_types: std::collections::HashMap<String, TypeRef> = self
            .value
            .infer_program(prog)
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // value_types 已转为独立所有权，self 不再被借用

        // 单次遍历 AST：解析注解 → 填充推断器 → 生成结果
        for stmt in &prog.statements {
            if let Stmt::Fn {
                name,
                effect,
                capability,
                confidence,
                cognitive_loop,
                governance,
                latency,
                timeout,
                throughput,
                body,
                async_,
                ..
            } = stmt
            {
                let eff = effect
                    .as_deref()
                    .map(parse_effect)
                    .unwrap_or_else(|| if *async_ { Effect::Async } else { Effect::Pure });
                let cap = capability
                    .as_deref()
                    .map(parse_capability)
                    .unwrap_or(Capability::Cpu);
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
                            to.trim_end_matches("s")
                                .parse::<u64>()
                                .ok()
                                .map(|x| x * 1000)
                        };
                    }
                    if let Some(tp) = throughput {
                        t.throughput = tp.trim_end_matches("/s").parse::<u64>().ok();
                    }
                    t
                };

                // 喂给能力/置信度/认知/治理/时间推断器（用于函数调用时的查表）
                self.capability
                    .fn_annotations
                    .insert(name.clone(), cap.clone());

                // 值层结果
                let value_typ = value_types.get(name).cloned();
                let (val, seven_conf) = if let Some(vt) = value_typ {
                    (Some(vt), conf) // 值层推断的置信度
                } else {
                    (Some(TypeRef::new(BaseType::Func)), conf) // 函数本身
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
                        time_constraint: if tc.latency_ms.is_some()
                            || tc.timeout_ms.is_some()
                            || tc.throughput.is_some()
                        {
                            Some(tc.clone())
                        } else {
                            None
                        },
                    },
                ));

                // P1.5: 遍历函数体，对每个表达式调用各通道的 check() 方法
                self.walk_body_and_check(body, name, &eff, &cap, &cl, &gov, &tc);
            }
        }
    }

    /// 遍历函数体，对每个表达式做通道级检查：
    /// 推断表达式在各通道上的值，然后对比函数声明的约束调用 check()。
    fn walk_body_and_check(
        &mut self,
        body: &[crate::ast::Stmt],
        fn_name: &str,
        declared_effect: &Effect,
        declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        for stmt in body {
            self.walk_stmt_for_check(
                stmt,
                fn_name,
                declared_effect,
                declared_capability,
                declared_cognitive_loop,
                declared_governance,
                declared_time_constraint,
            );
        }
    }

    /// 对一条语句中的表达式递归检查
    fn walk_stmt_for_check(
        &mut self,
        stmt: &crate::ast::Stmt,
        fn_name: &str,
        declared_effect: &Effect,
        declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        match stmt {
            Stmt::Expr(expr) => {
                self.walk_expr_check(
                    expr,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            Stmt::Return(Some(expr)) => {
                self.walk_expr_check(
                    expr,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            Stmt::Let {
                value: Some(expr), ..
            }
            | Stmt::Const {
                value: Some(expr), ..
            } => {
                self.walk_expr_check(
                    expr,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.walk_expr_check(
                    condition,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                for s in then_body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
                for s in else_body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
            }
            Stmt::While { condition, body } => {
                self.walk_expr_check(
                    condition,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                for s in body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.walk_expr_check(
                    iterable,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                for s in body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
            }
            Stmt::Assert { condition, .. } => {
                self.walk_expr_check(
                    condition,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            Stmt::Match { target, arms } => {
                self.walk_expr_check(
                    target,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                for arm in arms {
                    for s in &arm.body {
                        self.walk_stmt_for_check(
                            s,
                            fn_name,
                            declared_effect,
                            declared_capability,
                            declared_cognitive_loop,
                            declared_governance,
                            declared_time_constraint,
                        );
                    }
                }
            }
            Stmt::TryCatch {
                try_body,
                catch_body,
                ..
            } => {
                for s in try_body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
                for s in catch_body {
                    self.walk_stmt_for_check(
                        s,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
            }
            // 其他语句（StructDef, Fn嵌套等）不遍历 body 表达式
            _ => {}
        }
    }

    /// 对单表达式做全通道检查
    fn walk_expr_check(
        &mut self,
        expr: &crate::ast::Expr,
        fn_name: &str,
        declared_effect: &Effect,
        declared_capability: &Capability,
        declared_cognitive_loop: &Option<CognitiveLoop>,
        declared_governance: &Option<GovernanceLevel>,
        declared_time_constraint: &TimeConstraint,
    ) {
        let location = fn_name.to_string();

        // 效应检查
        let expr_eff = self.effect.infer_expr(expr);
        self.effect.check(declared_effect, &expr_eff, &location);

        // 能力检查
        let expr_cap = self.capability.infer_expr(expr);
        self.capability
            .check(declared_capability, &expr_cap, &location);

        // 认知循环检查
        if let Some(declared_cl) = declared_cognitive_loop {
            let expr_cl = self.cognitive_loop.infer_expr(expr);
            self.cognitive_loop.check(declared_cl, &expr_cl, &location);
        }

        // 治理检查
        if let Some(declared_gov) = declared_governance {
            let expr_gov = self.governance.infer_expr(expr);
            self.governance.check(declared_gov, &expr_gov, &location);
        }

        // 时间约束检查
        if declared_time_constraint.latency_ms.is_some()
            || declared_time_constraint.timeout_ms.is_some()
            || declared_time_constraint.throughput.is_some()
        {
            // 表达式级时间约束较难推断，用函数声明值作为 required
            // 实际时间推断在 latency::LatencyVerifier 中完成
            // 这里只做 TimeConstraint 的 satisfy 检查
            let inferred_tc = self.time_constraint.infer_expr(expr);
            if !inferred_tc.satisfies(declared_time_constraint) {
                self.time_constraint
                    .check(&inferred_tc, declared_time_constraint, &location);
            }
        }

        // 递归检查子表达式
        match expr {
            crate::ast::Expr::BinaryOp { left, right, .. } => {
                self.walk_expr_check(
                    left,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                self.walk_expr_check(
                    right,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            crate::ast::Expr::UnaryOp { operand, .. } => {
                self.walk_expr_check(
                    operand,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
            }
            crate::ast::Expr::Call { args, .. } => {
                for arg in args {
                    self.walk_expr_check(
                        arg,
                        fn_name,
                        declared_effect,
                        declared_capability,
                        declared_cognitive_loop,
                        declared_governance,
                        declared_time_constraint,
                    );
                }
            }
            crate::ast::Expr::IfExpr(cond, then_e, else_e) => {
                self.walk_expr_check(
                    cond,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                self.walk_expr_check(
                    then_e,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
                self.walk_expr_check(
                    else_e,
                    fn_name,
                    declared_effect,
                    declared_capability,
                    declared_cognitive_loop,
                    declared_governance,
                    declared_time_constraint,
                );
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
            for err in &self.effect.errors {
                lines.push(format!("  {}", err));
            }
        }
        if !self.capability.errors.is_empty() {
            lines.push("\nCapability Errors:".into());
            for err in &self.capability.errors {
                lines.push(format!("  {}", err));
            }
        }
        if !self.confidence.errors.is_empty() {
            lines.push("\nConfidence Errors:".into());
            for err in &self.confidence.errors {
                lines.push(format!("  {}", err));
            }
        }
        if !self.cognitive_loop.errors.is_empty() {
            lines.push("\nCognitive Loop Errors:".into());
            for err in &self.cognitive_loop.errors {
                lines.push(format!("  {}", err));
            }
        }
        if !self.governance.errors.is_empty() {
            lines.push("\nGovernance Errors:".into());
            for err in &self.governance.errors {
                lines.push(format!("  {}", err));
            }
        }
        if !self.time_constraint.errors.is_empty() {
            lines.push("\nTime Constraint Errors:".into());
            for err in &self.time_constraint.errors {
                lines.push(format!("  {}", err));
            }
        }
        if self.value.errors.is_empty()
            && self.effect.errors.is_empty()
            && self.capability.errors.is_empty()
            && self.confidence.errors.is_empty()
            && self.cognitive_loop.errors.is_empty()
            && self.governance.errors.is_empty()
            && self.time_constraint.errors.is_empty()
        {
            lines.push("\nNo type errors!".into());
        }
        lines.push(String::new());
        lines.join("\n")
    }

    pub fn has_errors(&self) -> bool {
        !self.value.errors.is_empty()
            || !self.effect.errors.is_empty()
            || !self.capability.errors.is_empty()
            || !self.confidence.errors.is_empty()
            || !self.cognitive_loop.errors.is_empty()
            || !self.governance.errors.is_empty()
            || !self.time_constraint.errors.is_empty()
    }

    /// 收集七通道所有结构化错误
    pub fn collect_errors(&self) -> Vec<crate::error::ChannelError> {
        let mut errs = Vec::new();
        for e in &self.effect.errors {
            errs.push(crate::error::ChannelError::EffectViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(),
                required: "".into(),
                detail: e.clone(),
            });
        }
        for e in &self.capability.errors {
            errs.push(crate::error::ChannelError::CapabilityViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(),
                required: "".into(),
                detail: e.clone(),
            });
        }
        for e in &self.confidence.errors {
            errs.push(crate::error::ChannelError::ConfidenceViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                actual: "".into(),
                required: "".into(),
                detail: e.clone(),
            });
        }
        for e in &self.cognitive_loop.errors {
            errs.push(crate::error::ChannelError::CognitiveLoopViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                context: "".into(),
                required: "".into(),
                detail: e.clone(),
            });
        }
        for e in &self.governance.errors {
            errs.push(crate::error::ChannelError::GovernanceViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                required: "".into(),
                actual: "".into(),
                detail: e.clone(),
            });
        }
        for e in &self.time_constraint.errors {
            errs.push(crate::error::ChannelError::LatencyViolation {
                location: crate::error::SourceLocation { line: 0, column: 0, filename: "compile".into() },
                declared_ms: 0,
                actual_ms: 0,
                detail: e.clone(),
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
        assert_eq!(
            Effect::join(&Effect::Io, &Effect::Async),
            Some(Effect::Async)
        );
        assert_eq!(
            Effect::join(&Effect::Spawn, &Effect::Pure),
            Some(Effect::Spawn)
        );
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
        assert_eq!(
            Capability::join(&Capability::Cpu, &Capability::Gpu),
            Some(Capability::Gpu)
        );
        assert_eq!(
            Capability::join(&Capability::Cpu, &Capability::Sfa),
            Some(Capability::Sfa)
        );
        assert_eq!(
            Capability::join(&Capability::Gpu, &Capability::Gpu),
            Some(Capability::Gpu)
        );
        assert_eq!(Capability::join(&Capability::Gpu, &Capability::Sfa), None);
    }

    #[test]
    fn test_capability_inference_wired() {
        // 能力标注必须真正从 AST 接到七通道结果，而不是恒为 Cpu。
        use crate::ast::{Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "remote".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("async".to_string()),
            capability: Some("net".to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_: false,
            pub_: false,
        });
        prog.add(Stmt::Fn {
            name: "local".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: None,
            capability: Some("sfa".to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
        let remote = by_name.get("remote").expect("remote fn present");
        assert_eq!(remote.effect, Some(Effect::Async));
        assert_eq!(remote.capability, Some(Capability::Net));
        let local = by_name.get("local").expect("local fn present");
        // 缺失效应注解 → 安全回落 Pure
        assert_eq!(local.effect, Some(Effect::Pure));
        assert_eq!(local.capability, Some(Capability::Sfa));
    }

    #[test]
    fn test_confidence_lattice() {
        assert!(Confidence::Uncertain.leq(&Confidence::Generated));
        assert!(Confidence::Uncertain.leq(&Confidence::Proven));
        assert!(Confidence::Generated.leq(&Confidence::Inferred));
        assert!(Confidence::Generated.leq(&Confidence::Verified));
        assert!(Confidence::Verified.leq(&Confidence::Proven));
        assert!(!Confidence::Proven.leq(&Confidence::Verified));
        assert!(!Confidence::Inferred.leq(&Confidence::Generated));
    }

    #[test]
    fn test_confidence_join() {
        assert_eq!(
            Confidence::join(&Confidence::Proven, &Confidence::Uncertain),
            Confidence::Uncertain
        );
        assert_eq!(
            Confidence::join(&Confidence::Generated, &Confidence::Inferred),
            Confidence::Generated
        );
        assert_eq!(
            Confidence::join(&Confidence::Verified, &Confidence::Proven),
            Confidence::Verified
        );
        assert_eq!(
            Confidence::join(&Confidence::Proven, &Confidence::Proven),
            Confidence::Proven
        );
    }

    #[test]
    fn test_confidence_inference_literals() {
        let mut inf = ConfidenceInferencer::new();
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::IntLiteral(42)),
            Confidence::Proven
        );
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::StringLiteral("hello".into())),
            Confidence::Proven
        );
    }

    #[test]
    fn test_confidence_inference_llm_call() {
        let mut inf = ConfidenceInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("llm_generate".into())),
            args: vec![crate::ast::Expr::StringLiteral("summarize".into())],
        };
        assert_eq!(inf.infer_expr(&expr), Confidence::Generated);
    }

    #[test]
    fn test_confidence_inference_verify() {
        let mut inf = ConfidenceInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("verify".into())),
            args: vec![crate::ast::Expr::IntLiteral(42)],
        };
        assert_eq!(inf.infer_expr(&expr), Confidence::Verified);
    }

    #[test]
    fn test_confidence_check_rejects_low() {
        let mut inf = ConfidenceInferencer::new();
        inf.check(
            &Confidence::Generated,
            &Confidence::Verified,
            "test_location",
        );
        assert!(!inf.errors.is_empty());
        assert!(inf.errors[0].contains("置信度不足"));
    }

    #[test]
    fn test_confidence_check_accepts_high() {
        let mut inf = ConfidenceInferencer::new();
        inf.check(&Confidence::Proven, &Confidence::Generated, "test_location");
        assert!(inf.errors.is_empty());
    }

    /// 通过 lexer→parser 全链路，验证七通道注解的"自动归类"解析行为：
    /// 单注解 `@X` 按 token 所属集合判定效应/能力，顺序无关。
    fn parse_fn_annotations(src: &str) -> (Option<String>, Option<String>) {
        use crate::ast::Stmt;
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        let mut lex = Lexer::new(src);
        let toks = lex.tokenize().expect("lex ok");
        let prog = Parser::new(toks).parse().expect("parse ok");
        for stmt in &prog.statements {
            if let Stmt::Fn {
                effect, capability, ..
            } = stmt
            {
                return (effect.clone(), capability.clone());
            }
        }
        panic!("no fn found in source: {}", src);
    }

    #[test]
    fn test_single_annotation_tagged_as_capability() {
        // 只写 `@ sfa`：sfa 属能力集合 → 应判为 capability，effect 留空。
        let (effect, cap) = parse_fn_annotations("fn encode(x) @ sfa { return x }");
        assert_eq!(effect, None);
        assert_eq!(cap, Some("sfa".to_string()));
    }

    #[test]
    fn test_async_fn_sugar_with_capability_annotation() {
        // `async fn` 语法糖预设 effect=async，再接 `@ net` 应得到 capability=net。
        let (effect, cap) = parse_fn_annotations("async fn fetch(url) @ net { return \"x\" }");
        assert_eq!(effect, Some("async".to_string()));
        assert_eq!(cap, Some("net".to_string()));
    }

    #[test]
    fn test_reversed_order_annotations() {
        // 顺序无关：`@ sfa @ async` 与 `@ async @ sfa` 等价。
        let (effect, cap) = parse_fn_annotations("fn f(x) @ sfa @ async { return x }");
        assert_eq!(effect, Some("async".to_string()));
        assert_eq!(cap, Some("sfa".to_string()));
    }

    // ═══════════════════════════════
    //  Phase C — 认知循环测试
    // ═══════════════════════════════

    #[test]
    fn test_cognitive_loop_lattice() {
        assert!(CognitiveLoop::Perceive.leq(&CognitiveLoop::Reason));
        assert!(CognitiveLoop::Perceive.leq(&CognitiveLoop::Loop));
        assert!(CognitiveLoop::Reason.leq(&CognitiveLoop::Decide));
        assert!(CognitiveLoop::Decide.leq(&CognitiveLoop::Act));
        assert!(CognitiveLoop::Act.leq(&CognitiveLoop::Loop));
        assert!(CognitiveLoop::Loop.leq(&CognitiveLoop::Loop));
        assert!(!CognitiveLoop::Loop.leq(&CognitiveLoop::Act));
        assert!(!CognitiveLoop::Decide.leq(&CognitiveLoop::Reason));
    }

    #[test]
    fn test_cognitive_loop_join() {
        assert_eq!(
            CognitiveLoop::join(&CognitiveLoop::Perceive, &CognitiveLoop::Loop),
            CognitiveLoop::Loop
        );
        assert_eq!(
            CognitiveLoop::join(&CognitiveLoop::Reason, &CognitiveLoop::Decide),
            CognitiveLoop::Decide
        );
        assert_eq!(
            CognitiveLoop::join(&CognitiveLoop::Perceive, &CognitiveLoop::Perceive),
            CognitiveLoop::Perceive
        );
        assert_eq!(
            CognitiveLoop::join(&CognitiveLoop::Act, &CognitiveLoop::Perceive),
            CognitiveLoop::Act
        );
    }

    #[test]
    fn test_cognitive_loop_infer_perceive() {
        let mut inf = CognitiveLoopInferencer::new();
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::IntLiteral(42)),
            CognitiveLoop::Perceive
        );
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::Ident("x".into())),
            CognitiveLoop::Perceive
        );
    }

    #[test]
    fn test_cognitive_loop_infer_reason() {
        let mut inf = CognitiveLoopInferencer::new();
        let expr = crate::ast::Expr::BinaryOp {
            left: Box::new(crate::ast::Expr::IntLiteral(1)),
            op: "+".to_string(),
            right: Box::new(crate::ast::Expr::IntLiteral(2)),
        };
        assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Reason);
    }

    #[test]
    fn test_cognitive_loop_infer_decide() {
        let mut inf = CognitiveLoopInferencer::new();
        let expr = crate::ast::Expr::IfExpr(
            Box::new(crate::ast::Expr::BoolLiteral(true)),
            Box::new(crate::ast::Expr::IntLiteral(1)),
            Box::new(crate::ast::Expr::IntLiteral(0)),
        );
        assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Decide);
    }

    #[test]
    fn test_cognitive_loop_infer_act() {
        let mut inf = CognitiveLoopInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("do_something".into())),
            args: vec![],
        };
        assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Act);
    }

    #[test]
    fn test_cognitive_loop_infer_loop_sfa() {
        let mut inf = CognitiveLoopInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("sfa_encode".into())),
            args: vec![],
        };
        assert_eq!(inf.infer_expr(&expr), CognitiveLoop::Loop);
    }

    #[test]
    fn test_cognitive_loop_check_rejects() {
        let mut inf = CognitiveLoopInferencer::new();
        inf.check(&CognitiveLoop::Perceive, &CognitiveLoop::Act, "test");
        assert!(!inf.errors.is_empty());
        assert!(inf.errors[0].contains("认知循环违规"));
    }

    #[test]
    fn test_cognitive_loop_check_accepts() {
        let mut inf = CognitiveLoopInferencer::new();
        inf.check(&CognitiveLoop::Loop, &CognitiveLoop::Reason, "test");
        assert!(inf.errors.is_empty());
    }

    // ═══════════════════════════════
    //  Phase C — 治理通道测试
    // ═══════════════════════════════

    #[test]
    fn test_governance_lattice() {
        assert!(GovernanceLevel::Prepare.leq(&GovernanceLevel::Suggest));
        assert!(GovernanceLevel::Prepare.leq(&GovernanceLevel::Execute));
        assert!(GovernanceLevel::Suggest.leq(&GovernanceLevel::Approve));
        assert!(GovernanceLevel::Approve.leq(&GovernanceLevel::Execute));
        assert!(GovernanceLevel::Execute.leq(&GovernanceLevel::Execute));
        assert!(!GovernanceLevel::Execute.leq(&GovernanceLevel::Approve));
        assert!(!GovernanceLevel::Approve.leq(&GovernanceLevel::Suggest));
    }

    #[test]
    fn test_governance_join() {
        assert_eq!(
            GovernanceLevel::join(&GovernanceLevel::Prepare, &GovernanceLevel::Execute),
            GovernanceLevel::Execute
        );
        assert_eq!(
            GovernanceLevel::join(&GovernanceLevel::Suggest, &GovernanceLevel::Approve),
            GovernanceLevel::Approve
        );
        assert_eq!(
            GovernanceLevel::join(&GovernanceLevel::Prepare, &GovernanceLevel::Prepare),
            GovernanceLevel::Prepare
        );
    }

    #[test]
    fn test_governance_infer_prepare() {
        let mut inf = GovernanceInferencer::new();
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::IntLiteral(42)),
            GovernanceLevel::Prepare
        );
        assert_eq!(
            inf.infer_expr(&crate::ast::Expr::Ident("x".into())),
            GovernanceLevel::Prepare
        );
    }

    #[test]
    fn test_governance_infer_suggest() {
        let mut inf = GovernanceInferencer::new();
        let expr = crate::ast::Expr::BinaryOp {
            left: Box::new(crate::ast::Expr::IntLiteral(1)),
            op: "+".to_string(),
            right: Box::new(crate::ast::Expr::IntLiteral(2)),
        };
        assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Suggest);
    }

    #[test]
    fn test_governance_infer_approve() {
        let mut inf = GovernanceInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("charge".into())),
            args: vec![],
        };
        assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Approve);
    }

    #[test]
    fn test_governance_infer_execute() {
        let mut inf = GovernanceInferencer::new();
        let expr = crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident("deploy".into())),
            args: vec![],
        };
        assert_eq!(inf.infer_expr(&expr), GovernanceLevel::Execute);
    }

    #[test]
    fn test_governance_check_rejects() {
        let mut inf = GovernanceInferencer::new();
        inf.check(&GovernanceLevel::Suggest, &GovernanceLevel::Execute, "test");
        assert!(!inf.errors.is_empty());
        assert!(inf.errors[0].contains("治理违规"));
    }

    #[test]
    fn test_governance_check_accepts() {
        let mut inf = GovernanceInferencer::new();
        inf.check(&GovernanceLevel::Execute, &GovernanceLevel::Suggest, "test");
        assert!(inf.errors.is_empty());
    }

    // ═══════════════════════════════
    //  Phase C — Parser 注解解析测试
    // ═══════════════════════════════

    /// Parse function annotation values from a `Stmt::Fn`. Returns a 7-tuple of optional strings.
    #[allow(clippy::type_complexity)] // 7-option tuple for phase C parser tests
    fn parse_fn_all_annotations(
        src: &str,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) {
        use crate::ast::Stmt;
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        let mut lex = Lexer::new(src);
        let toks = lex.tokenize().expect("lex ok");
        let prog = Parser::new(toks).parse().expect("parse ok");
        for stmt in &prog.statements {
            if let Stmt::Fn {
                effect,
                capability,
                cognitive_loop,
                governance,
                latency,
                timeout,
                throughput,
                ..
            } = stmt
            {
                return (
                    effect.clone(),
                    capability.clone(),
                    cognitive_loop.clone(),
                    governance.clone(),
                    latency.clone(),
                    timeout.clone(),
                    throughput.clone(),
                );
            }
        }
        panic!("no fn found in source: {}", src);
    }

    #[test]
    fn test_cognitive_loop_annotation_parsed() {
        let (effect, cap, cl, gov, _, _, _) =
            parse_fn_all_annotations("fn f(x) @ perceive { return x }");
        assert_eq!(cl, Some("perceive".to_string()));
        assert_eq!(effect, None);
        assert_eq!(cap, None);
        assert_eq!(gov, None);
    }

    #[test]
    fn test_governance_annotation_parsed() {
        let (effect, cap, cl, gov, _, _, _) =
            parse_fn_all_annotations("fn f(x) @ gov(approve) { return x }");
        assert_eq!(gov, Some("approve".to_string()));
        assert_eq!(effect, None);
        assert_eq!(cap, None);
        assert_eq!(cl, None);
    }

    #[test]
    fn test_mixed_cognitive_and_governance() {
        let (_, _, cl, gov, _, _, _) =
            parse_fn_all_annotations("fn f(x) @ decide @ gov(execute) @ io { return x }");
        assert_eq!(cl, Some("decide".to_string()));
        assert_eq!(gov, Some("execute".to_string()));
    }

    #[test]
    fn test_cognitive_loop_with_llm_and_effect() {
        let (effect, cap, cl, gov, _, _, _) = parse_fn_all_annotations(
            "fn f(x) @ reason @ pure @ llm(\"analyze data\") { return x }",
        );
        assert_eq!(cl, Some("reason".to_string()));
        assert_eq!(effect, Some("pure".to_string()));
        assert_eq!(cap, None);
        assert_eq!(gov, None);
    }

    #[test]
    fn test_governance_rejects_invalid_level() {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        let mut lex = Lexer::new("fn f(x) @ gov(invalid) { return x }");
        let toks = lex.tokenize().expect("lex ok");
        let result = Parser::new(toks).parse();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown governance level"));
    }

    // ═══════════════════════════════
    //  Phase C — 七通道端到端测试
    // ═══════════════════════════════

    #[test]
    fn test_six_channel_inference_from_ast() {
        use crate::ast::{Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "sensor_read".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("io".to_string()),
            capability: Some("cpu".to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: Some("perceive".to_string()),
            governance: Some("prepare".to_string()),
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
        let sensor = by_name.get("sensor_read").expect("sensor_read present");
        assert_eq!(sensor.effect, Some(Effect::Io));
        assert_eq!(sensor.capability, Some(Capability::Cpu));
        assert_eq!(sensor.cognitive_loop, Some(CognitiveLoop::Perceive));
        assert_eq!(sensor.governance, Some(GovernanceLevel::Prepare));
    }

    #[test]
    fn test_six_channel_with_governance_execute() {
        use crate::ast::{Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "deploy_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("spawn".to_string()),
            capability: Some("net".to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: Some("act".to_string()),
            governance: Some("execute".to_string()),
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        let by_name: std::collections::HashMap<_, _> = inf.results.iter().cloned().collect();
        let deploy = by_name.get("deploy_fn").expect("deploy_fn present");
        assert_eq!(deploy.effect, Some(Effect::Spawn));
        assert_eq!(deploy.capability, Some(Capability::Net));
        assert_eq!(deploy.cognitive_loop, Some(CognitiveLoop::Act));
        assert_eq!(deploy.governance, Some(GovernanceLevel::Execute));
    }

    // ═══════════════════════════════
    //  Phase D — 时间通道测试
    // ═══════════════════════════════

    #[test]
    fn test_time_constraint_meet_both_some() {
        let a = TimeConstraint {
            latency_ms: Some(50),
            timeout_ms: Some(5000),
            throughput: Some(100),
        };
        let b = TimeConstraint {
            latency_ms: Some(100),
            timeout_ms: Some(3000),
            throughput: Some(200),
        };
        let m = TimeConstraint::meet(&a, &b);
        assert_eq!(m.latency_ms, Some(50)); // 取更严格（更小）
        assert_eq!(m.timeout_ms, Some(3000)); // 取更严格（更小）
        assert_eq!(m.throughput, Some(100)); // 取更严格（更小）
    }

    #[test]
    fn test_time_constraint_meet_none() {
        let a = TimeConstraint {
            latency_ms: Some(50),
            timeout_ms: None,
            throughput: None,
        };
        let b = TimeConstraint {
            latency_ms: None,
            timeout_ms: Some(5000),
            throughput: None,
        };
        let m = TimeConstraint::meet(&a, &b);
        assert_eq!(m.latency_ms, Some(50));
        assert_eq!(m.timeout_ms, Some(5000));
        assert_eq!(m.throughput, None);
    }

    #[test]
    fn test_time_constraint_satisfies_latency() {
        let actual = TimeConstraint {
            latency_ms: Some(30),
            timeout_ms: None,
            throughput: None,
        };
        let required = TimeConstraint {
            latency_ms: Some(50),
            timeout_ms: None,
            throughput: None,
        };
        assert!(actual.satisfies(&required)); // 30ms < 50ms ✓
        let actual2 = TimeConstraint {
            latency_ms: Some(60),
            timeout_ms: None,
            throughput: None,
        };
        assert!(!actual2.satisfies(&required)); // 60ms > 50ms ✗
    }

    #[test]
    fn test_time_constraint_satisfies_throughput() {
        let actual = TimeConstraint {
            latency_ms: None,
            timeout_ms: None,
            throughput: Some(200),
        };
        let required = TimeConstraint {
            latency_ms: None,
            timeout_ms: None,
            throughput: Some(100),
        };
        assert!(actual.satisfies(&required)); // 200/s > 100/s ✓
        let actual2 = TimeConstraint {
            latency_ms: None,
            timeout_ms: None,
            throughput: Some(50),
        };
        assert!(!actual2.satisfies(&required)); // 50/s < 100/s ✗
    }

    #[test]
    fn test_parse_time_latency() {
        let tc = parse_time_constraint("latency", "50ms");
        assert_eq!(tc.latency_ms, Some(50));
    }

    #[test]
    fn test_parse_time_timeout_seconds() {
        let tc = parse_time_constraint("timeout", "5s");
        assert_eq!(tc.timeout_ms, Some(5000));
    }

    #[test]
    fn test_parse_time_throughput() {
        let tc = parse_time_constraint("throughput", "100/s");
        assert_eq!(tc.throughput, Some(100));
    }

    #[test]
    fn test_time_constraint_inferencer_check() {
        let mut inf = TimeConstraintInferencer::new();
        let actual = TimeConstraint {
            latency_ms: Some(60),
            timeout_ms: None,
            throughput: None,
        };
        let required = TimeConstraint {
            latency_ms: Some(50),
            timeout_ms: None,
            throughput: None,
        };
        inf.check(&actual, &required, "test_fn");
        assert!(!inf.errors.is_empty());
        assert!(inf.errors[0].contains("时间约束违规"));
    }

    #[test]
    fn test_parser_latency_annotation() {
        let (_, _, _, _, latency, timeout, throughput) =
            parse_fn_all_annotations("fn f(x) @ latency(50ms) { return x }");
        assert_eq!(latency, Some("50ms".to_string()));
        assert_eq!(timeout, None);
        assert_eq!(throughput, None);
    }

    #[test]
    fn test_parser_all_time_annotations() {
        let (_, _, _, _, latency, timeout, throughput) = parse_fn_all_annotations(
            "fn f(x) @ latency(30ms) @ timeout(5s) @ throughput(100/s) { return x }",
        );
        assert_eq!(latency, Some("30ms".to_string()));
        assert_eq!(timeout, Some("5s".to_string()));
        assert_eq!(throughput, Some("100/s".to_string()));
    }

    #[test]
    fn test_parser_time_with_effect_and_capability() {
        let (effect, cap, _, _, latency, _, _) =
            parse_fn_all_annotations("fn f(x) @ io @ sfa @ latency(50ms) { return x }");
        assert_eq!(effect, Some("io".to_string()));
        assert_eq!(cap, Some("sfa".to_string()));
        assert_eq!(latency, Some("50ms".to_string()));
    }

    // ═══════════════════════════════
    //  P2.2 — Body-level 违规检测测试
    // ═══════════════════════════════

    #[allow(dead_code)] // helper for body-level violation tests
    fn make_body_with_call(called_fn: &str) -> Vec<crate::ast::Stmt> {
        vec![crate::ast::Stmt::Expr(Box::new(crate::ast::Expr::Call {
            func: Box::new(crate::ast::Expr::Ident(called_fn.to_string())),
            args: vec![],
        }))]
    }

    #[test]
    fn test_body_walk_detects_effect_violation() {
        // pure 声明的函数体内调用了 println (io 效应) → 应报效应违规
        use crate::ast::{Expr, Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "bad".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("pure".to_string()),
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("println".to_string())),
                args: vec![Expr::StringLiteral("hello".to_string())],
            }))],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        // println 是 Io 效应，但函数声明 pure → 应有违规
        assert!(
            !inf.effect.errors.is_empty(),
            "Expected effect violation: pure fn body calls println (IO)"
        );
    }

    #[test]
    fn test_body_walk_detects_cognitive_loop_violation() {
        // perceive 声明的函数体内调用了 do_something (act) → 应报认知循环违规
        use crate::ast::{Expr, Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "bad_cl".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: None,
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: Some("perceive".to_string()),
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("do_something".to_string())),
                args: vec![],
            }))],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        // do_something 是 Act，但函数声明 perceive → 应有违规
        assert!(
            !inf.cognitive_loop.errors.is_empty(),
            "Expected cognitive loop violation: perceive fn body calls do_something (act)"
        );
    }

    #[test]
    fn test_body_walk_detects_governance_violation() {
        // prepare 声明的函数体内调用了 deploy (execute) → 应报治理违规
        use crate::ast::{Expr, Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "bad_gov".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: None,
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: Some("prepare".to_string()),
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![Stmt::Expr(Box::new(Expr::Call {
                func: Box::new(Expr::Ident("deploy".to_string())),
                args: vec![],
            }))],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        // deploy 是 Execute，但函数声明 prepare → 应有违规
        assert!(
            !inf.governance.errors.is_empty(),
            "Expected governance violation: prepare fn body calls deploy (execute)"
        );
    }

    #[test]
    fn test_confidence_annotation_parsed() {
        // 验证 @ proven 注解被正确解析为 confidence
        let (effect, cap, _, _, _, _, _) =
            parse_fn_all_annotations("fn f(x) @ proven { return x }");
        // proven 是置信度 token，不应落到 effect 或 capability
        assert_eq!(effect, None);
        assert_eq!(cap, None);
    }

    #[test]
    fn test_confidence_annotation_with_pure() {
        // 混合注解: @ proven @ pure
        let (effect, cap, _, _, _, _, _) =
            parse_fn_all_annotations("fn f(x) @ proven @ pure { return x }");
        assert_eq!(effect, Some("pure".to_string()));
        assert_eq!(cap, None);
    }

    #[test]
    fn test_infer_program_confidence_from_field() {
        // 验证 infer_program 从 confidence 字段读取，不再是 capability 借道
        use crate::ast::{Program, Stmt};
        let mut prog = Program::new();
        prog.add(Stmt::Fn {
            name: "proven_fn".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: None,
            capability: Some("cpu".to_string()),
            llm_prompt: None,
            confidence: Some("verified".to_string()),
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_: false,
            pub_: false,
        });
        let mut inf = SevenChannelInferencer::new();
        inf.infer_program(&prog);
        let r = &inf.results[0].1;
        assert_eq!(
            r.confidence,
            Some(Confidence::Verified),
            "confidence should come from the confidence field, not capability"
        );
        assert_eq!(
            r.capability,
            Some(Capability::Cpu),
            "capability should remain Cpu from the capability field"
        );
    }
}
