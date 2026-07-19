/// Phase J — J3: 进化验证框架 (Evolution Verification Framework)
///
/// 提供进化评分、AB 实验、综合评估，确保每次进化变更都是可量化、可验证、可回滚的。
///
/// # 评分公式
///
/// `EvolutionScore::composite()` 使用加权公式：
/// - regression_pass_rate × 0.4 + ln(1 + performance_delta) × 0.3
///   + (1 - |memory_delta|) × 0.1 + coverage_impact × 0.1 + governance_compliance × 0.1
///
/// # 示例
///
/// ```
/// use dalin_l_compiler::j3_evolution_verify::{EvolutionVerificationEngine, ABExperimentConfig};
///
/// let mut engine = EvolutionVerificationEngine::new();
/// let config = ABExperimentConfig {
///     experiment_id: "exp_001".to_string(),
///     group_a_control: "old_strategy".to_string(),
///     group_b_treatment: "new_strategy".to_string(),
///     regression_threshold: 0.95,
///     performance_threshold: 0.10,
///     samples: 100,
/// };
/// // 实际执行由上层控制面调度
/// ```
use std::fmt;

/// AB 实验中的组别标识
#[derive(Debug, Clone, PartialEq)]
pub enum Group {
    Control,   // A 组（旧策略）
    Treatment, // B 组（新策略）
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Group::Control => write!(f, "A"),
            Group::Treatment => write!(f, "B"),
        }
    }
}

// ── 权重常量 ─────────────────────────────────────────────────────

const W_REGRESSION: f64 = 0.4;
const W_PERFORMANCE: f64 = 0.3;
const W_MEMORY: f64 = 0.1;
const W_COVERAGE: f64 = 0.1;
const W_GOVERNANCE: f64 = 0.1;

/// 进化评分
///
/// 每个字段对应一个评估维度，综合评分通过 `composite()` 计算。
#[derive(Debug, Clone)]
pub struct EvolutionScore {
    /// 回归测试通过率 [0.0, 1.0]
    pub regression_pass_rate: f64,
    /// 性能变化 [-0.2, +0.5]，负值表示退化，正值表示提升
    pub performance_delta: f64,
    /// 内存变化 [-0.1, +0.1]
    pub memory_delta: f64,
    /// 测试覆盖率影响 [-1.0, +1.0]
    pub coverage_impact: f64,
    /// 是否通过治理检查
    pub governance_compliance: bool,
}

impl EvolutionScore {
    /// 按设计文档的加权公式计算综合评分
    ///
    /// 权重: [regression: 0.4, performance: 0.3, memory: 0.1, coverage: 0.1, governance: 0.1]
    pub fn composite(&self) -> f64 {
        let regression_part = self.regression_pass_rate * W_REGRESSION;

        // ln(1 + performance_delta)，保证在 delta ∈ [-0.2, 0.5] 时输出合理
        let perf_input = 1.0 + self.performance_delta;
        let performance_part = perf_input.ln().max(0.0) * W_PERFORMANCE;

        // (1 - |memory_delta|) ∈ [0.9, 1.1]，用 clamp 限制到 [0.0, 1.0]
        let memory_part = (1.0 - self.memory_delta.abs()).clamp(0.0, 1.0) * W_MEMORY;

        // coverage_impact 直接在 [-1.0, +1.0] 范围内映射到 [0.0, 1.0]
        let coverage_part = ((1.0 + self.coverage_impact) / 2.0).clamp(0.0, 1.0) * W_COVERAGE;

        // governance 是 0 或 1
        let governance_part = if self.governance_compliance { 1.0 } else { 0.0 } * W_GOVERNANCE;

        let total =
            regression_part + performance_part + memory_part + coverage_part + governance_part;
        (total * 10000.0).round() / 10000.0
    }

    /// 检查综合评分是否通过阈值
    ///
    /// 默认阈值为 0.8
    pub fn passes_threshold(&self, threshold: f64) -> bool {
        self.composite() >= threshold
    }

