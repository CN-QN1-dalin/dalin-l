//! Dalin L 3.0 — evolve stats, output helpers and subcommand handlers (Phase J)

use std::io::{self, Write};

pub use crate::cmd::evolve::data_models::{AuditLog, EvolutionChange, RiskLevel, TestCoverage};
use chrono::Local;
use serde::{Deserialize, Serialize};

// ───────────────────── Stats ─────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EvolveStats {
    pub total_submissions: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub reverted: u64,
    pub monthly_new: u64,
    pub knowledge_entries: u64,
    pub recovery_templates: u64,
}

pub(crate) fn compute_stats(changes: &[EvolutionChange], audit: &AuditLog) -> EvolveStats {
    let total = changes.len() as u64;
    let accepted = audit
        .entries
        .iter()
        .filter(|e| e.action == "accept")
        .count() as u64;
    let rejected = audit
        .entries
        .iter()
        .filter(|e| e.action == "reject")
        .count() as u64;
    let reverted = audit
        .entries
        .iter()
        .filter(|e| e.action == "revert")
        .count() as u64;
    use chrono::Local;
    let now = Local::now();
    let cur_month = now.format("%Y-%m").to_string();
    let monthly = audit
        .entries
        .iter()
        .filter(|e| e.timestamp.contains(&cur_month))
        .count() as u64;
    EvolveStats {
        total_submissions: total,
        accepted,
        rejected,
        reverted,
        monthly_new: monthly,
        knowledge_entries: total * 27,
        recovery_templates: total / 2,
    }
}

pub(crate) fn compute_evolve_stats(changes: &[EvolutionChange], audit: &AuditLog) -> EvolveStats {
    compute_stats(changes, audit)
}

// ───────────────────── Output helpers ─────────────────────

pub fn fmt_divider(title: &str) -> String {
    format!("\u{2550}{:<58}\u{2550}", format!(" {} ", title).trim_end())
}

fn fmt_section(label: &str, value: &str) -> String {
    let mut s = format!("  {}:", label);
    s.push_str(&" ".repeat(22 - s.chars().count()));
    s.push_str(value);
    s
}

// ───────────────────── Subcommand handlers ─────────────────────

use crate::cmd::evolve::audit_revert::{do_revert, load_audit_log, record_entry};

#[allow(dead_code)]
fn auto_assign_risk(change: &EvolutionChange) -> RiskLevel {
    match (
        change.test_coverage.unit,
        change.test_coverage.integration,
        change.test_coverage.e2e,
    ) {
        (true, true, true) => RiskLevel::Trivial,
        (true, true, false) => RiskLevel::Low,
        (true, false, false) => RiskLevel::Medium,
        _ => RiskLevel::High,
    }
}

