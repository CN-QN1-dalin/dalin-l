/// Dalin L 3.0 — Cognitive Loop Machine, Governance Checker, Time Monitor
///
/// Three independent monitoring subsystems for the runtime execution engine.
use std::fmt;
use std::time::Instant;

use crate::ty2::{CognitiveLoop, GovernanceLevel, TimeConstraint};

use super::value::{RuntimeResult, RuntimeError};

// ═══════════════════════════════════════════
//  CognitiveLoopPhase — 认知循环相位
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
            CognitiveLoopPhase::Idle => write!(f, "idle"),
            CognitiveLoopPhase::Perceiving => write!(f, "perceive"),
            CognitiveLoopPhase::Reasoning => write!(f, "reason"),
            CognitiveLoopPhase::Deciding => write!(f, "decide"),
            CognitiveLoopPhase::Acting => write!(f, "act"),
            CognitiveLoopPhase::Looping => write!(f, "loop"),
        }
    }
}

/// 从 CognitiveLoop 枚举映射到运行时相位
pub(crate) fn cognitive_loop_to_phase(cl: &CognitiveLoop) -> CognitiveLoopPhase {
    match cl {
        CognitiveLoop::Perceive => CognitiveLoopPhase::Perceiving,
        CognitiveLoop::Reason => CognitiveLoopPhase::Reasoning,
        CognitiveLoop::Decide => CognitiveLoopPhase::Deciding,
        CognitiveLoop::Act => CognitiveLoopPhase::Acting,
        CognitiveLoop::Loop => CognitiveLoopPhase::Looping,
    }
}

// ═══════════════════════════════════════════
//  CognitiveLoopMachine — 认知循环执行器
// ═══════════════════════════════════════════

/// 认知循环机：管理 Perceive→Reason→Decide→Act→Loop 的相位切换
#[derive(Debug, Clone)]
pub struct CognitiveLoopMachine {
    pub current_phase: CognitiveLoopPhase,
    pub phase_history: Vec<(CognitiveLoopPhase, String, u64)>, // (phase, fn_name, elapsed_us)
}

impl Default for CognitiveLoopMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveLoopMachine {
    pub fn new() -> Self {
        Self {
            current_phase: CognitiveLoopPhase::Idle,
            phase_history: Vec::new(),
        }
    }

    /// 进入下一认知相位
    pub fn advance(&mut self, phase: CognitiveLoopPhase, fn_name: &str, elapsed_us: u64) {
        self.current_phase = phase.clone();
        self.phase_history
            .push((phase, fn_name.to_string(), elapsed_us));
    }

    /// 检查调用方的认知阶段是否满足声明的阶段要求
    pub fn check_phase(&self, declared: &CognitiveLoop, fn_name: &str) -> RuntimeResult<()> {
        let required_phase = cognitive_loop_to_phase(declared);
        // 如果当前为 Idle，任何认知循环都是合法的
        if self.current_phase == CognitiveLoopPhase::Idle {
            return Ok(());
        }
        // 检查相位进度：当前必须 >= 声明
        let phase_order: Vec<CognitiveLoopPhase> = vec![
            CognitiveLoopPhase::Perceiving,
            CognitiveLoopPhase::Reasoning,
            CognitiveLoopPhase::Deciding,
            CognitiveLoopPhase::Acting,
            CognitiveLoopPhase::Looping,
        ];
        let current_idx = phase_order.iter().position(|p| *p == self.current_phase);
        let required_idx = phase_order.iter().position(|p| *p == required_phase);

        if let (Some(ci), Some(ri)) = (current_idx, required_idx)
            && ri > ci
        {
            return Err(RuntimeError::CognitiveLoopViolation {
                declared: declared.clone(),
                required: CognitiveLoop::Perceive,
                fn_name: fn_name.to_string(),
            });
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════
//  GovernanceChecker — 治理权限检查
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct GovernanceChecker {
    /// 当前会话的治理级别（调用者权限）
    pub session_level: GovernanceLevel,
    /// 权限日志
    pub check_log: Vec<(String, GovernanceLevel, bool)>,
}

impl GovernanceChecker {
    pub fn new(session_level: GovernanceLevel) -> Self {
        Self {
            session_level,
            check_log: Vec::new(),
        }
    }

    /// 检查调用者是否有权执行目标治理级别的操作
    /// 调用者级别必须 >= 目标级别
    pub fn check(&mut self, target: &GovernanceLevel, fn_name: &str) -> RuntimeResult<()> {
        let permitted = match (&self.session_level, target) {
            // Execute 可以执行任何级别
            (GovernanceLevel::Execute, _) => true,
            // Approve 可以执行 Prepare/Suggest/Approve
            (GovernanceLevel::Approve, GovernanceLevel::Execute) => false,
            (GovernanceLevel::Approve, _) => true,
            // Suggest 只能执行 Prepare/Suggest
            (GovernanceLevel::Suggest, GovernanceLevel::Approve)
            | (GovernanceLevel::Suggest, GovernanceLevel::Execute) => false,
            (GovernanceLevel::Suggest, _) => true,
            // Prepare 只能执行 Prepare
            (GovernanceLevel::Prepare, GovernanceLevel::Prepare) => true,
            (GovernanceLevel::Prepare, _) => false,
        };
        self.check_log
            .push((fn_name.to_string(), target.clone(), permitted));
        if !permitted {
            return Err(RuntimeError::GovernanceViolation {
                declared: self.session_level.clone(),
                required: target.clone(),
                fn_name: fn_name.to_string(),
            });
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════
//  TimeMonitor — 时间约束监控
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct TimeMonitor {
    pub start: Instant,
    pub fn_timings: Vec<(String, u64)>, // (fn_name, elapsed_ms)
}

impl Default for TimeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeMonitor {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            fn_timings: Vec::new(),
        }
    }

    /// 记录函数执行耗时
    pub fn record(&mut self, fn_name: &str, elapsed_ms: u64) {
        self.fn_timings.push((fn_name.to_string(), elapsed_ms));
    }

    /// 检查时间约束
    pub fn check_constraint(
        &mut self,
        constraint: &TimeConstraint,
        fn_name: &str,
        actual_ms: u64,
    ) -> Vec<RuntimeError> {
        let mut errors = Vec::new();
        if let Some(latency) = constraint.latency_ms
            && actual_ms > latency
        {
            errors.push(RuntimeError::LatencyViolation {
                declared_ms: latency,
                actual_ms,
                fn_name: fn_name.to_string(),
            });
        }
        if let Some(timeout) = constraint.timeout_ms
            && actual_ms > timeout
        {
            errors.push(RuntimeError::TimeoutExceeded {
                constraint_ms: timeout,
                elapsed_ms: actual_ms,
                fn_name: fn_name.to_string(),
            });
        }
        errors
    }
}