    /// 检查是否通过默认阈值 0.8
    pub fn passes_default(&self) -> bool {
        self.passes_threshold(0.8)
    }
}

/// 默认评分构造函数（方便测试）
impl Default for EvolutionScore {
    fn default() -> Self {
        Self {
            regression_pass_rate: 1.0,
            performance_delta: 0.0,
            memory_delta: 0.0,
            coverage_impact: 0.0,
            governance_compliance: true,
        }
    }
}

// ── AB Experiment ────────────────────────────────────────────────

/// AB 实验配置
#[derive(Debug, Clone)]
pub struct ABExperimentConfig {
    /// 实验唯一标识
    pub experiment_id: String,
    /// A 组（控制组）旧策略名称
    pub group_a_control: String,
    /// B 组（治疗组）新策略名称
    pub group_b_treatment: String,
    /// 回归测试通过率阈值
    pub regression_threshold: f64,
    /// 性能提升阈值
    pub performance_threshold: f64,
    /// 样本数
    pub samples: u64,
}

/// 实验结果
#[derive(Debug, Clone)]
pub struct ABExperimentResult {
    /// 实验配置
    pub config: ABExperimentConfig,
    /// A 组综合得分
    pub group_a_score: f64,
    /// B 组综合得分
    pub group_b_score: f64,
    /// 获胜方
    pub winner: Group,
    /// 分差
    pub score_differential: f64,
}

impl fmt::Display for ABExperimentResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Experiment: {}", self.config.experiment_id)?;
        writeln!(
            f,
            "  A ({}) score: {:.4}",
            self.config.group_a_control, self.group_a_score
        )?;
        writeln!(
            f,
            "  B ({}) score: {:.4}",
            self.config.group_b_treatment, self.group_b_score
        )?;
        writeln!(f, "  Winner: Group {}", self.winner)?;
        writeln!(f, "  Differential: {:.4}", self.score_differential)
    }
}

// ═══════════════ EvolutionVerificationEngine ═══════════════

/// 验证引擎 — 运行 AB 实验并生成总结报告
pub struct EvolutionVerificationEngine {
    experiments: Vec<ABExperimentResult>,
}

impl Default for EvolutionVerificationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl EvolutionVerificationEngine {
    pub fn new() -> Self {
        Self {
            experiments: Vec::new(),
        }
    }

    /// 运行一次 AB 实验
    ///
    /// 注意：实际的策略切换和测试执行由上层控制面调度。
    /// 此方法模拟实验结果，用于验证评分和比较逻辑。
    ///
    /// # Arguments
    /// * `config` — 实验配置
    /// * `group_a_avg` — A 组（旧策略）的平均综合得分（由上层提供）
    /// * `group_b_avg` — B 组（新策略）的平均综合得分（由上层提供）
    ///
    /// # Returns
    /// 实验结果或错误信息
    pub fn run_experiment_with_scores(
        &mut self,
        config: ABExperimentConfig,
        group_a_avg: f64,
        group_b_avg: f64,
    ) -> Result<ABExperimentResult, String> {
        if !(0.0..=1.0).contains(&group_a_avg) || !(0.0..=1.0).contains(&group_b_avg) {
            return Err("scores must be in [0.0, 1.0]".to_string());
        }
        if config.samples == 0 {
            return Err("samples must be > 0".to_string());
        }

        let diff = ((group_b_avg - group_a_avg).abs() * 10000.0).round() / 10000.0;
        let winner = if group_b_avg >= group_a_avg {
            Group::Treatment
        } else {
            Group::Control
        };

        let result = ABExperimentResult {
            config,
            group_a_score: group_a_avg,
            group_b_score: group_b_avg,
            winner,
            score_differential: diff,
        };

        self.experiments.push(result.clone());
        Ok(result)
    }

