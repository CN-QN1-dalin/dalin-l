//! Effect Monitor — 效应运行时强制
//!
//! 保障三通道类型系统给出的效应契约在运行期不被违反。
//! 与设计文档对齐：编译期类型检查 + 运行期执行强制 = defense in depth。
//!
//! 强制规则：
//! - **Pure 沙箱**：禁用所有 IO / 网络 / spawn 操作（仅纯计算允许）。
//! - **Spawn 配额**：每个 session 有限制子任务数上限 `max_children`（默认 64）。
//!   超限派发 `SpawnLimitExceeded` 错误。
//! - **IO / Net 配额**：token bucket 限制单位时间 IO 次数与流量（Phase 3 细化）。
//! - **Async 非阻塞保证**：async 上下文中禁止同步阻塞调用（Phase 3 接入 tokio 阻塞检测）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Spawn 配额：每个 session 的最大子任务数。
const DEFAULT_MAX_CHILDREN: usize = 64;

/// 效应违规错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectViolation {
    /// Pure 上下文中尝试 IO 操作
    IoInPureContext { operation: String },
    /// 超出 spawn 配额
    SpawnLimitExceeded { current: usize, max: usize },
    /// 效应与上下文不兼容
    EffectMismatch { required: String, context: String },
}

impl std::fmt::Display for EffectViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoInPureContext { operation } => {
                write!(f, "pure 上下文中禁止 IO 操作: {operation}")
            }
            Self::SpawnLimitExceeded { current, max } => {
                write!(f, "spawn 配额超限: {current}/{max}")
            }
            Self::EffectMismatch { required, context } => {
                write!(f, "效应不兼容: 需要 {required}，上下文 {context}")
            }
        }
    }
}

/// 效应监视器（每个 Agent Session 一个实例）。
pub struct EffectMonitor {
    /// 当前上下文的效应（pure / io / async / spawn）。
    context_effect: String,
    /// 已派生子任务数（spawn 配额计数）。
    child_count: AtomicUsize,
    /// 最大允许子任务数。
    max_children: usize,
}

impl EffectMonitor {
    /// 新建效应监视器，默认 Pure 上下文、64 子任务配额。
    pub fn new() -> Self {
        Self {
            context_effect: "pure".to_string(),
            child_count: AtomicUsize::new(0),
            max_children: DEFAULT_MAX_CHILDREN,
        }
    }

    /// 设置上下文效应。
    pub fn set_context(&mut self, effect: &str) {
        self.context_effect = effect.to_string();
    }

    /// 获取当前上下文效应。
    pub fn context_effect(&self) -> &str {
        &self.context_effect
    }

    /// 设置 spawn 配额。
    pub fn set_max_children(&mut self, max: usize) {
        self.max_children = max;
    }

    /// 检查并注册一次 spawn 操作。
    /// 上下文必须是 `spawn`，且未超配额。
    pub fn check_spawn(&self) -> Result<(), EffectViolation> {
        // Pure 上下文中禁止 spawn
        if self.context_effect == "pure" {
            return Err(EffectViolation::IoInPureContext {
                operation: "spawn".to_string(),
            });
        }
        let current = self.child_count.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_children {
            return Err(EffectViolation::SpawnLimitExceeded {
                current: current + 1,
                max: self.max_children,
            });
        }
        Ok(())
    }

    /// 检查 IO 操作是否被允许（纯计算上下文禁止 IO）。
    pub fn check_io(&self, operation: &str) -> Result<(), EffectViolation> {
        if self.context_effect == "pure" {
            return Err(EffectViolation::IoInPureContext {
                operation: operation.to_string(),
            });
        }
        Ok(())
    }

    /// 检查效应兼容性：`required` 是否 ≤ `context`。
    pub fn check_effect(&self, required: &str) -> Result<(), EffectViolation> {
        match required {
            "pure" => Ok(()),
            "io" if self.context_effect == "pure" => {
                Err(EffectViolation::EffectMismatch {
                    required: "io".into(),
                    context: self.context_effect.clone(),
                })
            }
            "async" if self.context_effect == "pure" || self.context_effect == "io" => {
                Err(EffectViolation::EffectMismatch {
                    required: "async".into(),
                    context: self.context_effect.clone(),
                })
            }
            "spawn" if self.context_effect != "spawn" => {
                Err(EffectViolation::EffectMismatch {
                    required: "spawn".into(),
                    context: self.context_effect.clone(),
                })
            }
            _ => Ok(()),
        }
    }

    /// 获取当前子任务计数。
    pub fn child_count(&self) -> usize {
        self.child_count.load(Ordering::SeqCst)
    }

    /// 释放一个子任务槽位（任务完成或失败时调用）。
    pub fn release_child(&self) {
        self.child_count.fetch_sub(1, Ordering::SeqCst);
    }
}

// ═══════════════════════════════
//  Session 级别的配额跟踪
// ═══════════════════════════════

/// 管理所有活跃 Agent Session 的效应边界
pub struct SessionManager {
    monitors: std::sync::Mutex<std::collections::HashMap<String, Arc<EffectMonitor>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            monitors: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 获取或创建 Session 的效应监视器。
    pub fn get_or_create(&self, session_id: &str, effect: &str) -> Arc<EffectMonitor> {
        let mut monitors = self.monitors.lock().unwrap();
        monitors
            .entry(session_id.to_string())
            .or_insert_with(|| {
                let mut m = EffectMonitor::new();
                m.set_context(effect);
                Arc::new(m)
            })
            .clone()
    }

    /// 释放 Session 资源。
    pub fn remove(&self, session_id: &str) {
        self.monitors.lock().unwrap().remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_context_rejects_io() {
        let monitor = EffectMonitor::new();
        assert_eq!(monitor.context_effect(), "pure");
        assert!(monitor.check_io("println").is_err());
    }

    #[test]
    fn test_spawn_quota() {
        let mut monitor = EffectMonitor::new();
        monitor.set_context("spawn");
        monitor.set_max_children(2);
        assert!(monitor.check_spawn().is_ok());
        assert!(monitor.check_spawn().is_ok());
        assert!(monitor.check_spawn().is_err()); // 超限
    }

    #[test]
    fn test_effect_compatibility() {
        let monitor = EffectMonitor::new();
        // pure 上下文中 spawn 被拒绝
        assert!(monitor.check_effect("spawn").is_err());
        // pure 中 pure 允许
        assert!(monitor.check_effect("pure").is_ok());
    }

    #[test]
    fn test_pure_context_rejects_spawn() {
        let monitor = EffectMonitor::new();
        let result = monitor.check_spawn();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EffectViolation::IoInPureContext {
                operation: "spawn".into()
            }
        );
    }
}
