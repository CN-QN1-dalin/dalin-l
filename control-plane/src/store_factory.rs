//! 任务存储工厂：按 URL scheme 在运行时选择后端（零改 control-plane 服务层）
//!
//! 支持的 scheme：
//!   - `memory://` 或空           → `InMemoryTaskStore`（默认，单机 / 测试）
//!   - `redis://host:port`        → `RedisTaskStore`
//!   - `etcd://h1:2379,h2:2379`   → `EtcdTaskStore`（逗号分隔多 seed）
//!   - `postgres://user@host/db`  → `PostgresTaskStore`
//!
//! `cpd` 读 `TASK_STORE` 环境变量即可切换后端；`server.rs` 只认 `Arc<dyn TaskStore>`。

use std::sync::Arc;

use thiserror::Error;

use crate::registry::InMemoryTaskStore;
use crate::store::TaskStore;
use crate::store_etcd::EtcdTaskStore;
use crate::store_postgres::PostgresTaskStore;
use crate::store_redis::RedisTaskStore;

#[derive(Debug, Error)]
pub enum StoreFactoryError {
    #[error("未知的存储 scheme: {0}")]
    UnknownScheme(String),
    #[error("Redis 连接失败: {0}")]
    Redis(String),
    #[error("Etcd 连接失败: {0}")]
    Etcd(String),
    #[error("Postgres 连接失败: {0}")]
    Postgres(String),
}

/// 构建任务存储。`node_id` 标识本控制面实例，用于跨节点事件去重。
pub async fn build_task_store(
    url: &str,
    node_id: &str,
) -> Result<Arc<dyn TaskStore>, StoreFactoryError> {
    let url = url.trim();
    if url.is_empty() || url.starts_with("memory://") {
        return Ok(Arc::new(InMemoryTaskStore::new()));
    }
    if url.starts_with("redis://") {
        let s = RedisTaskStore::connect(url, node_id)
            .await
            .map_err(|e| StoreFactoryError::Redis(e.to_string()))?;
        return Ok(Arc::new(s));
    }
    if url.starts_with("postgres://") {
        let s = PostgresTaskStore::connect(url, node_id)
            .await
            .map_err(|e| StoreFactoryError::Postgres(e.to_string()))?;
        return Ok(Arc::new(s));
    }
    if let Some(rest) = url.strip_prefix("etcd://") {
        let seeds: Vec<String> = rest
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let s = EtcdTaskStore::connect(seeds, node_id)
            .await
            .map_err(|e| StoreFactoryError::Etcd(e.to_string()))?;
        return Ok(Arc::new(s));
    }
    Err(StoreFactoryError::UnknownScheme(url.to_string()))
}

/// 简短可读的后端名（用于启动日志）。
pub fn backend_name(url: &str) -> &'static str {
    let url = url.trim();
    if url.is_empty() || url.starts_with("memory://") {
        "memory"
    } else if url.starts_with("redis://") {
        "redis"
    } else if url.starts_with("postgres://") {
        "postgres"
    } else if url.starts_with("etcd://") {
        "etcd"
    } else {
        "unknown"
    }
}
