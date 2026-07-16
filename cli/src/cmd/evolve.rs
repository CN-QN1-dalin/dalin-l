/// Dalin L 2.0 — evolve CLI (Phase J Human Review Interface)
///
/// Subcommands: review, view, accept, reject, revert, stats
/// Audit log: ~/.dalan/audit_log/{YYYY-MM}/{action}_{id}.json

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// ───────────────────────────── Data structures ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    TRIVIAL,
    LOW,
    MEDIUM,
    HIGH,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::TRIVIAL => write!(f, "TRIVIAL"),
            RiskLevel::LOW => write!(f, "LOW"),
            RiskLevel::MEDIUM => write!(f, "MEDIUM"),
            RiskLevel::HIGH => write!(f, "HIGH"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCoverage {
    pub unit: bool,
    pub integration: bool,
    pub e2e: bool,
}

impl TestCoverage {
    fn fmt(&self) -> String {
        let ui = if self.unit { "\u{2705}" } else { "\u{274C}" };
        let ii = if self.integration { "\u{2705}" } else { "\u{274C}" };
        let ei = if self.e2e { "\u{2705}" } else { "\u{26a0}\u{fe0f}" };
        format!("Unit {} Integration {} E2E {}", ui, ii, ei)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionChange {
    pub id: u64,
    pub module: String,
    pub description: String,
    pub diff: String,
    pub impact: String,
    pub expected_benefit: String,
    pub risk_level: RiskLevel,
    pub test_coverage: TestCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub change_id: u64,
    pub action: String,
    pub reason: Option<String>,
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub entries: Vec<AuditEntry>,
}

// ───────────────────── Mock data ─────────────────────

fn mock_changes() -> Vec<EvolutionChange> {
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
            ).into(),
            impact: "3个 TaskSpec 生成逻辑".into(),
            expected_benefit: "延迟违规减少 15%".into(),
            risk_level: RiskLevel::LOW,
            test_coverage: TestCoverage { unit: true, integration: true, e2e: false },
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
            ).into(),
            impact: "ty2 模块全部推断路径".into(),
            expected_benefit: "类型推导覆盖率提升 10%".into(),
            risk_level: RiskLevel::MEDIUM,
            test_coverage: TestCoverage { unit: true, integration: true, e2e: true },
        },
        EvolutionChange {
            id: 44,
            module: "macro_expand.rs".into(),
            description: "宏展开缓存键规范化".into(),
            diff: concat!(
                "@@ -12,7 +12,7 @@\n",
                "-    let key = format!(\"{}\", macro_name);\n",
                "+    let key = canonicalize(&macro_name);\n"
            ).into(),
            impact: "宏缓存命中率".into(),
            expected_benefit: "编译速度提升 5%".into(),
            risk_level: RiskLevel::TRIVIAL,
            test_coverage: TestCoverage { unit: true, integration: false, e2e: false },
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
            ).into(),
            impact: "全部代码生成路径".into(),
            expected_benefit: "运行时性能提升 12%".into(),
            risk_level: RiskLevel::HIGH,
            test_coverage: TestCoverage { unit: true, integration: true, e2e: true },
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
            ).into(),
            impact: "词法分析后处理".into(),
            expected_benefit: "代码风格一致性".into(),
            risk_level: RiskLevel::TRIVIAL,
            test_coverage: TestCoverage { unit: false, integration: false, e2e: false },
        },
    ]
}

// ───────────────────── Audit log helpers ─────────────────────

fn audit_log_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".dalan/audit_log")
}

fn ensure_audit_dir() -> Result<(), String> {
    let dir = audit_log_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create audit dir: {}", e))?;
    Ok(())
}

fn load_audit_log() -> AuditLog {
    let path = audit_log_dir().join("_index.json");
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(log) = serde_json::from_str(&content) {
                return log;
            }
        }
    }
    AuditLog { entries: vec![] }
}

fn save_audit_log(log: &AuditLog) -> Result<(), String> {
    fs::create_dir_all(&audit_log_dir()).map_err(|e| format!("Cannot create dir: {}", e))?;
    let path = audit_log_dir().join("_index.json");
    let content = serde_json::to_string_pretty(log).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(path, content).map_err(|e| format!("Cannot write file: {}", e))?;
    Ok(())
}

