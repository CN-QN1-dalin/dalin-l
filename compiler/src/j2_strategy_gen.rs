/// Phase J — J2: 策略自动生成 (Strategy Auto-Generation)
///
/// 从成功修复的案例中归纳出新 recovery rule，更新 ConfidenceCalibrator 权重，
/// 并在知识库积累到阈值时触发 hot-recompile 建议。
///
/// # 示例
///
/// ```
/// use dalin_l_compiler::j2_strategy_gen::StrategyGenerator;
/// use dalin_l_compiler::runtime::RecoveryMode;
///
/// let mut generator = StrategyGenerator::new();
/// generator.record_fix(FixRecord {
///     error_id: 1,
///     applied_rule: RecoveryMode::Fallback,
///     success: true,
///     confidence_before: 0.5,
///     confidence_after: 0.9,
/// });
/// let rules = generator.infer_new_rules();
/// ```
use std::collections::{HashMap, HashSet};

use crate::runtime::RecoveryMode;

/// 修复记录：每次恢复操作的追踪数据
#[derive(Debug, Clone)]
pub struct FixRecord {
    /// 关联的错误 ID
    pub error_id: u64,
    /// 应用的恢复模式
    pub applied_rule: RecoveryMode,
    /// 是否成功
    pub success: bool,
    /// 修复前的置信度
    pub confidence_before: f64,
    /// 修复后的置信度
    pub confidence_after: f64,
}

/// 归纳出的恢复规则
#[derive(Debug, Clone)]
pub struct RecoveryRule {
    /// 规则 ID（基于错误模式和恢复模式组合生成）
    pub rule_id: String,
    /// 触发条件描述
    pub triggers_on: String,
    /// 适用的恢复模式
    pub applies_mode: RecoveryMode,
    /// 规则置信度 (0.0 - 1.0)
    pub confidence: f64,
    /// 是否经过回归测试验证
    pub tested: bool,
    /// 被使用了多少次
    pub usage_count: u64,
}

/// 热重编译建议计划
#[derive(Debug, Clone)]
pub struct HotRecompilePlan {
    /// 新规则总数
    pub new_rules_count: u64,
    /// 建议的编译优先级
    pub priority: RecompilePriority,
    /// 预期收益描述
    pub expected_benefit: String,
}

/// 编译优先级
#[derive(Debug, Clone, PartialEq)]
pub enum RecompilePriority {
    Low,    // < 3 条新规则
    Medium, // 3-5 条
    High,   // > 5 条
}

// ── Confidence channel weights ───────────────────────────────────

/// 七通道权重（与 ty2 中的通道对应）
const CHANNEL_NAMES: [&str; 7] = [
    "value", "effect", "capability", "governance", "latency", "confidence", "qn",
];
const WEIGHT_MIN: f64 = 0.05;
const WEIGHT_MAX: f64 = 0.5;
const LEARNING_RATE: f64 = 0.01;

/// 权重状态
struct ChannelWeights {
    weights: HashMap<String, f64>,
    success_counts: HashMap<String, usize>,  // 每个通道的成功次数
    total_counts: HashMap<String, usize>,    // 总执行次数
}

impl ChannelWeights {
    fn new() -> Self {
        Self {
            weights: CHANNEL_NAMES.iter().map(|n| (n.to_string(), 1.0 / 7.0)).collect(),
            success_counts: HashMap::new(),
            total_counts: HashMap::new(),
        }
    }

    /// 根据修复历史计算梯度并更新权重（带边界约束）
    fn update_from_fixes(&mut self, fixes: &[FixRecord]) {
        for ch in &CHANNEL_NAMES {
            let key = ch.to_string();
            let current = *self.weights.get(&key).unwrap_or(&(1.0 / 7.0));

            // 统计该通道相关的修复
            let success_count = self.success_counts.get(&key).copied().unwrap_or(0);
            let total_count = self.total_counts.get(&key).copied().unwrap_or(fixes.len());

            // 成功率
            let success_rate = if total_count == 0 {
                0.5
            } else {
                success_count as f64 / total_count as f64
            };

            // gradient = current - success_rate
            let gradient = current - success_rate;
            let new_w = current - LEARNING_RATE * gradient;
            self.weights.insert(key, new_w.clamp(WEIGHT_MIN, WEIGHT_MAX));
        }
    }

    fn record_outcome(&mut self, outcome: bool) {
        // 对所有通道记录一次 outcome
        for ch in &CHANNEL_NAMES {
            *self.total_counts.entry(ch.to_string()).or_default() += 1;
            if outcome {
                *self.success_counts.entry(ch.to_string()).or_default() += 1;
            }
        }
    }

    fn get_weights(&self) -> HashMap<String, f64> {
        self.weights.clone()
    }
}

// ═══════════════ StrategyGenerator ═══════════════

/// 策略生成器 — 从修复历史中归纳规则、更新权重、触发热编译
pub struct StrategyGenerator {
    fix_history: Vec<FixRecord>,
    known_rules: Vec<RecoveryRule>,
    weights: ChannelWeights,
    rule_seq: u64,
}