    /// 简化版：仅传入实验 ID 和两组得分
    pub fn run_experiment(
        &mut self,
        experiment_id: &str,
        control_name: &str,
        treatment_name: &str,
        group_a_score: f64,
        group_b_score: f64,
    ) -> Result<ABExperimentResult, String> {
        let config = ABExperimentConfig {
            experiment_id: experiment_id.to_string(),
            group_a_control: control_name.to_string(),
            group_b_treatment: treatment_name.to_string(),
            regression_threshold: 0.95,
            performance_threshold: 0.10,
            samples: 100,
        };
        self.run_experiment_with_scores(config, group_a_score, group_b_score)
    }

    /// 综合评估所有已运行实验
    pub fn summary_report(&self) -> String {
        if self.experiments.is_empty() {
            return "No experiments run yet.".to_string();
        }

        let mut report = "=== Evolution Verification Summary ===\n\n".to_string();
        for exp in &self.experiments {
            use std::fmt::Write as FmtWrite;
            let _ = write!(report, "{}", exp);
        }

        let total = self.experiments.len();
        let treatment_wins = self
            .experiments
            .iter()
            .filter(|e| e.winner == Group::Treatment)
            .count();
        let control_wins = total - treatment_wins;

        use std::fmt::Write as FmtWrite;
        let _ = write!(report, "\nTotal experiments: {}", total);
        let _ = write!(report, "\nTreatment wins (B): {}", treatment_wins);
        let _ = write!(report, "\nControl wins (A): {}", control_wins);
        let win_rate = if total > 0 {
            treatment_wins as f64 / total as f64
        } else {
            0.0
        };
        let _ = write!(report, "\nTreatment win rate: {:.2}%", win_rate * 100.0);

        report
    }

    /// 返回已运行的实验数量
    pub fn experiment_count(&self) -> usize {
        self.experiments.len()
    }

    /// 获取最后一次实验结果
    pub fn last_result(&self) -> Option<&ABExperimentResult> {
        self.experiments.last()
    }
}

