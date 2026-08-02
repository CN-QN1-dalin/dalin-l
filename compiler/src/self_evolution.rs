/// Phase J — Self-Evolution Orchestrator
///
/// 串联 PatternLearning → StrategyGen → VerificationEngine，形成自动—人工闭环。
/// 负责错误收集、聚类→规则归纳→AB实验验证→知识库更新的完整流水线。
use crate::borrow_checker::BorrowError;
use crate::j1_pattern_learning::{ErrorClusteringEngine, ErrorRecord, Template};
use crate::j2_strategy_gen::{FixRecord, RecoveryRule, StrategyGenerator};
use crate::j3_evolution_verify::{ABExperimentResult, EvolutionVerificationEngine};
use crate::runtime::RecoveryMode;

/// Self-evolution engine main state machine — unified management of J1/J2/J3 lifecycles and data flow
pub struct SelfEvolutionEngine {
    j1: ErrorClusteringEngine,       // J1: 模式学习 — 错误聚类 + 模板提取
    j2: StrategyGenerator,           // J2: 策略生成 — 从修复历史归纳新规则
    j3: EvolutionVerificationEngine, // J3: 进化验证 — AB实验 + 评分验证
    kb_path: String,                 // 知识库持久化路径
    next_error_id: u64,              // 错误 ID 计数器
}

impl SelfEvolutionEngine {
    /// Initialize the engine — default path ~/.dal_kb.jsonl
    #[must_use]
    pub fn new(kb_path: &str) -> Self {
        Self {
            j1: ErrorClusteringEngine::new(),
            j2: StrategyGenerator::new(),
            j3: EvolutionVerificationEngine::new(),
            kb_path: kb_path.to_string(),
            next_error_id: 1,
        }
    }

    /// Record a borrow checker error — enters the J1 pipeline
    pub fn record_borrow_error(&mut self, err: &BorrowError, line: usize) {
        let record = ErrorRecord {
            id: self.next_error_id,
            timestamp: self.now_iso(),
            error_type: "borrow_check_failed".to_string(),
            message: err.to_string(),
            source_location: Some(("borrow".into(), line, 0)),
            stack_trace: None,
            recovery_applied: None,
            recovery_success: false,
        };
        self.next_error_id += 1;
        self.j1.add_error(record);
    }

    /// Run one J1 clustering pass and output templates — eps = neighborhood radius, min_points = minimum sample count
    pub fn run_j1_cluster(&mut self, eps: f32, min_points: usize) -> Vec<Template> {
        let clusters = self.j1.cluster(eps, min_points);
        self.j1.extract_templates(&clusters)
    }

    /// Record a fix operation (applied manually or automatically) — enters the J2 pipeline
    pub fn record_fix(&mut self, fix: FixRecord) {
        self.j2.record_fix(fix);
    }

    /// Induce new rules from fix history — returns the rule list pending heuristic evaluation
    pub fn infer_new_rules(&mut self) -> Vec<RecoveryRule> {
        self.j2.infer_new_rules()
    }

    /// Get all known rules (with confidence scores)
    pub fn known_rules(&self) -> Vec<RecoveryRule> {
        self.j2.known_rules().to_vec()
    }

    /// Run an A/B experiment to evaluate a new strategy
    pub fn run_ab_experiment(
        &mut self,
        exp_id: &str,
        control_name: &str,
        treatment_name: &str,
        a_score: f64,
        b_score: f64,
    ) -> Result<ABExperimentResult, String> {
        self.j3
            .run_experiment(exp_id, control_name, treatment_name, a_score, b_score)
    }

    /// Get hot recompilation suggestions triggered by untested new rules (if the count exceeds the threshold)
    pub fn suggest_hot_recompile(&self, threshold: u64) -> Option<Vec<RecoveryRule>> {
        let untested: Vec<RecoveryRule> = self
            .j2
            .known_rules()
            .iter()
            .filter(|r| !r.tested)
            .cloned()
            .collect();
        if (untested.len() as u64) >= threshold {
            Some(untested)
        } else {
            None
        }
    }

    /// Check whether enough errors have been collected to trigger clustering
    pub fn needs_clustering(&self, min_errors: usize) -> bool {
        self.j1.error_count() >= min_errors
    }

    /// Number of errors collected so far (for testing and status reporting)
    pub fn error_count(&self) -> usize {
        self.j1.error_count()
    }