impl Default for StrategyGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyGenerator {
    pub fn new() -> Self {
        Self {
            fix_history: Vec::new(),
            known_rules: Vec::new(),
            weights: ChannelWeights::new(),
            rule_seq: 0,
        }
    }

    /// 记录一次修复操作
    pub fn record_fix(&mut self, record: FixRecord) {
        self.fix_history.push(record.clone());
        self.weights.record_outcome(record.success);
    }

    /// 从成功修复案例中归纳新规则
    ///
    /// 对最近的成功修复按 RecoveryMode 分组，统计出现频率最高的模式作为新规则。
    /// 只有出现 >= 2 次的模式才会被生成为规则。
    pub fn infer_new_rules(&mut self) -> Vec<RecoveryRule> {
        let successful: Vec<&FixRecord> =
            self.fix_history.iter().filter(|r| r.success).collect();

        if successful.is_empty() {
            return Vec::new();
        }

                let mut mode_counts: HashMap<RecoveryMode, usize> = HashMap::new();
        for r in &successful {
            *mode_counts.entry(r.applied_rule.clone()).or_default() += 1;
        }

        let mut new_rules = Vec::new();
        let successful_len = successful.len();
        for (mode, count) in &mode_counts {
            if *count >= 2 {
                self.rule_seq += 1;
                let conf = (*count as f64 / successful_len as f64).min(1.0);
                let conf_rounded = (conf * 10000.0).round() / 10000.0;
                new_rules.push(RecoveryRule {
                    rule_id: format!("rule_{}_{}", self.rule_seq, mode),
                    triggers_on: format!("{:?} 恢复模式成功修复 {} 次", mode, count),
                    applies_mode: mode.clone(),
                    confidence: conf_rounded,
                    tested: false,
                    usage_count: *count as u64,
                });
            }
        }

        // 通用规则（基于整体成功率）
        if !new_rules.is_empty() {
            let avg_confidence_increase: f64 = successful
                .iter()
                .map(|r| r.confidence_after - r.confidence_before)
                .sum::<f64>()
                .max(0.0);

            self.rule_seq += 1;
            let conf = (avg_confidence_increase / successful_len as f64).min(1.0);
            let conf_rounded = (conf * 10000.0).round() / 10000.0;
            new_rules.push(RecoveryRule {
                rule_id: format!("rule_{}_general", self.rule_seq),
                triggers_on: "任意成功修复后的通用模式".to_string(),
                applies_mode: RecoveryMode::Fallback,
                confidence: conf_rounded,
                tested: false,
                usage_count: successful_len as u64,
            });
        }

        // 合并到已知规则
        let new_ids: HashSet<String> = new_rules.iter().map(|r| r.rule_id.clone()).collect();
        // 移除同 ID 的旧规则
        self.known_rules.retain(|r| !new_ids.contains(&r.rule_id));
        for new_rule in &new_rules {
            if let Some(existing) = self.known_rules.iter_mut().find(|r| r.rule_id == new_rule.rule_id) {
                existing.usage_count = new_rule.usage_count;
                existing.confidence = new_rule.confidence;
            } else {
                self.known_rules.push(new_rule.clone());
            }
        }

        new_rules
    }

    /// 根据历史准确率做梯度下降更新 ConfidenceCalibrator 权重
    ///
    /// 每通道权重 ∈ [WEIGHT_MIN, WEIGHT_MAX]
    pub fn update_calibrator_weights(&mut self) -> HashMap<String, f64> {
        self.weights.update_from_fixes(&self.fix_history);
        self.weights.get_weights()
    }

    /// 当知识库中有 N+ 条未测试的新规则时触发热编译建议
    pub fn suggest_hot_recompile(&self, threshold: u64) -> Option<HotRecompilePlan> {
        let new_rules = self.known_rules.iter()
            .filter(|r| !r.tested)
            .count() as u64;

        if new_rules >= threshold {
            let priority = match new_rules {
                0..=2 => RecompilePriority::Low,
                3..=5 => RecompilePriority::Medium,
                _ => RecompilePriority::High,
            };

            let benefit_desc = match priority {
                RecompilePriority::High => format!("高优先级：{new_rules} 条未测试规则待验证"),
                RecompilePriority::Medium => format!("中优先级：{new_rules} 条规则积累达到阈值"),
                RecompilePriority::Low => format!("低优先级：{new_rules} 条新规则待集成"),
            };

            Some(HotRecompilePlan {
                new_rules_count: new_rules,
                priority,
                expected_benefit: benefit_desc,
            })
        } else {
            None
        }
    }

    /// 返回当前已知规则列表
    pub fn known_rules(&self) -> &[RecoveryRule] {
        &self.known_rules
    }

    /// 返回修复历史长度
    pub fn history_len(&self) -> usize {
        self.fix_history.len()
    }

