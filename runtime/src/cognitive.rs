/// Dalin L 3.0 — Cognitive Runtime (Phase 3)
///
/// Bridges compile-time cognitive annotations (@cognitive_loop, @governance, @confidence, @latency)
/// into runtime execution behavior.
///
/// Architecture:
///   - CognitiveLoopMachine: Perceive→Reason→Decide→Act→Loop phase state machine
///   - GovernanceChecker: @gov(prepare/suggest/approve/execute) access control
///   - TimeMonitor: @latency/@timeout/@throughput runtime enforcement
///   - ConfidenceGate: @proven/@verified/@inferred/@generated execution path selection
use dalin_compiler::ty2::{CognitiveLoop, GovernanceLevel};
use std::fmt;
use std::time::Instant;

// ═══════════════════════════════════════════
//  CognitiveLoopPhase
// ═══════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum CognitiveLoopPhase {
    Idle,
    Perceiving,
    Reasoning,
    Deciding,
    Acting,
    Looping,
}

impl fmt::Display for CognitiveLoopPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Perceiving => write!(f, "perceive"),
            Self::Reasoning => write!(f, "reason"),
            Self::Deciding => write!(f, "decide"),
            Self::Acting => write!(f, "act"),
            Self::Looping => write!(f, "loop"),
        }
    }
}

fn cognitive_loop_to_phase(cl: &CognitiveLoop) -> CognitiveLoopPhase {
    match cl {
        CognitiveLoop::Perceive => CognitiveLoopPhase::Perceiving,
        CognitiveLoop::Reason => CognitiveLoopPhase::Reasoning,
        CognitiveLoop::Decide => CognitiveLoopPhase::Deciding,
        CognitiveLoop::Act => CognitiveLoopPhase::Acting,
        CognitiveLoop::Loop => CognitiveLoopPhase::Looping,
    }
}

// ═══════════════════════════════════════════
//  CognitiveLoopMachine
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct CognitiveLoopMachine {
    pub current_phase: CognitiveLoopPhase,
    pub phase_history: Vec<(CognitiveLoopPhase, String, u64)>,
    /// Phase timestamps for latency tracking
    #[allow(dead_code)]
    phase_start: Option<Instant>,
}

impl Default for CognitiveLoopMachine {
    fn default() -> Self { Self::new() }
}

impl CognitiveLoopMachine {
    pub fn new() -> Self {
        Self {
            current_phase: CognitiveLoopPhase::Idle,
            phase_history: Vec::new(),
            phase_start: None,
        }
    }

    /// Transition to next cognitive phase
    pub fn advance(&mut self, phase: CognitiveLoopPhase, fn_name: &str, elapsed_us: u64) {
        self.current_phase = phase;
        self.phase_history.push((self.current_phase.clone(), fn_name.to_string(), elapsed_us));
    }

    /// Validate that a function's declared cognitive loop is allowed in current phase.
    pub fn check_phase(&self, declared: &CognitiveLoop, fn_name: &str) -> Result<(), String> {
        let required_phase = cognitive_loop_to_phase(declared);
        if self.current_phase == CognitiveLoopPhase::Idle {
            return Ok(());
        }
        let order = [
            CognitiveLoopPhase::Perceiving,
            CognitiveLoopPhase::Reasoning,
            CognitiveLoopPhase::Deciding,
            CognitiveLoopPhase::Acting,
            CognitiveLoopPhase::Looping,
        ];
        let current_idx = order.iter().position(|p| *p == self.current_phase);
        let required_idx = order.iter().position(|p| *p == required_phase);
        if let (Some(ci), Some(ri)) = (current_idx, required_idx)
            && ri > ci
        {
            return Err(format!(
                "cognitive loop violation: fn '{}' requires @{:?} but current phase is @{:?}",
                fn_name, declared, self.current_phase
            ));
        }
        Ok(())
    }

    pub fn report(&self) -> String {
        let mut out = format!("cognitive loop: {} phase(s)\n", self.phase_history.len());
        for (i, (phase, name, us)) in self.phase_history.iter().enumerate() {
            out.push_str(&format!("  {}. {} — {} ({}.{:03}ms)\n", i + 1, phase, name, us / 1000, us % 1000));
        }
        out
    }
}

