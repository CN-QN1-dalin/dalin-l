/// Dalin L 3.0 — Self-Healing & Self-Evolving Runtime
///
/// Recovery mechanisms, confidence calibration, and runtime code evolution.
use std::fmt;
use std::time::Instant;

use crate::ast::Program;
use crate::qn1::{Qn1Backend, Qn1CodeGenerator};

use super::engine::{Runtime, RuntimeEvent};use super::scheduler::{CognitiveLoopPhase};
use super::value::{RuntimeResult, RuntimeError, RuntimeValue};

// ═══════════════════════════════════════════
//  Recovery Mode & Events
// ═══════════════════════════════════════════

/// 错误恢复模式
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecoveryMode {
    /// 回退到上一认知相位重试（Act→Reason）
    Fallback,
    /// 使用默认值重试
    RetryWithDefault,
    /// 降级治理级别（Execute→Approve→Suggest→Prepare）
    DegradeGovernance,
}

impl fmt::Display for RecoveryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryMode::Fallback => write!(f, "Fallback"),
            RecoveryMode::RetryWithDefault => write!(f, "RetryWithDefault"),
            RecoveryMode::DegradeGovernance => write!(f, "DegradeGovernance"),
        }
    }
}

/// 恢复事件日志
#[derive(Debug, Clone)]
pub struct RecoveryEvent {
    pub fn_name: String,
    pub error: RuntimeError,
    pub mode: RecoveryMode,
    pub success_after_recovery: bool,
    pub timestamp_us: u64,
}

/// 进化事件日志
#[derive(Debug, Clone)]
pub struct EvolutionEvent {
    pub fn_name: String,
    pub prompt: String,
    pub old_body_len: usize,
    pub new_body_len: usize,
    pub new_code: String,
}

// ═══════════════════════════════════════════
//  SelfHealingRuntime
// ═══════════════════════════════════════════

use crate::ty2::GovernanceLevel;

/// 自修复运行时 — 包装 Runtime 添加错误恢复能力
pub struct SelfHealingRuntime {
    pub inner: Runtime,
    pub recovery_mode: RecoveryMode,
    pub recovery_log: Vec<RecoveryEvent>,
    recovery_seq: u64,
    start_time: Instant,
}

impl SelfHealingRuntime {
    pub fn new(session_governance: GovernanceLevel) -> Self {
        Self {
            inner: Runtime::new(session_governance),
            recovery_mode: RecoveryMode::Fallback,
            recovery_log: Vec::new(),
            recovery_seq: 0,
            start_time: Instant::now(),
        }
    }

    /// 调用函数，含自修复逻辑
    pub fn call_with_healing(
        &mut self,
        fn_name: &str,
        args: &[RuntimeValue],
    ) -> RuntimeResult<RuntimeValue> {
        self.recovery_log.clear();
        self.recovery_seq = 0;
        let result = self.inner.call(fn_name, args);

        match &result {
            Err(err) => {
                match self.recovery_mode {
                    RecoveryMode::Fallback => {
                        if matches!(err, RuntimeError::CognitiveLoopViolation { .. }) {
                            self.inner.cognitive.current_phase = CognitiveLoopPhase::Reasoning;
                            let retry_result = self.inner.call(fn_name, args);

                            let success = retry_result.is_ok();
                            self.recovery_log.push(RecoveryEvent {
                                fn_name: fn_name.to_string(),
                                error: err.clone(),
                                mode: RecoveryMode::Fallback,
                                success_after_recovery: success,
                                timestamp_us: self.start_time.elapsed().as_micros() as u64,
                            });

                            if success {
                                retry_result
                            } else {
                                self.inner.cognitive.current_phase = CognitiveLoopPhase::Acting;
                                Err(err.clone())
                            }
                        } else {
                            Err(err.clone())
                        }
                    }
                    RecoveryMode::RetryWithDefault => {
                        if matches!(err, RuntimeError::DivisionByZero) {
                            self.recovery_log.push(RecoveryEvent {
                                fn_name: fn_name.to_string(),
                                error: err.clone(),
                                mode: RecoveryMode::RetryWithDefault,
                                success_after_recovery: true,
                                timestamp_us: self.start_time.elapsed().as_micros() as u64,
                            });
                            Ok(RuntimeValue::Int(0))
                        } else {
                            Err(err.clone())
                        }
                    }
                    RecoveryMode::DegradeGovernance => {
                        if matches!(err, RuntimeError::GovernanceViolation { .. }) {
                            let new_level = match self.inner.governance.session_level {
                                GovernanceLevel::Execute => GovernanceLevel::Approve,
                                GovernanceLevel::Approve => GovernanceLevel::Suggest,
                                GovernanceLevel::Suggest => GovernanceLevel::Prepare,
                                GovernanceLevel::Prepare => return Err(err.clone()),
                            };

                            self.inner.governance.session_level = new_level;
                            let retry_result = self.inner.call(fn_name, args);
                            let success = retry_result.is_ok();

                            self.recovery_log.push(RecoveryEvent {
                                fn_name: fn_name.to_string(),
                                error: err.clone(),
                                mode: RecoveryMode::DegradeGovernance,
                                success_after_recovery: success,
                                timestamp_us: self.start_time.elapsed().as_micros() as u64,
                            });

                            if success {
                                retry_result
                            } else {
                                Err(err.clone())
                            }
                        } else {
                            Err(err.clone())
                        }
                    }
                }
            }
            Ok(_) => result,
        }
    }