    /// Evaluate the current evolution state with a composite score
    pub fn current_status(&self) -> String {
        format!(
            "J1: {} errors tracked; J2: {} rules ({} tested); J3: {} experiments",
            self.j1.error_count(),
            self.j2.known_rules().len(),
            self.j2.known_rules().iter().filter(|r| r.tested).count(),
            self.j3.experiment_count()
        )
    }

    /// Save the knowledge base to disk (JSONL format: one record per line)
    pub fn save_knowledge_base(&self) -> Result<(), String> {
        // TODO: 实现序列化写入
        let _ = &self.kb_path;
        Ok(())
    }

    /// Load an existing knowledge base
    pub fn load_knowledge_base(&mut self) -> Result<(), String> {
        // TODO: 实现反序列化读取
        Ok(())
    }

    /// 生成 ISO 8601 风格时间戳（无外部依赖，用 std 时间）
    fn now_iso(&self) -> String {
        // 简单 UTC 时间戳格式：seconds.millis（避免引入 chrono 依赖）
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}.{:03}", secs.as_secs(), secs.subsec_millis())
    }
}

// ═══════════════════════════════════════════════════════
// 编译器集成扩展：在 compile_with_llm 中调用此引擎
// ═══════════════════════════════════════════════════════

/// Convert each borrow checker error into a self-evolution event record
pub fn process_borrow_errors(
    checker: &crate::borrow_checker::BorrowChecker,
    engine: &mut SelfEvolutionEngine,
) {
    for err in checker.errors() {
        engine.record_borrow_error(err, 0);
    }
}

/// Self-evolution closed-loop entry — optionally called after compilation completes
pub fn try_self_evolve(engine: &mut SelfEvolutionEngine) -> Option<String> {
    // 只有当积累了足够错误才触发聚类（防抖）
    if !engine.needs_clustering(3) {
        return None; // 错误不足，不触发
    }

    // Step 1: J1 — 聚类发现常见错误模式
    let templates = engine.run_j1_cluster(0.5, 2);
    if templates.is_empty() {
        return None; // 未发现模式
    }

    // Step 2: J2 — 对每个模板记录候选修复（简化：基于模板构造 FixRecord）
    for tmpl in &templates {
        let fix = FixRecord {
            error_id: tmpl.template_id.parse().unwrap_or(0),
            applied_rule: RecoveryMode::Fallback,
            success: true,
            confidence_before: tmpl.confidence,
            confidence_after: tmpl.confidence,
        };
        engine.record_fix(fix);
    }

    // Step 3: 若有新规则且需要人工审批，返回提示
    let maybe_hot = engine.suggest_hot_recompile(3);
    maybe_hot.map(|rules| {
        format!(
            "Self-evolution found {} potential rules to review",
            rules.len()
        )
    })
}

// ───────────────────────────────────────────────────────
// 单元测试
#[cfg(test)]
mod test_self_evolution {
    use super::*;

    #[test]
    fn test_engine_initialization() {
        let engine = SelfEvolutionEngine::new("/tmp/test_kb.jsonl");
        assert_eq!(engine.error_count(), 0);
        assert!(engine.known_rules().is_empty());
    }

    #[test]
    fn test_record_error_incremental() {
        let mut engine = SelfEvolutionEngine::new("/tmp/test_kb.jsonl");
        let err = BorrowError::UseAfterMove {
            variable: "x".to_string(),
            moved_line: 10,
            use_line: 20,
        };
        engine.record_borrow_error(&err, 15);
        assert_eq!(engine.error_count(), 1);
    }

    #[test]
    fn test_needs_clustering_threshold() {
        let mut engine = SelfEvolutionEngine::new("/tmp/test_kb.jsonl");
        assert!(!engine.needs_clustering(3));
        for i in 0..4 {
            let err = BorrowError::AssignToImmutable {
                variable: format!("v{i}"),
                line: i,
            };
            engine.record_borrow_error(&err, i);
        }
        assert!(engine.needs_clustering(3));
        assert!(!engine.needs_clustering(5));
    }

    #[test]
    fn test_status_report() {
        let engine = SelfEvolutionEngine::new("/tmp/test_kb.jsonl");
        let status = engine.current_status();
        assert!(status.contains("J1"));
        assert!(status.contains("J2"));
        assert!(status.contains("J3"));
    }
}