// ═══════════════════════════════════════════
//  GovernanceChecker
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct GovernanceChecker {
    pub session_level: GovernanceLevel,
    pub check_log: Vec<(String, GovernanceLevel, bool)>,
}

impl GovernanceChecker {
    pub fn new(session_level: GovernanceLevel) -> Self {
        Self {
            session_level,
            check_log: Vec::new(),
        }
    }

    /// Check if caller has authority to execute at `target` governance level
    pub fn check(&mut self, target: &GovernanceLevel, fn_name: &str) -> Result<(), String> {
        let permitted = match (&self.session_level, target) {
            (GovernanceLevel::Execute, _) => true,
            (GovernanceLevel::Approve, GovernanceLevel::Execute) => false,
            (GovernanceLevel::Approve, _) => true,
            (GovernanceLevel::Suggest, GovernanceLevel::Approve | GovernanceLevel::Execute) => false,
            (GovernanceLevel::Suggest, _) => true,
            (GovernanceLevel::Prepare, GovernanceLevel::Prepare) => true,
            (GovernanceLevel::Prepare, _) => false,
        };
        self.check_log.push((fn_name.to_string(), target.clone(), permitted));
        if !permitted {
            return Err(format!(
                "governance violation: fn '{}' requires {:?} but session is {:?}",
                fn_name, target, self.session_level
            ));
        }
        Ok(())
    }

    pub fn report(&self) -> String {
        let mut out = format!("governance session: {:?}\n", self.session_level);
        for (name, level, ok) in &self.check_log {
            out.push_str(&format!("  {} {}: {:?}\n", if *ok { "✅" } else { "❌" }, name, level));
        }
        out
    }
}

// ═══════════════════════════════════════════
//  TimeMonitor
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct TimeMonitor {
    pub start: Instant,
    pub fn_timings: Vec<(String, u64)>,
}

impl Default for TimeMonitor {
    fn default() -> Self { Self::new() }
}

impl TimeMonitor {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            fn_timings: Vec::new(),
        }
    }

    pub fn record(&mut self, fn_name: &str, elapsed_ms: u64) {
        self.fn_timings.push((fn_name.to_string(), elapsed_ms));
    }

    pub fn report(&self) -> String {
        let mut out = String::from("time monitor:\n");
        for (name, ms) in &self.fn_timings {
            out.push_str(&format!("  {}: {}ms\n", name, ms));
        }
        out
    }
}

// ═══════════════════════════════════════════
//  ConfidenceGate — execution path selection
// ═══════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum ConfidenceLevel {
    Proven,      // Mathematically proven — unconditional fast path
    Verified,    // Verified by type system + tests — checked fast path
    Inferred,    // Inferred by static analysis — normal path
    Generated,   // LLM-generated — guarded path with runtime verification
    Uncertain,   // Low confidence — requires human approval or fallback
}

impl fmt::Display for ConfidenceLevel {
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

impl ConfidenceLevel {
    /// Convert from compiler's confidence annotation string
    pub fn from_annotation(s: Option<&str>) -> Self {
        match s {
            Some("proven") => Self::Proven,
            Some("verified") => Self::Verified,
            Some("inferred") => Self::Inferred,
            Some("generated") => Self::Generated,
            Some("uncertain") => Self::Uncertain,
            _ => Self::Inferred, // default
        }
    }

    /// Whether this confidence level allows execution without guard
    pub fn allows_fast_path(&self) -> bool {
        matches!(self, Self::Proven | Self::Verified)
    }

    /// Whether this confidence level requires runtime guard
    pub fn requires_guard(&self) -> bool {
        matches!(self, Self::Generated | Self::Uncertain)
    }
}

/// ConfidenceGate — evaluates whether a function can execute based on confidence
pub struct ConfidenceGate {
    pub threshold: ConfidenceLevel,
    pub gate_log: Vec<(String, ConfidenceLevel, bool)>,
}

impl fmt::Display for ConfidenceGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "confidence_gate (threshold={})", self.threshold)?;
        for (name, level, allowed) in &self.gate_log {
            write!(f, "  {} {:?}: {}", if *allowed { "✅" } else { "❌" }, name, level)?;
        }
        Ok(())
    }
}

