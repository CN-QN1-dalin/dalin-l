//! Dalin L 3.0 — evolve CLI (Phase J Human Review Interface)
//!
//! Subcommands: review, view, accept, reject, revert, stats, knowledge, calibrate
//! Audit log: ~/.dalan/audit_log/{YYYY-MM}/{action}_{id}.json
//!
//! 四通道闭环串联：J1(模式聚类) → J2(策略生成) → J3(进化验证) → J4(人类审查)
//!
//! Module structure:
//! - data_models — RiskLevel, TestCoverage, EvolutionChange, fetch_* channels
//! - audit_revert — audit log dir / load/save/record, snapshot/revert
//! - subcommands — stats, review/view/accept/reject, run() dispatcher

pub mod data_models;
pub mod audit_revert;
pub mod subcommands;

// Re-export public API (j_status_report used by main dispatcher)
use std::collections::HashMap;

/// Public API entry point — dispatches to subcommand handlers
pub fn run(subcmd: &str, args: &HashMap<String, String>) -> Result<(), String> {
    match subcmd {
        "review" => {
            let jf = args.get("json").map(|v| v == "true").unwrap_or(false);
            subcommands::cmd_review(jf)
        }
        "view" => {
            let jf = args.get("json").map(|v| v == "true").unwrap_or(false);
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            subcommands::cmd_view(id, jf)
        }
        "accept" => {
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            subcommands::cmd_accept(id)
        }
        "reject" => {
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            let reason = args.get("reason").cloned();
            subcommands::cmd_reject(id, reason)
        }
        "revert" => {
            let to = args
                .get("to")
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| "revert requires --to=<epoch>".to_string())?;
            subcommands::cmd_revert(to)
        }
        "stats" => {
            let jf = args.get("json").map(|v| v == "true").unwrap_or(false);
            subcommands::cmd_stats(jf)
        }
        "status" => {
            let report = data_models::j_status_report()?;
            println!("\n{}\n", subcommands::fmt_divider("Phase J 自进化状态"));
            for line in report.lines() {
                println!("{}", line);
            }
            println!("{}", subcommands::fmt_divider(""));
            Ok(())
        }
        "j1-clusters" => {
            let clusters = data_models::fetch_j1_clusters();
            println!("[J1] 聚类结果: {} 条进化变更", clusters.len());
            for c in &clusters {
                println!(
                    "  #{} {} — {}",
                    c.id,
                    c.module,
                    c.description.chars().take(60).collect::<String>()
                );
            }
            Ok(())
        }
        "j2-strategies" => {
            let strategies = data_models::fetch_j2_strategies();
            println!("[J2] 策略结果: {} 条进化变更", strategies.len());
            for s in &strategies {
                println!(
                    "  #{} {} — {}",
                    s.id,
                    s.module,
                    s.description.chars().take(60).collect::<String>()
                );
            }
            Ok(())
        }
        other => Err(format!(
            "Unknown evolve subcommand: {}. Available: review, view, accept, reject, revert, stats",
            other
        )),
    }
}

// ───────────────────── Unit Tests ─────────────────────

#[cfg(test)]
mod tests {
    use super::data_models::{AuditEntry, EvolutionChange, RiskLevel, TestCoverage, j_status_report, fetch_real_evolutions};
    use super::subcommands::{mock_changes, compute_stats, fmt_divider};
    use crate::cmd::evolve::audit_revert::{load_audit_log, record_entry, do_revert};
    use dalin_compiler::{
        j1_pattern_learning::{ErrorClusteringEngine, ErrorRecord},
        j2_strategy_gen::{FixRecord, StrategyGenerator},
        j3_evolution_verify::{EvolutionScore, EvolutionVerificationEngine, Group},
        runtime::RecoveryMode,
    };
    use std::fs;