fn record_entry(change_id: u64, action: &str, reason: Option<String>) -> Result<PathBuf, String> {
    ensure_audit_dir()?;
    let log = load_audit_log();
    let entry = AuditEntry {
        timestamp: Local::now().to_rfc3339(),
        change_id,
        action: action.to_string(),
        reason,
        user: "dalib-cli".into(),
    };
    // Write individual file
    let now = Local::now();
    let day = now.format("%d").to_string();
    let ym = now.format("%Y-%m").to_string();
    let padded = format!("{:03}", change_id);
    let file_path = audit_log_dir().join(&ym).join(format!("{}_{}_{}.json", day, action, padded));
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir: {}", e))?;
    }
    let content = serde_json::to_string_pretty(&entry).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(&file_path, content).map_err(|e| format!("Cannot write file: {}", e))?;

    let mut updated = AuditLog { entries: log.entries };
    updated.entries.push(entry);
    save_audit_log(&updated)?;
    Ok(file_path)
}

// ───────────────────── Revert logic ─────────────────────

fn get_snapshot_base() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".dalan/evolution")
}

fn do_revert(target_epoch: u64, current: &[EvolutionChange]) -> Result<String, String> {
    ensure_audit_dir()?;
    let snap_dir = get_snapshot_base();
    fs::create_dir_all(&snap_dir).map_err(|e| format!("Cannot create dir: {}", e))?;
    let max_id = current.iter().map(|c| c.id).max().unwrap_or(0);
    let snapshot_name = format!("snapshot-epoch-{}", max_id);
    let snapshot_path = snap_dir.join(&snapshot_name);
    fs::create_dir_all(&snapshot_path).map_err(|e| format!("Cannot create snapshot: {}", e))?;

    let applied_count = current.iter().filter(|c| c.id > target_epoch).count();
    let changes_json = serde_json::to_string_pretty(
        &current.iter().filter(|c| c.id <= target_epoch).cloned().collect::<Vec<_>>()
    ).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(snapshot_path.join("baseline.json"), changes_json)
        .map_err(|e| format!("Cannot write snapshot: {}", e))?;

    Ok(format!(
        "已回滚至 epoch {}\n撤销 {} 个进化变更\nSnapshot 已保存到 evolution/{}",
        target_epoch, applied_count, &snapshot_name
    ))
}

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

fn compute_stats(changes: &[EvolutionChange], audit: &AuditLog) -> EvolveStats {
    let total = changes.len() as u64;
    let accepted = audit.entries.iter().filter(|e| e.action == "accept").count() as u64;
    let rejected = audit.entries.iter().filter(|e| e.action == "reject").count() as u64;
    let reverted = audit.entries.iter().filter(|e| e.action == "revert").count() as u64;
    let now = Local::now();
    let cur_month = now.format("%Y-%m").to_string();
    let monthly = audit.entries.iter()
        .filter(|e| e.timestamp.contains(&cur_month)).count() as u64;
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

fn auto_assign_risk(change: &EvolutionChange) -> RiskLevel {
    match (change.test_coverage.unit, change.test_coverage.integration, change.test_coverage.e2e) {
        (true, true, true) => RiskLevel::TRIVIAL,
        (true, true, false) => RiskLevel::LOW,
        (true, false, false) => RiskLevel::MEDIUM,
        _ => RiskLevel::HIGH,
    }
}

// ───────────────────── Output helpers ─────────────────────

fn fmt_divider(title: &str) -> String {
    format!("\u{2550}{:<58}\u{2550}", format!(" {} ", title).trim_end())
}

fn fmt_section(label: &str, value: &str) -> String {
    let mut s = format!("  {}:", label);
    s.push_str(&" ".repeat(22 - s.chars().count()));
    s.push_str(value);
    s
}

fn print_help() {
    println!("  [A]ccept   Accept the highlighted change");
    println!("  [R]eject   Reject the highlighted change");
    println!("  [V]iew     View detailed diff");
    println!("  [Q]uit     Exit review panel");
}

// ───────────────────── Subcommand handlers ─────────────────────

fn cmd_review(json: bool) -> Result<(), String> {
    let changes = mock_changes();

    if json {
        let summary: Vec<serde_json::Value> = changes.iter().map(|c| {
            serde_json::json!({
                "id": c.id, "module": c.module,
                "risk_level": format!("{}", c.risk_level),
                "expected_benefit": c.expected_benefit,
                "description": c.description,
                "test_coverage": c.test_coverage,
            })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        return Ok(());
    }

    // Interactive review panel
    let mut changes = changes;
    changes.sort_by_key(|c| c.id);

    println!("\n  \u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}");
    println!("  \u{2551}  \u{1f9b2} 进化审查面板                              \u{2551}");
    println!("  \u{2551}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2551}");
    println!("  \u{2551} #ID    Module              Risk      Benefit    \u{2551}");
    println!("  \u{2551}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2551}");
    for c in &changes {
        let mod_short = if c.module.len() > 18 {
            format!("{}..", &c.module[..15])
        } else {
            format!("{:<18}", c.module)
        };
        let risk = match c.risk_level {
            RiskLevel::TRIVIAL => "[OK]",
            RiskLevel::LOW => "[--]",
            RiskLevel::MEDIUM => "[!!]",
            RiskLevel::HIGH => "[XX]",
        };
        println!("  \u{2551} #{}     {}  {}  {}     \u{2551}", c.id, mod_short, risk, c.expected_benefit);
    }
    println!("  \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}");

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
            handle_view(id, true)?;
        }
    }
    Ok(())
}

fn cmd_view(change_id: Option<u64>, json: bool) -> Result<(), String> {
    let changes = mock_changes();
    let target_id = change_id.unwrap_or(42);
    let change = changes.iter().find(|c| c.id == target_id)
        .ok_or_else(|| format!("Evolution change #{} not found", target_id))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&change).unwrap());
        return Ok(());
    }
    handle_view(change_id.unwrap_or(42), false)
}