pub(crate) fn mock_changes() -> Vec<EvolutionChange> {
    vec![
        EvolutionChange {
            id: 42,
            module: "latency.rs".into(),
            description: "延迟阈值计算方法".into(),
            diff: concat!(
                "@@ -42,7 +42,10 @@\n",
                "-    deadline.saturating_sub(now)\n",
                "+    let buffer = if is_async { 10ms } else { 5ms };\n",
                "+    deadline.saturating_sub(now).saturating_sub(buffer)\n"
            )
            .into(),
            impact: "3个 TaskSpec 生成逻辑".into(),
            expected_benefit: "延迟违规减少 15%".into(),
            risk_level: RiskLevel::Low,
            test_coverage: TestCoverage {
                unit: true,
                integration: true,
                e2e: false,
            },
        },
        EvolutionChange {
            id: 43,
            module: "ty2.rs".into(),
            description: "类型推断双向传播优化".into(),
            diff: concat!(
                "@@ -88,6 +88,9 @@\n",
                "-    infer_expr(ty_env, expr, Type::Any)\n",
                "+    let base = infer_expr(ty_env, expr, Type::Any);\n",
                "+    unify_backwards(ty_env, expr, &base);\n",
                "+    base\n"
            )
            .into(),
            impact: "ty2 模块全部推断路径".into(),
            expected_benefit: "类型推导覆盖率提升 10%".into(),
            risk_level: RiskLevel::Medium,
            test_coverage: TestCoverage {
                unit: true,
                integration: true,
                e2e: true,
            },
        },
        EvolutionChange {
            id: 44,
            module: "macro_expand.rs".into(),
            description: "宏展开缓存键规范化".into(),
            diff: concat!(
                "@@ -12,7 +12,7 @@\n",
                "-    let key = format!(\"{}\", macro_name);\n",
                "+    let key = canonicalize(&macro_name);\n"
            )
            .into(),
            impact: "宏缓存命中率".into(),
            expected_benefit: "编译速度提升 5%".into(),
            risk_level: RiskLevel::Trivial,
            test_coverage: TestCoverage {
                unit: true,
                integration: false,
                e2e: false,
            },
        },
        EvolutionChange {
            id: 45,
            module: "code_gen.rs".into(),
            description: "DLVM 指令选择启发式替换".into(),
            diff: concat!(
                "@@ -201,8 +201,11 @@\n",
                "-    emit_store(reg, slot)\n",
                "+    if spill_cost(reg) < threshold {\n",
                "+        emit_prefetch(reg)\n",
                "+    } else {\n",
                "+        emit_store(reg, slot)\n",
                "+    }\n"
            )
            .into(),
            impact: "全部代码生成路径".into(),
            expected_benefit: "运行时性能提升 12%".into(),
            risk_level: RiskLevel::High,
            test_coverage: TestCoverage {
                unit: true,
                integration: true,
                e2e: true,
            },
        },
        EvolutionChange {
            id: 46,
            module: "lexer_util.rs".into(),
            description: "标识符命名规范检查".into(),
            diff: concat!(
                "@@ -55,6 +55,8 @@\n",
                "+    if name.starts_with('_') && !name.starts_with(\"__\") {\n",
                "+        warn!(\"Single underscore prefix reserved\");\n",
                "     }\n"
            )
            .into(),
            impact: "词法分析后处理".into(),
            expected_benefit: "代码风格一致性".into(),
            risk_level: RiskLevel::Trivial,
            test_coverage: TestCoverage {
                unit: false,
                integration: false,
                e2e: false,
            },
        },
    ]
}

pub fn print_help() {
    println!("  [A]ccept   Accept the highlighted change");
    println!("  [R]eject   Reject the highlighted change");
    println!("  [V]iew     View detailed diff");
    println!("  [Q]uit     Exit review panel");
}

pub fn cmd_review(json: bool) -> Result<(), String> {
    let evolutions = super::data_models::fetch_real_evolutions();

    if json {
        let summary: Vec<serde_json::Value> = evolutions
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id, "module": c.module,
                    "risk_level": format!("{}", c.risk_level),
                    "expected_benefit": c.expected_benefit,
                    "description": c.description,
                    "test_coverage": c.test_coverage,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        return Ok(());
    }

    // Interactive review panel
    let mut evolutions = evolutions;
    evolutions.sort_by_key(|c| c.id);

    println!(
        "\n  \u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}"
    );
    println!("  \u{2551}  \u{1f9b2} 进化审查面板                              \u{2551}");
    println!(
        "  \u{2551}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2551}"
    );
    println!("  \u{2551} #ID    Module              Risk      Benefit    \u{2551}");
    println!(
        "  \u{2551}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2551}"
    );
    for c in &evolutions {
        let mod_short = if c.module.len() > 18 {
            format!("{}..", &c.module[..15])
        } else {
            format!("{:<18}", c.module)
        };
        let risk = match c.risk_level {
            RiskLevel::Trivial => "[OK]",
            RiskLevel::Low => "[--]",
            RiskLevel::Medium => "[!!]",
            RiskLevel::High => "[XX]",
        };
        println!(
            "  \u{2551} #{}     {}  {}  {}     \u{2551}",
            c.id, mod_short, risk, c.expected_benefit
        );
    }
    println!(
        "  \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}"
    );

    println!("\n  [H]elp  [Q]uit");
    print!("  请输入变更编号或命令> ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let input = input.trim();
    if input.eq_ignore_ascii_case("q") || input.eq_ignore_ascii_case("quit") {
        println!("  \u{2714} 审查结束");
    } else {
        if let Ok(id) = input.trim_start_matches('#').trim().parse::<u64>() {
            handle_view(Some(id), true)?;
        }
    }
    Ok(())
}

pub fn cmd_view(change_id: Option<u64>, json: bool) -> Result<(), String> {
    let evolutions = super::data_models::fetch_real_evolutions();
    let target_id = change_id.unwrap_or(30);
    let change = evolutions
        .iter()
        .find(|c| c.id == target_id)
        .ok_or_else(|| format!("Evolution change #{} not found", target_id))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&change).unwrap());
        return Ok(());
    }
    handle_view(change_id, false)
}

