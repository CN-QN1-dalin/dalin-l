/// Dalin L 3.0 — 七通道偏序格类型定义
///
/// Effect (效应), Capability (能力), Confidence (置信度),
/// CognitiveLoop (认知循环), GovernanceLevel (治理级别)
///
/// 每个类型都有 leq() (偏序) 和 join() (最小上界) 操作，
/// 构成完整的格结构 (Lattice)，支持类型系统的 unification。
use std::fmt;

// ═══════════════════════════════
//  效应类型 (Effect Channel)
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect { Pure, Io, Async, Spawn }

impl Effect {
    pub fn leq(&self, other: &Effect) -> bool {
        use Effect::*;
        match (self, other) {
            (Pure, _) => true, (_, Pure) => false,
            (Io, Io) | (Io, Async) => true, (Async, Async) => true,
            (Spawn, Spawn) => true, _ => false,
        }
    }
    pub fn join(a: &Effect, b: &Effect) -> Option<Effect> {
        use Effect::*;
        match (a, b) {
            (Pure, x) | (x, Pure) => Some(x.clone()),
            (Io, Io) => Some(Io), (Io, Async) | (Async, Io) => Some(Async),
            (Async, Async) => Some(Async), (Spawn, Spawn) => Some(Spawn),
            (Io, Spawn) | (Spawn, Io) => None,
            (Async, Spawn) | (Spawn, Async) => None,
        }
    }
}
impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Pure => write!(f, "pure"), Self::Io => write!(f, "io"), Self::Async => write!(f, "async"), Self::Spawn => write!(f, "spawn") }
    }
}

// ═══════════════════════════════
//  能力类型 (Capability Channel)
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability { Cpu, Gpu, Sfa, Net }

impl Capability {
    pub fn leq(&self, other: &Capability) -> bool {
        use Capability::*;
        match (self, other) {
            (Cpu, _) => true, (_, Cpu) => false,
            (Gpu, Gpu) => true, (Sfa, Sfa) => true, (Net, Net) => true,
            _ => false,
        }
    }
    pub fn join(a: &Capability, b: &Capability) -> Option<Capability> {
        use Capability::*;
        match (a, b) {
            (Cpu, x) | (x, Cpu) => Some(x.clone()),
            (Gpu, Gpu) => Some(Gpu), (Sfa, Sfa) => Some(Sfa),
            (Net, Net) => Some(Net), _ => None,
        }
    }
}
impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Cpu => write!(f, "cpu"), Self::Gpu => write!(f, "gpu"), Self::Sfa => write!(f, "sfa"), Self::Net => write!(f, "net") }
    }
}

// ═══════════════════════════════
//  置信度通道 (Confidence Channel)
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Confidence { Proven, Verified, Inferred, Generated, Uncertain }

impl Confidence {
    pub fn leq(&self, other: &Confidence) -> bool {
        use Confidence::*;
        match (self, other) {
            (Uncertain, _) => true, (_, Uncertain) => false,
            (Generated, Inferred|Verified|Proven) => true,
            (Generated, _) => false,
            (Inferred, Inferred|Verified|Proven) => true,
            (Inferred, _) => false,
            (Verified, Verified|Proven) => true,
            (Verified, _) => false,
            (Proven, Proven) => true, (Proven, _) => false,
        }
    }
    pub fn join(a: &Confidence, b: &Confidence) -> Confidence {
        let order = |c: &Confidence| -> u8 {
            match c { Confidence::Proven=>4, Confidence::Verified=>3, Confidence::Inferred=>2, Confidence::Generated=>1, Confidence::Uncertain=>0 }
        };
        if order(a) <= order(b) { a.clone() } else { b.clone() }
    }
    pub fn score(&self) -> f64 {
        match self { Confidence::Proven=>1.0, Confidence::Verified=>0.95, Confidence::Inferred=>0.85, Confidence::Generated=>0.7, Confidence::Uncertain=>0.5 }
    }
}
impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Proven => write!(f, "proven"), Self::Verified => write!(f, "verified"), Self::Inferred => write!(f, "inferred"), Self::Generated => write!(f, "generated"), Self::Uncertain => write!(f, "uncertain") }
    }
}

// ═══════════════════════════════
//  认知循环类型 (Cognitive Loop Channel)
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CognitiveLoop { Perceive, Reason, Decide, Act, Loop }

impl CognitiveLoop {
    pub fn leq(&self, other: &CognitiveLoop) -> bool {
        use CognitiveLoop::*;
        match (self, other) {
            (Perceive, _) => true,
            (Reason, Reason|Decide|Act|Loop) => true, (_, Reason) => false,
            (Decide, Decide|Act|Loop) => true, (_, Decide) => false,
            (Act, Act|Loop) => true, (_, Act) => false,
            (Loop, Loop) => true, _ => false,
        }
    }
    pub fn join(a: &CognitiveLoop, b: &CognitiveLoop) -> CognitiveLoop {
        let order = |c: &CognitiveLoop| -> u8 {
            match c { CognitiveLoop::Perceive=>0, CognitiveLoop::Reason=>1, CognitiveLoop::Decide=>2, CognitiveLoop::Act=>3, CognitiveLoop::Loop=>4 }
        };
        if order(a) >= order(b) { a.clone() } else { b.clone() }
    }
}
impl fmt::Display for CognitiveLoop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Perceive => write!(f, "perceive"), Self::Reason => write!(f, "reason"), Self::Decide => write!(f, "decide"), Self::Act => write!(f, "act"), Self::Loop => write!(f, "loop") }
    }
}

// ═══════════════════════════════
//  治理通道 (Governance Channel)
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GovernanceLevel { Prepare, Suggest, Approve, Execute }

impl GovernanceLevel {
    pub fn leq(&self, other: &GovernanceLevel) -> bool {
        use GovernanceLevel::*;
        match (self, other) {
            (Prepare, _) => true, (_, Prepare) => false,
            (Suggest, Suggest|Approve|Execute) => true, (_, Suggest) => false,
            (Approve, Approve|Execute) => true, (_, Approve) => false,
            (Execute, _) => matches!(other, Execute),
        }
    }
    pub fn join(a: &GovernanceLevel, b: &GovernanceLevel) -> GovernanceLevel {
        let order = |g: &GovernanceLevel| -> u8 {
            match g { GovernanceLevel::Prepare=>0, GovernanceLevel::Suggest=>1, GovernanceLevel::Approve=>2, GovernanceLevel::Execute=>3 }
        };
        if order(a) >= order(b) { a.clone() } else { b.clone() }
    }
}
impl fmt::Display for GovernanceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Prepare => write!(f, "prepare"), Self::Suggest => write!(f, "suggest"), Self::Approve => write!(f, "approve"), Self::Execute => write!(f, "execute") }
    }
}