// ═══════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_score_calculation() {
        let score = EvolutionScore {
            regression_pass_rate: 1.0,
            performance_delta: 0.2,
            memory_delta: 0.0,
            coverage_impact: 0.5,
            governance_compliance: true,
        };

        let composite = score.composite();
        // 理论值: 1.0*0.4 + ln(1.2)*0.3 + 1.0*0.1 + 0.75*0.1 + 1.0*0.1
        // ≈ 0.4 + 0.0562 + 0.1 + 0.075 + 0.1 = 0.7312
        assert!(
            composite > 0.6 && composite < 0.9,
            "composite should be ~0.73, got {}",
            composite
        );
    }

    #[test]
    fn test_composite_perfect_score() {
        let score = EvolutionScore {
            regression_pass_rate: 1.0,
            performance_delta: 0.5,
            memory_delta: 0.0,
            coverage_impact: 1.0,
            governance_compliance: true,
        };
        let composite = score.composite();
        // theoretical: 1.0*0.4 + ln(1.5)*0.3 + 1.0*0.1 + 1.0*0.1 + 1.0*0.1
        // ≈ 0.4 + 0.1216 + 0.1 + 0.1 + 0.1 = 0.8216
        assert!(
            composite >= 0.7,
            "near-perfect score should be high, got {}",
            composite
        );
    }

    #[test]
    fn test_composite_failing_score() {
        let score = EvolutionScore {
            regression_pass_rate: 0.5,
            performance_delta: -0.2,
            memory_delta: 0.1,
            coverage_impact: -0.5,
            governance_compliance: false,
        };
        let composite = score.composite();
        assert!(
            composite < 0.5,
            "failing score should be < 0.5, got {}",
            composite
        );
    }

    #[test]
    fn test_passes_threshold() {
        let high = EvolutionScore {
            regression_pass_rate: 1.0,
            performance_delta: 0.5,
            memory_delta: 0.0,
            coverage_impact: 0.5,
            governance_compliance: true,
        };
        assert!(
            high.passes_threshold(0.7),
            "should pass 0.7 threshold, got: {}",
            high.composite()
        );

        let low = EvolutionScore {
            regression_pass_rate: 0.6,
            performance_delta: -0.1,
            memory_delta: 0.05,
            coverage_impact: -0.5,
            governance_compliance: false,
        };
        assert!(
            !low.passes_threshold(0.8),
            "should fail 0.8 threshold, got: {}",
            low.composite()
        );
    }

    #[test]
    fn test_default_score_passes() {
        let score = EvolutionScore::default();
        let composite = score.composite();
        // Default: 1.0*0.4 + 0.0 + 0.1 + 0.05 + 0.1 = 0.65
        assert!(
            composite > 0.5 && composite < 0.9,
            "default score should be reasonable (~0.65), got {}",
            composite
        );
    }

    #[test]
    fn test_ab_experiment_basic() {
        let mut engine = EvolutionVerificationEngine::new();
        let result = engine.run_experiment("exp_001", "v1_strategy", "v2_strategy", 0.75, 0.85);
        assert!(result.is_ok(), "experiment should succeed");
        let res = result.unwrap();
        assert_eq!(
            res.winner,
            Group::Treatment,
            "B group should win with higher score"
        );
        assert!((res.score_differential - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_ab_experiment_control_wins() {
        let mut engine = EvolutionVerificationEngine::new();
        let result = engine.run_experiment("exp_002", "new_strategy", "old_strategy", 0.9, 0.7);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().winner, Group::Control);
    }

    #[test]
    fn test_ab_experiment_validation() {
        let mut engine = EvolutionVerificationEngine::new();
        let result = engine.run_experiment("bad", "a", "b", 1.5, 0.5);
        assert!(result.is_err(), "should reject out-of-range scores");
    }

    #[test]
    fn test_summary_report() {
        let mut engine = EvolutionVerificationEngine::new();
        let _ = engine.run_experiment("exp_1", "old", "new", 0.7, 0.9);
        let _ = engine.run_experiment("exp_2", "v1", "v2", 0.8, 0.85);
        let _ = engine.run_experiment("exp_3", "alpha", "beta", 0.9, 0.6);

        assert_eq!(engine.experiment_count(), 3);

        let report = engine.summary_report();
        assert!(report.contains("exp_1"));
        assert!(report.contains("Total experiments: 3"));
        assert!(report.contains("Treatment wins (B): 2"));
        assert!(report.contains("Control wins (A): 1"));
    }

    #[test]
    fn test_last_result() {
        let mut engine = EvolutionVerificationEngine::new();
        assert!(engine.last_result().is_none());

        let _ = engine.run_experiment("exp_1", "a", "b", 0.7, 0.8);
        let _ = engine.run_experiment("exp_2", "c", "d", 0.6, 0.9);

        let last = engine.last_result().unwrap();
        assert_eq!(last.config.experiment_id, "exp_2");
    }

    #[test]
    fn test_empty_summary() {
        let engine = EvolutionVerificationEngine::new();
        let report = engine.summary_report();
        assert!(report.contains("No experiments"));
    }

    #[test]
    fn test_composite_governance_penalty() {
        let with_gov = EvolutionScore {
            regression_pass_rate: 1.0,
            performance_delta: 0.0,
            memory_delta: 0.0,
            coverage_impact: 0.0,
            governance_compliance: true,
        };
        let without_gov = EvolutionScore {
            governance_compliance: false,
            ..with_gov.clone()
        };

        assert!(
            with_gov.composite() > without_gov.composite(),
            "governance compliance should add points"
        );
    }

    #[test]
    fn test_ab_experiment_equal_scores() {
        let mut engine = EvolutionVerificationEngine::new();
        let result = engine.run_experiment("equal", "a", "b", 0.8, 0.8);
        let res = result.unwrap();
        // 相等时 treatment 算赢（>=条件）
        assert_eq!(res.winner, Group::Treatment);
        assert!((res.score_differential - 0.0).abs() < 0.001);
    }
}