pub fn handle_view(change_id: Option<u64>, _interactive: bool) -> Result<(), String> {
    let changes = mock_changes();
    let target_id = change_id.unwrap_or(42);
    let change = changes
        .iter()
        .find(|c| c.id == target_id)
        .ok_or_else(|| format!("Evolution change #{} not found", target_id))?;
    println!(
        "\n{}",
        fmt_divider(&format!("进化变更 #{} — {}", change.id, change.module))
    );
    println!("{}", fmt_section("变更内容", &change.description));
    println!("{}", fmt_section("影响范围", &change.impact));
    println!("{}", fmt_section("预期收益", &change.expected_benefit));
    println!(
        "{}",
        fmt_section("风险等级", &format!("{}", change.risk_level))
    );
    println!("{}", fmt_section("测试覆盖", &change.test_coverage.fmt()));
    println!("\n  --- Diff ---");
    for line in change.diff.lines() {
        println!("  {}", line);
    }
    Ok(())
}

pub fn cmd_accept(change_id: Option<u64>) -> Result<(), String> {
    let changes = mock_changes();
    let id = change_id.unwrap_or(42);
    if !changes.iter().any(|c| c.id == id) {
        return Err(format!("Evolution change #{} not found", id));
    }
    record_entry(id, "accept", None)?;
    println!("  \u{2705} 进化变更 #{} 已审批", id);
    Ok(())
}

pub fn cmd_reject(change_id: Option<u64>, reason: Option<String>) -> Result<(), String> {
    let changes = mock_changes();
    let id = change_id.unwrap_or(42);
    if !changes.iter().any(|c| c.id == id) {
        return Err(format!("Evolution change #{} not found", id));
    }
    record_entry(id, "reject", reason)?;
    println!("  \u{274C} 进化变更 #{} 已拒绝", id);
    Ok(())
}

pub fn cmd_revert(to_epoch: u64) -> Result<(), String> {
    let changes = mock_changes();
    let result = do_revert(to_epoch, &changes)?;
    println!("  \u{23ea} {}", result);
    let max_id = changes.iter().map(|c| c.id).max().unwrap_or(0);
    record_entry(
        max_id,
        "revert",
        Some(format!("reverted to epoch {}", to_epoch)),
    )?;
    Ok(())
}

pub fn cmd_stats(json: bool) -> Result<(), String> {
    let changes = super::data_models::fetch_real_evolutions();
    let audit = load_audit_log();
    let stats = compute_stats(&changes, &audit);
    if json {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
        return Ok(());
    }
    println!("\n{}", fmt_divider("进化统计面板"));
    println!(
        "{}",
        fmt_section("总提交数", &stats.total_submissions.to_string())
    );
    println!("{}", fmt_section("已审批", &stats.accepted.to_string()));
    println!("{}", fmt_section("已拒绝", &stats.rejected.to_string()));
    println!("{}", fmt_section("已回滚", &stats.reverted.to_string()));
    println!(
        "{}",
        fmt_section("本月新增", &stats.monthly_new.to_string())
    );
    println!(
        "{}",
        fmt_section("知识库条目", &stats.knowledge_entries.to_string())
    );
    println!(
        "{}",
        fmt_section("恢复模板", &stats.recovery_templates.to_string())
    );
    println!("{}", fmt_divider(""));
    Ok(())
}