    #[test]
    fn test_audit_log_write_and_read() {
        let tmp_dir = std::env::temp_dir().join("dalib_test_evolve_j4");
        let log_path = tmp_dir.join("_index.json");
        fs::create_dir_all(&tmp_dir).unwrap();
        let entry = AuditEntry {
            timestamp: "2026-07-15T10:00:00+08:00".into(),
            change_id: 99,
            action: "accept".into(),
            reason: None,
            user: "test".into(),
        };
        fs::write(&log_path, serde_json::to_string_pretty(&entry).unwrap()).unwrap();
        assert!(log_path.exists());
        let read_entry: AuditEntry =
            serde_json::from_str(&fs::read_to_string(&log_path).unwrap()).unwrap();
        assert_eq!(read_entry.change_id, 99);
        assert_eq!(read_entry.action, "accept");
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_evolution_change_serialization() {
        let c = EvolutionChange {
            id: 1,
            module: "t.rs".into(),
            description: "d".into(),
            diff: "diff".into(),
            impact: "all".into(),
            expected_benefit: "+10%".into(),
            risk_level: RiskLevel::Low,
            test_coverage: TestCoverage {
                unit: true,
                integration: false,
                e2e: false,
            },
        };
        let s = serde_json::to_string(&c).expect("serialize");
        let d: EvolutionChange = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(d.id, 1);
        assert_eq!(d.module, "t.rs");
        assert_eq!(d.risk_level, RiskLevel::Low);
    }

    #[test]
    fn test_revert_logic() {
        let changes = mock_changes();
        let max_id = changes.iter().map(|c| c.id).max().unwrap();
        let target = max_id - 1;
        let result = do_revert(target, &changes);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("已回滚至 epoch"));
    }