fn handle_view(change_id: u64, _interactive: bool) -> Result<(), String> {
    let changes = mock_changes();
    let change = changes.iter().find(|c| c.id == change_id)
        .ok_or_else(|| format!("Evolution change #{} not found", change_id))?;
    println!("\n{}", fmt_divider(&format!("进化变更 #{} — {}", change.id, change.module)));
    println!("{}", fmt_section("变更内容", &change.description));
    println!("{}", fmt_section("影响范围", &change.impact));
    println!("{}", fmt_section("预期收益", &change.expected_benefit));
    println!("{}", fmt_section("风险等级", &format!("{}", change.risk_level)));
    println!("{}", fmt_section("测试覆盖", &change.test_coverage.fmt()));
    println!("\n  --- Diff ---");
    for line in change.diff.lines() {
        println!("  {}", line);
    }
    Ok(())
}

fn cmd_accept(change_id: Option<u64>) -> Result<(), String> {
    let changes = mock_changes();
    let id = change_id.unwrap_or(42);
    if !changes.iter().any(|c| c.id == id) {
        return Err(format!("Evolution change #{} not found", id));
    }
    record_entry(id, "accept", None)?;
    println!("  \u{2705} 进化变更 #{} 已审批", id);
    Ok(())
}

fn cmd_reject(change_id: Option<u64>, reason: Option<String>) -> Result<(), String> {
    let changes = mock_changes();
    let id = change_id.unwrap_or(42);
    if !changes.iter().any(|c| c.id == id) {
        return Err(format!("Evolution change #{} not found", id));
    }
    record_entry(id, "reject", reason)?;
    println!("  \u{274C} 进化变更 #{} 已拒绝", id);
    Ok(())
}

fn cmd_revert(to_epoch: u64) -> Result<(), String> {
    let changes = mock_changes();
    let result = do_revert(to_epoch, &changes)?;
    println!("  \u{23ea} {}", result);
    let max_id = changes.iter().map(|c| c.id).max().unwrap_or(0);
    record_entry(max_id, "revert", Some(format!("reverted to epoch {}", to_epoch)))?;
    Ok(())
}