    /// 返回所有修复记录
    pub fn fix_history(&self) -> &[FixRecord] {
        &self.fix_history
    }
}

// ═══════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fix(id: u64, rule: RecoveryMode, success: bool) -> FixRecord {
        FixRecord {
            error_id: id,
            applied_rule: rule,
            success,
            confidence_before: 0.5,
            confidence_after: if success { 0.9 } else { 0.3 },
        }
    }

    #[test]
    fn test_record_fix_and_history() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::Fallback, true));
        generator.record_fix(make_fix(2, RecoveryMode::Fallback, true));
        assert_eq!(generator.history_len(), 2);
    }

    #[test]
    fn test_infer_no_rules_when_all_fail() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::Fallback, false));
        generator.record_fix(make_fix(2, RecoveryMode::Fallback, false));
        let rules = generator.infer_new_rules();
        assert!(rules.is_empty(), "no rules from all-failure history");
    }

    #[test]
    fn test_infer_new_rules_with_multiple_fixes() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::Fallback, true));
        generator.record_fix(make_fix(2, RecoveryMode::Fallback, true));
        generator.record_fix(make_fix(3, RecoveryMode::DegradeGovernance, true));
        generator.record_fix(make_fix(4, RecoveryMode::Fallback, true));

        let rules = generator.infer_new_rules();
        assert!(!rules.is_empty(), "should produce rules with multiple successes");

        // Fallback 模式出现了 3 次，应该被识别为独立规则
        let fallback_rules: Vec<_> = rules.iter()
            .filter(|r| matches!(r.applies_mode, RecoveryMode::Fallback))
            .collect();
        assert!(!fallback_rules.is_empty(), "Fallback mode should be in the rules");
    }

    #[test]
    fn test_weight_update_boundary() {
        let mut generator = StrategyGenerator::new();
        // 注入大量成功修复
        for i in 0..20 {
            generator.record_fix(make_fix(i, RecoveryMode::Fallback, true));
        }

        let weights = generator.update_calibrator_weights();

        // 每个通道权重应在 [WEIGHT_MIN, WEIGHT_MAX] 范围内
        for (ch, w) in &weights {
            assert!(
                *w >= WEIGHT_MIN - 0.001 && *w <= WEIGHT_MAX + 0.001,
                "weight for '{}' = {} should be in [{}, {}]",
                ch, w, WEIGHT_MIN, WEIGHT_MAX
            );
        }
    }

    #[test]
    fn test_suggest_hot_recompile_threshold() {
        let mut generator = StrategyGenerator::new();

        // 没有记录 → 无建议
        assert!(generator.suggest_hot_recompile(3).is_none());

        // 注入足够多的成功修复来生成规则
        for i in 0..10 {
            generator.record_fix(make_fix(i, RecoveryMode::Fallback, true));
            generator.record_fix(make_fix(i + 100, RecoveryMode::RetryWithDefault, true));
        }
        generator.infer_new_rules();

        let suggestion = generator.suggest_hot_recompile(3);
        assert!(
            suggestion.is_some(),
            "should suggest hot recompile when threshold reached"
        );

        if let Some(plan) = suggestion {
            assert!(plan.new_rules_count >= 3);
            assert!(matches!(plan.priority, RecompilePriority::High | RecompilePriority::Medium));
        }
    }

    #[test]
    fn test_known_rules_persisted() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::Fallback, true));
        generator.record_fix(make_fix(2, RecoveryMode::Fallback, true));
        generator.infer_new_rules();

        assert!(!generator.known_rules().is_empty(), "rules should be persisted");
        assert_eq!(generator.history_len(), 2, "history length preserved");
    }

    #[test]
    fn test_recovery_modes_distinct_in_rules() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::RetryWithDefault, true));
        generator.record_fix(make_fix(2, RecoveryMode::RetryWithDefault, true));
        generator.record_fix(make_fix(3, RecoveryMode::DegradeGovernance, true));
        generator.record_fix(make_fix(4, RecoveryMode::DegradeGovernance, true));
        generator.infer_new_rules();

        let modes: HashSet<RecoveryMode> = generator.known_rules()
            .iter()
            .map(|r| r.applies_mode.clone())
            .collect();

        assert!(modes.contains(&RecoveryMode::RetryWithDefault),
            "RetryWithDefault should be in rules, got modes: {:?}", modes);
        assert!(modes.contains(&RecoveryMode::DegradeGovernance),
            "DegradeGovernance should be in rules, got modes: {:?}", modes);
    }

    #[test]
    fn test_single_fix_no_pattern_rule() {
        let mut generator = StrategyGenerator::new();
        generator.record_fix(make_fix(1, RecoveryMode::Fallback, true));
        let rules = generator.infer_new_rules();
        // 单一修复不应产生模式规则（因为 < 2 次）
        // 但通用规则可能被创建
        assert!(rules.is_empty() || rules.len() <= 1, "single fix should not produce many rules");
    }
}