impl ConfidenceGate {
    pub fn new(threshold: ConfidenceLevel) -> Self {
        Self {
            threshold,
            gate_log: Vec::new(),
        }
    }

    /// Check if a function's confidence level meets the threshold
    pub fn check(&mut self, fn_name: &str, level: &ConfidenceLevel) -> Result<(), String> {
        let allowed = self.compare(level, &self.threshold);
        self.gate_log.push((fn_name.to_string(), level.clone(), allowed));
        if !allowed {
            return Err(format!(
                "confidence gate: fn '{}' is {:?} but threshold is {:?}",
                fn_name, level, self.threshold
            ));
        }
        Ok(())
    }

    fn compare(&self, level: &ConfidenceLevel, threshold: &ConfidenceLevel) -> bool {
        let order: &[ConfidenceLevel] = &[
            ConfidenceLevel::Uncertain,
            ConfidenceLevel::Generated,
            ConfidenceLevel::Inferred,
            ConfidenceLevel::Verified,
            ConfidenceLevel::Proven,
        ];
        let l_idx = order.iter().position(|c| c == level);
        let t_idx = order.iter().position(|c| c == threshold);
        match (l_idx, t_idx) {
            (Some(l), Some(t)) => l >= t,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognitive_loop_advance() {
        let mut m = CognitiveLoopMachine::new();
        assert_eq!(m.current_phase, CognitiveLoopPhase::Idle);
        m.advance(CognitiveLoopPhase::Perceiving, "read_sensor", 100);
        assert_eq!(m.current_phase, CognitiveLoopPhase::Perceiving);
        assert_eq!(m.phase_history.len(), 1);
    }

    #[test]
    fn test_cognitive_loop_violation() {
        let mut m = CognitiveLoopMachine::new();
        m.advance(CognitiveLoopPhase::Perceiving, "read", 10);
        let result = m.check_phase(&CognitiveLoop::Decide, "decide_fn");
        assert!(result.is_err(), "should block decide before reason phase");
    }

    #[test]
    fn test_cognitive_loop_idle_allows_any() {
        let m = CognitiveLoopMachine::new();
        assert!(m.check_phase(&CognitiveLoop::Act, "act_fn").is_ok());
    }

    #[test]
    fn test_governance_execute_allows_all() {
        let mut c = GovernanceChecker::new(GovernanceLevel::Execute);
        assert!(c.check(&GovernanceLevel::Execute, "x").is_ok());
        assert!(c.check(&GovernanceLevel::Approve, "x").is_ok());
        assert!(c.check(&GovernanceLevel::Suggest, "x").is_ok());
        assert!(c.check(&GovernanceLevel::Prepare, "x").is_ok());
    }

    #[test]
    fn test_governance_prepare_limited() {
        let mut c = GovernanceChecker::new(GovernanceLevel::Prepare);
        assert!(c.check(&GovernanceLevel::Prepare, "x").is_ok());
        assert!(c.check(&GovernanceLevel::Suggest, "x").is_err());
    }

    #[test]
    fn test_confidence_levels() {
        assert!(ConfidenceLevel::Proven.allows_fast_path());
        assert!(ConfidenceLevel::Verified.allows_fast_path());
        assert!(!ConfidenceLevel::Inferred.allows_fast_path());
        assert!(ConfidenceLevel::Generated.requires_guard());
        assert!(ConfidenceLevel::Uncertain.requires_guard());
    }

    #[test]
    fn test_confidence_gate() {
        let mut gate = ConfidenceGate::new(ConfidenceLevel::Verified);
        assert!(gate.check("safe", &ConfidenceLevel::Proven).is_ok());
        assert!(gate.check("safe", &ConfidenceLevel::Verified).is_ok());
        assert!(gate.check("risky", &ConfidenceLevel::Generated).is_err());
    }

    #[test]
    fn test_confidence_from_annotation() {
        assert_eq!(ConfidenceLevel::from_annotation(Some("proven")), ConfidenceLevel::Proven);
        assert_eq!(ConfidenceLevel::from_annotation(Some("generated")), ConfidenceLevel::Generated);
        assert_eq!(ConfidenceLevel::from_annotation(None), ConfidenceLevel::Inferred);
    }
}
