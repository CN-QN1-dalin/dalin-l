//! Dalin L 3.0 — evolve audit log helpers and revert logic (Phase J)

use std::fs;
use std::path::PathBuf;

use crate::cmd::evolve::data_models::{AuditEntry, AuditLog, EvolutionChange};
use chrono::Local;

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

pub(crate) fn load_audit_log() -> AuditLog {
    let path = audit_log_dir().join("_index.json");
    if let Ok(content) = fs::read_to_string(&path)
        && let Ok(log) = serde_json::from_str(&content)
    {
        return log;
    }
    AuditLog { entries: vec![] }
}

pub(crate) fn save_audit_log(log: &AuditLog) -> Result<(), String> {
    fs::create_dir_all(audit_log_dir()).map_err(|e| format!("Cannot create dir: {}", e))?;
    let path = audit_log_dir().join("_index.json");
    let content = serde_json::to_string_pretty(log).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(path, content).map_err(|e| format!("Cannot write file: {}", e))?;
    Ok(())
}

pub(crate) fn record_entry(change_id: u64, action: &str, reason: Option<String>) -> Result<PathBuf, String> {
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
    let file_path = audit_log_dir()
        .join(&ym)
        .join(format!("{}_{}_{}.json", day, action, padded));
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir: {}", e))?;
    }
    let content = serde_json::to_string_pretty(&entry).map_err(|e| format!("JSON error: {}", e))?;
    fs::write(&file_path, content).map_err(|e| format!("Cannot write file: {}", e))?;

    let mut updated = AuditLog {
        entries: log.entries,
    };
    updated.entries.push(entry);
    save_audit_log(&updated)?;
    Ok(file_path)
}

// ───────────────────── Revert logic ─────────────────────

fn get_snapshot_base() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".dalan/evolution")
}

pub(crate) fn do_revert(target_epoch: u64, current: &[EvolutionChange]) -> Result<String, String> {
    ensure_audit_dir()?;
    let snap_dir = get_snapshot_base();
    fs::create_dir_all(&snap_dir).map_err(|e| format!("Cannot create dir: {}", e))?;
    let max_id = current.iter().map(|c| c.id).max().unwrap_or(0);
    let snapshot_name = format!("snapshot-epoch-{}", max_id);
    let snapshot_path = snap_dir.join(&snapshot_name);
    fs::create_dir_all(&snapshot_path).map_err(|e| format!("Cannot create snapshot: {}", e))?;

    let applied_count = current.iter().filter(|c| c.id > target_epoch).count();
    let changes_json = serde_json::to_string_pretty(
        &current
            .iter()
            .filter(|c| c.id <= target_epoch)
            .cloned()
            .collect::<Vec<_>>(),
    )
    .map_err(|e| format!("JSON error: {}", e))?;
    fs::write(snapshot_path.join("baseline.json"), changes_json)
        .map_err(|e| format!("Cannot write snapshot: {}", e))?;

    Ok(format!(
        "已回滚至 epoch {}\n撤销 {} 个进化变更\nSnapshot 已保存到 evolution/{}",
        target_epoch, applied_count, &snapshot_name
    ))
}