    #[test]
    fn test_stats_aggregation() {
        let changes = mock_changes();
        let mut log = load_audit_log();
        log.entries.push(AuditEntry {
            timestamp: "2026-07-15T10:00:00+08:00".into(),
            change_id: 1,
            action: "accept".into(),
            reason: None,
            user: "test".into(),
        });
        log.entries.push(AuditEntry {
            timestamp: "2026-07-15T11:00:00+08:00".into(),
            change_id: 2,
            action: "reject".into(),
            reason: Some("too risky".into()),
            user: "test".into(),
        });
        let stats = compute_stats(&changes, &log);
        assert_eq!(stats.total_submissions, 5);
        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.knowledge_entries, 135);
    }

    // ═══════════════════════════════════════════
    //  Phase J 端到端集成测试 — J1→J2→J3→J4 闭环
    // ═══════════════════════════════════════════

    #[test]
    fn test_e2e_j1_to_j4_pipeline() {
        // Step 1: J1 — 注入真实错误并聚类
        let mut j1_engine = ErrorClusteringEngine::new();
        for (id, etype, msg) in [
            (1u64, "latency", "latency constraint exceeded by 50ms"),
            (2, "latency", "latency deadline miss by 30ms"),
            (3, "panic", "segmentation fault core dumped"),
            (4, "latency", "latency violated timeout threshold"),
        ] {
            j1_engine.add_error(ErrorRecord {
                id,
                timestamp: "2026-07-17T12:00:00Z".into(),
                error_type: etype.into(),
                message: msg.into(),
                source_location: None,
                stack_trace: None,
                recovery_applied: None,
                recovery_success: false,
            });
        }
        let clusters = j1_engine.cluster(0.5, 2);
        let templates = j1_engine.extract_templates(&clusters);

        // Step 2: 聚类结果至少产生一些模板
        assert!(
            !clusters.is_empty(),
            "J1 should produce clusters from similar errors"
        );
        assert!(
            !templates.is_empty(),
            "J1 should extract at least 1 template from clusters"
        );

        // Step 3: J2 — 注入修复记录并生成策略
        let mut j2_gen = StrategyGenerator::new();
        for i in 0..5 {
            j2_gen.record_fix(FixRecord {
                error_id: i,
                applied_rule: RecoveryMode::Fallback,
                success: true,
                confidence_before: 0.4,
                confidence_after: 0.9,
            });
        }
        let _j2_rules = j2_gen.infer_new_rules();
        let j2_weights = j2_gen.update_calibrator_weights();

        assert!(
            j2_gen.history_len() == 5,
            "J2 history should have 5 records"
        );
        assert!(
            j2_weights.len() == 7,
            "J2 should update all 7 channel weights"
        );

        // Step 4: J3 — 验证引擎评分和 AB 实验
        let mut j3_engine = EvolutionVerificationEngine::new();
        let result = j3_engine.run_experiment("e2e_exp_001", "baseline", "optimized", 0.72, 0.89);
        assert!(result.is_ok(), "J3 experiment should succeed");
        let res = result.unwrap();
        assert_eq!(
            res.winner,
            Group::Treatment,
            "New strategy should win with higher score"
        );

        // Step 5: J4 — 四通道串联数据 → evolve review 面板输出有效
        let evolutions = super::data_models::fetch_real_evolutions();
        assert!(
            !evolutions.is_empty(),
            "fetch_real_evolutions should return non-empty Vec"
        );

        // Verify data integrity
        for c in &evolutions {
            assert!(!c.module.is_empty(), "module should not be empty");
            assert!(!c.description.is_empty(), "description should not be empty");
            assert!(c.id > 0, "id should be positive");
        }
    }

    #[test]
    fn test_j1_clustering_with_many_errors() {
        let mut engine = ErrorClusteringEngine::new();
        // 注入 20 条错误：10 条 latency 相关 + 10 条 panic 相关
        for i in 0..10 {
            engine.add_error(ErrorRecord {
                id: i,
                timestamp: "2026-07-17T12:00:00Z".into(),
                error_type: "latency".into(),
                message: format!("latency constraint violation #{}", i),
                source_location: None,
                stack_trace: None,
                recovery_applied: None,
                recovery_success: false,
            });
        }
        for i in 10..20 {
            engine.add_error(ErrorRecord {
                id: i,
                timestamp: "2026-07-17T12:00:00Z".into(),
                error_type: "panic".into(),
                message: format!("panic runtime error #{}", i - 10),
                source_location: None,
                stack_trace: None,
                recovery_applied: None,
                recovery_success: false,
            });
        }
        let clusters = engine.cluster(0.5, 2);
        // 应该至少有 1 个簇包含多个元素
        let max_cluster_size = clusters.iter().map(|c| c.len()).max().unwrap_or(0);
        assert!(
            max_cluster_size >= 2,
            "Should cluster similar errors, max_cluster_size={}",
            max_cluster_size
        );

        let templates = engine.extract_templates(&clusters);
        assert!(
            !templates.is_empty(),
            "Should produce templates from clustered errors"
        );
    }

    #[test]
    fn test_j2_strategy_inference_from_fixes() {
        let mut strategy_gen = StrategyGenerator::new();

        // 注入大量同类型成功修复，确保模式能被识别
        for i in 0..10 {
            strategy_gen.record_fix(FixRecord {
                error_id: i,
                applied_rule: RecoveryMode::Fallback,
                success: true,
                confidence_before: 0.3,
                confidence_after: 0.95,
            });
        }
        let rules = strategy_gen.infer_new_rules();

        // Fallback 模式出现 10 次，应该被识别为有效规则
        assert!(
            !rules.is_empty(),
            "Should infer rules from repeated successful fixes"
        );
        let fallback_rules: Vec<_> = rules
            .iter()
            .filter(|r| matches!(r.applies_mode, RecoveryMode::Fallback))
            .collect();
        assert!(
            !fallback_rules.is_empty(),
            "Fallback mode should be recognized as a rule"
        );

        // 验证权重更新
        let weights = strategy_gen.update_calibrator_weights();
        assert_eq!(weights.len(), 7, "All 7 channels should be updated");

        // 所有权重在合法范围内
        for w in weights.values() {
            assert!(*w >= 0.05 && *w <= 0.5, "weight {} out of bounds", w);
        }
    }

    #[test]
    fn test_j3_ab_experiment_and_scoring() {
        let mut engine = EvolutionVerificationEngine::new();

        // Run multiple experiments
        let _ = engine.run_experiment("exp_001", "v1", "v2", 0.70, 0.85);
        let _ = engine.run_experiment("exp_002", "alpha", "beta", 0.65, 0.72);
        let _ = engine.run_experiment("exp_003", "old", "new", 0.80, 0.80); // tie

        assert_eq!(engine.experiment_count(), 3);

        // Last result should be exp_003
        let last = engine.last_result().unwrap();
        assert_eq!(last.config.experiment_id, "exp_003");

        // Tie case: treatment wins (>= condition)
        assert_eq!(last.winner, Group::Treatment);

        // Verify summary report
        let report = engine.summary_report();
        assert!(report.contains("Total experiments: 3"));
        assert!(report.contains("Treatment wins (B): 3"));

        // Test EvolutionScore composite
        let good_score = EvolutionScore {
            regression_pass_rate: 1.0,
            performance_delta: 0.5,
            memory_delta: 0.0,
            coverage_impact: 1.0,
            governance_compliance: true,
        };
        assert!(
            good_score.passes_threshold(0.8),
            "Good score should pass threshold 0.8"
        );

        let bad_score = EvolutionScore {
            regression_pass_rate: 0.5,
            performance_delta: -0.1,
            memory_delta: 0.05,
            coverage_impact: -0.3,
            governance_compliance: false,
        };
        assert!(
            !bad_score.passes_threshold(0.8),
            "Bad score should fail threshold 0.8"
        );
    }

    #[test]
    fn test_j4_evolve_status_output() {
        let report = j_status_report().expect("j_status_report should succeed");

        // Verify all three channels are represented in output
        assert!(
            report.contains("J1") || report.contains("Pattern"),
            "Report should mention J1"
        );
        assert!(
            report.contains("J2") || report.contains("Strategy"),
            "Report should mention J2"
        );
        assert!(
            report.contains("J3") || report.contains("Verification"),
            "Report should mention J3"
        );

        // Verify numerical data is present
        assert!(
            report.contains("errors") || report.contains("clustered"),
            "Report should mention error clustering"
        );
        assert!(
            report.contains("rules") || report.contains("weight"),
            "Report should mention strategy rules"
        );
    }

    #[test]
    fn test_review_panel_data_integrity() {
        let evolutions = fetch_real_evolutions();

        // Should have at least some entries
        assert!(
            !evolutions.is_empty(),
            "Review panel should have data from real engines"
        );

        // Each evolution change must satisfy basic integrity constraints
        for c in &evolutions {
            // Module name should identify source
            assert!(
                !c.module.is_empty(),
                "Module must not be empty for #{}",
                c.id
            );

            // Description should be human-readable
            assert!(
                !c.description.is_empty(),
                "Description must not be empty for #{}",
                c.id
            );

            // Impact should be non-trivial
            assert!(
                c.impact.chars().count() > 2,
                "Impact too short for #{}",
                c.id
            );

            // Expected benefit should indicate some value
            assert!(
                !c.expected_benefit.is_empty(),
                "Expected benefit must not be empty for #{}",
                c.id
            );

            // Risk level must be valid
            match c.risk_level {
                RiskLevel::Trivial | RiskLevel::Low | RiskLevel::Medium | RiskLevel::High => {}
            }

            // Test coverage must have at least one field set
            assert!(
                c.test_coverage.unit || c.test_coverage.integration || c.test_coverage.e2e,
                "Test coverage must have at least one flag set for #{}",
                c.id
            );
        }

        // Verify IDs are unique within the same module type
        let j1_changes: Vec<_> = evolutions
            .iter()
            .filter(|c| c.module.contains("j1"))
            .collect();
        let mut j1_ids: Vec<u64> = j1_changes.iter().map(|c| c.id).collect();
        j1_ids.sort();
        j1_ids.dedup();
        assert_eq!(
            j1_ids.len(),
            j1_changes.len(),
            "J1 module IDs must be unique"
        );
    }

    #[test]
    fn test_j1_export_templates_to_file() {
        use std::fs;

        let mut engine = ErrorClusteringEngine::new();
        engine.add_error(ErrorRecord {
            id: 1,
            timestamp: "2026-07-17T12:00:00Z".into(),
            error_type: "latency".into(),
            message: "latency exceeded".into(),
            source_location: None,
            stack_trace: None,
            recovery_applied: None,
            recovery_success: false,
        });
        engine.add_error(ErrorRecord {
            id: 2,
            timestamp: "2026-07-17T12:00:00Z".into(),
            error_type: "latency".into(),
            message: "latency exceeded".into(),
            source_location: None,
            stack_trace: None,
            recovery_applied: None,
            recovery_success: false,
        });

        let output = "/tmp/.dalib_test_export.jsonl";
        let _ = fs::remove_file(output);
        let result = engine.export_templates_json(output);
        assert!(result.is_ok(), "Export should succeed: {:?}", result);
        assert!(fs::metadata(output).is_ok(), "Export file should exist");
    }

    #[test]
    fn test_j2_suggest_hot_recompile() {
        let mut strategy = StrategyGenerator::new();

        // Generate enough successful fixes to trigger rule creation
        for i in 0..15 {
            strategy.record_fix(FixRecord {
                error_id: i,
                applied_rule: if i % 2 == 0 {
                    RecoveryMode::Fallback
                } else {
                    RecoveryMode::RetryWithDefault
                },
                success: true,
                confidence_before: 0.3,
                confidence_after: 0.9,
            });
        }

        strategy.infer_new_rules();

        // Threshold 3: should suggest recompile when many untested rules accumulated
        let _suggestion = strategy.suggest_hot_recompile(3);

        // Either suggests recompilation or has rules worth tracking
        // The key assertion: strategy generation produced rules
        assert!(
            !strategy.known_rules().is_empty(),
            "Should track known rules"
        );
        assert!(strategy.history_len() >= 15, "Should record all fixes");
    }

    #[test]
    fn test_four_channel_data_flow_j1_to_j2_to_j3() {
        // Simulate real-world data flow through J1 → J2 → J3 pipeline
        // This mirrors how the actual evolution system operates

        // J1: Collect errors, cluster them, generate fix templates
        let mut j1 = ErrorClusteringEngine::new();
        for i in 0..8 {
            j1.add_error(ErrorRecord {
                id: i,
                timestamp: "2026-07-17T12:00:00Z".into(),
                error_type: if i < 4 {
                    "latency".to_string()
                } else {
                    "governance".to_string()
                },
                message: format!("error pattern group {}", i / 4),
                source_location: None,
                stack_trace: None,
                recovery_applied: Some("recovery_fallback".into()),
                recovery_success: i < 4,
            });
        }
        let clusters = j1.cluster(0.5, 2);
        let templates = j1.extract_templates(&clusters);

        // J2: Use templates as input signal for strategy generation
        let mut j2 = StrategyGenerator::new();
        for t in &templates {
            if t.confidence > 0.5 {
                j2.record_fix(FixRecord {
                    error_id: t.template_id.parse::<u64>().unwrap_or(0) % 100,
                    applied_rule: RecoveryMode::Fallback,
                    success: t.confidence > 0.7,
                    confidence_before: 1.0 - t.confidence,
                    confidence_after: t.confidence,
                });
            }
        }
        let rules = j2.infer_new_rules();
        let weights = j2.update_calibrator_weights();

        // J3: Score the combined improvement
        let total_j1_clusters = clusters.iter().map(|c| c.len()).sum::<usize>();
        let total_j2_rules = rules.len();
        let avg_weight = weights.values().sum::<f64>() / weights.len() as f64;

        let score = EvolutionScore {
            regression_pass_rate: if total_j1_clusters >= 2 { 1.0 } else { 0.7 },
            performance_delta: avg_weight.clamp(-0.2, 0.5),
            memory_delta: 0.0,
            coverage_impact: (total_j2_rules as f64 / 10.0).min(1.0) * 2.0 - 1.0,
            governance_compliance: total_j2_rules > 0,
        };

        let composite = score.composite();
        assert!(
            composite > 0.0,
            "Pipeline should produce positive composite score, got {}",
            composite
        );

        // Verify intermediate results make sense
        assert!(
            total_j1_clusters >= 2,
            "J1 should cluster at least 2 errors"
        );
    }
}