fn cmd_stats(json: bool) -> Result<(), String> {
    let changes = mock_changes();
    let audit = load_audit_log();
    let stats = compute_stats(&changes, &audit);
    if json {
        println!("{}", serde_json::to_string_pretty(&stats).unwrap());
        return Ok(());
    }
    println!("\n{}", fmt_divider("进化统计面板"));
    println!("{}", fmt_section("总提交数", &stats.total_submissions.to_string()));
    println!("{}", fmt_section("已审批", &stats.accepted.to_string()));
    println!("{}", fmt_section("已拒绝", &stats.rejected.to_string()));
    println!("{}", fmt_section("已回滚", &stats.reverted.to_string()));
    println!("{}", fmt_section("本月新增", &stats.monthly_new.to_string()));
    println!("{}", fmt_section("知识库条目", &stats.knowledge_entries.to_string()));
    println!("{}", fmt_section("恢复模板", &stats.recovery_templates.to_string()));
    println!("{}", fmt_divider(""));
    Ok(())
}

// Public API entry point
pub fn run(subcmd: &str, args: &HashMap<String, String>) -> Result<(), String> {
    match subcmd {
        "review" => cmd_review(false),
        "view" => {
            let jf = args.get("json").map(|v| v == "true").unwrap_or(false);
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            cmd_view(id, jf)
        }
        "accept" => {
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            cmd_accept(id)
        }
        "reject" => {
            let id = args.get("id").and_then(|s| s.parse::<u64>().ok());
            let reason = args.get("reason").cloned();
            cmd_reject(id, reason)
        }
        "revert" => {
            let to = args.get("to")
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| "revert requires --to=<epoch>".to_string())?;
            cmd_revert(to)
        }
        "stats" => {
            let jf = args.get("json").map(|v| v == "true").unwrap_or(false);
            cmd_stats(jf)
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
    use super::*;
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
        let read_entry: AuditEntry = serde_json::from_str(&fs::read_to_string(&log_path).unwrap()).unwrap();
        assert_eq!(read_entry.change_id, 99);
        assert_eq!(read_entry.action, "accept");
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_evolution_change_serialization() {
        let c = EvolutionChange {
            id: 1, module: "t.rs".into(), description: "d".into(),
            diff: "diff".into(), impact: "all".into(), expected_benefit: "+10%".into(),
            risk_level: RiskLevel::LOW,
            test_coverage: TestCoverage { unit: true, integration: false, e2e: false },
        };
        let s = serde_json::to_string(&c).expect("serialize");
        let d: EvolutionChange = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(d.id, 1);
        assert_eq!(d.module, "t.rs");
        assert_eq!(d.risk_level, RiskLevel::LOW);
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
        let mut log = AuditLog { entries: vec![] };
        log.entries.push(AuditEntry { timestamp: "2026-07-15T10:00:00+08:00".into(), change_id: 1, action: "accept".into(), reason: None, user: "test".into() });
        log.entries.push(AuditEntry { timestamp: "2026-07-15T11:00:00+08:00".into(), change_id: 2, action: "reject".into(), reason: Some("too risky".into()), user: "test".into() });
        let stats = compute_stats(&changes, &log);
        assert_eq!(stats.total_submissions, 5);
        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.knowledge_entries, 135);
    }

    #[test]
    fn test_risk_level_auto_assign() {
        let c_full = EvolutionChange {
            id: 1, module: "a".into(), description: "d".into(), diff: "d".into(),
            impact: "i".into(), expected_benefit: "b".into(),
            risk_level: RiskLevel::LOW,
            test_coverage: TestCoverage { unit: true, integration: true, e2e: true },
        };
        assert_eq!(auto_assign_risk(&c_full), RiskLevel::TRIVIAL);

        let c_partial = EvolutionChange {
            id: 2, module: "a".into(), description: "d".into(), diff: "d".into(),
            impact: "i".into(), expected_benefit: "b".into(),
            risk_level: RiskLevel::LOW,
            test_coverage: TestCoverage { unit: true, integration: true, e2e: false },
        };
        assert_eq!(auto_assign_risk(&c_partial), RiskLevel::LOW);

        let c_unit_only = EvolutionChange {
            id: 3, module: "a".into(), description: "d".into(), diff: "d".into(),
            impact: "i".into(), expected_benefit: "b".into(),
            risk_level: RiskLevel::MEDIUM,
            test_coverage: TestCoverage { unit: true, integration: false, e2e: false },
        };
        assert_eq!(auto_assign_risk(&c_unit_only), RiskLevel::MEDIUM);
    }
}