    /// 返回恢复事件总数
    pub fn recovery_count(&self) -> usize {
        self.recovery_log.len()
    }
}

// ═══════════════════════════════════════════
//  ConfidenceCalibrator
// ═══════════════════════════════════════════

use std::collections::HashMap;

/// 置信度校准器 — 根据历史执行准确率动态调整 confidence
pub struct ConfidenceCalibrator {
    calibration_table: HashMap<String, Vec<(f64, bool)>>, // (expected_confidence, success)
    #[allow(dead_code)]
    step_size: f64,
}

impl ConfidenceCalibrator {
    pub fn new(step_size: f64) -> Self {
        Self {
            calibration_table: HashMap::new(),
            step_size,
        }
    }

    /// 记录执行结果
    pub fn record_outcome(
        &mut self,
        fn_name: &str,
        expected_confidence: f64,
        actual_success: bool,
    ) {
        let entry = self
            .calibration_table
            .entry(fn_name.to_string())
            .or_default();
        entry.push((expected_confidence, actual_success));
    }

    /// 计算校准后的置信度
    pub fn calibrated_confidence(&self, fn_name: &str) -> f64 {
        if let Some(entries) = self.calibration_table.get(fn_name) {
            if entries.is_empty() {
                return 0.85;
            }
            let successes: f64 =
                entries.iter().filter(|(_, s)| *s).count() as f64 / entries.len() as f64;
            successes.clamp(0.1, 1.0)
        } else {
            0.85
        }
    }

    /// 获取某个函数的历史统计
    pub fn stats(&self, fn_name: &str) -> Option<(usize, f64)> {
        self.calibration_table.get(fn_name).map(|entries| {
            let total = entries.len();
            let success_rate =
                entries.iter().filter(|(_, s)| *s).count() as f64 / total as f64;
            (total, success_rate)
        })
    }
}

// ═══════════════════════════════════════════
//  RuntimeSelfEvolution
// ═══════════════════════════════════════════

/// 运行时代码进化器 — 允许 @llm 在运行时生成新代码
pub struct RuntimeSelfEvolution {
    qn1_generator: Qn1CodeGenerator,
    pub evolution_log: Vec<EvolutionEvent>,
}

impl RuntimeSelfEvolution {
    pub fn new(backend: Box<dyn Qn1Backend>) -> Self {
        Self {
            qn1_generator: Qn1CodeGenerator::new(backend),
            evolution_log: Vec::new(),
        }
    }

    pub fn new_mock() -> Self {
        Self {
            qn1_generator: Qn1CodeGenerator::new_mock(),
            evolution_log: Vec::new(),
        }
    }

    /// 进化指定函数：调用 QN1 生成新代码并热替换
    pub fn evolve(&mut self, fn_name: &str, prompt: &str) -> crate::qn1::Qn1GeneratedCode {
        use std::collections::HashMap;
        let ctx = crate::qn1::GenerationContext {
            fn_name: Some(fn_name.to_string()),
            params: Vec::new(),
            annotations: HashMap::new(),
        };
        let result = self.qn1_generator.generate(prompt, &ctx);

        let new_code = format!("{:?}", result.statements);
        self.evolution_log.push(EvolutionEvent {
            fn_name: fn_name.to_string(),
            prompt: prompt.to_string(),
            old_body_len: 0,
            new_body_len: result.statements.len(),
            new_code,
        });

        result
    }
}

/// 便利函数：创建自修复运行时并执行程序
pub fn run_with_healing(
    prog: &Program,
    entry: &str,
    governance_level: GovernanceLevel,
) -> RuntimeResult<Vec<RuntimeEvent>> {
    let mut healing_rt = SelfHealingRuntime::new(governance_level);
    healing_rt.inner.load_program(prog);
    let _result = healing_rt.call_with_healing(entry, &[])?;
    Ok(healing_rt.inner.events)
}
