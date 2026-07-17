//! PostgresTaskStore — `TaskStore` 的 Postgres 后端（关系型 / 强一致）
//!
//! 数据布局（规范化 + 索引，符合关系型最佳实践）：
//!   - `tasks` 表：id(PK) / name / parent(NULL=根) / effect / capability /
//!     idempotency_key / status / node / submitted_at
//!   - 索引：`idx_tasks_parent`（parent 上的部分索引）
//!   - 幂等：按 `(parent, idempotency_key)` 查询复用既有记录
//!
//! 跨节点事件：当前不支持 Postgres NOTIFY（tokio-postgres 0.7 + PG 18 下
//! 跨连接通知不可达）。本地 `subscribe()` 在同一进程内可靠工作。
//! 多实例分布式场景请使用 RedisTaskStore 或 EtcdTaskStore。

use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_postgres::NoTls;

use crate::registry::{TaskEvent, TaskRecord, TaskStatus};
use crate::store::TaskStore;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent TEXT,
    effect TEXT NOT NULL,
    capability TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL,
    node TEXT,
    submitted_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks (parent) WHERE parent IS NOT NULL;
";

pub struct PostgresTaskStore {
    client: tokio_postgres::Client,
    tx: broadcast::Sender<TaskEvent>,
}

impl PostgresTaskStore {
    /// 连接 Postgres、建表、启动监听任务。
    pub async fn connect(db_url: &str, _node_id: &str) -> Result<Self, tokio_postgres::Error> {
        let (client, connection) = tokio_postgres::connect(db_url, NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("postgres connection error: {e}");
            }
        });
        client.batch_execute(SCHEMA).await?;
        let (tx, _rx) = broadcast::channel(1024);
        Ok(Self { client, tx })
    }

    async fn get_record(&self, id: &str) -> Option<TaskRecord> {
        let rows = self
            .client
            .query(
                "SELECT id,name,parent,effect,capability,idempotency_key,status,node,submitted_at \
                 FROM tasks WHERE id=$1",
                &[&id],
            )
            .await
            .ok()?;
        let row = rows.into_iter().next()?;
        Some(row_to_record(&row))
    }
}

#[async_trait]
impl TaskStore for PostgresTaskStore {
    async fn register(
        &self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
        idempotency_key: &str,
    ) -> TaskRecord {
        let parent_val: Option<String> = parent.map(|s| s.to_string());
        let idem = idempotency_key.to_string();
        // 幂等：同 (parent, idem) 已存在则复用
        if let Ok(rows) = self
            .client
            .query(
                "SELECT id FROM tasks WHERE parent IS NOT DISTINCT FROM $1 AND idempotency_key=$2",
                &[&parent_val, &idem],
            )
            .await
            && let Some(row) = rows.into_iter().next() {
                let existing_id: String = row.get("id");
                if let Some(rec) = self.get_record(&existing_id).await {
                    return rec;
                }
            }

        let id = uuid::Uuid::new_v4().to_string();
        let rec = TaskRecord {
            id: id.clone(),
            name: name.to_string(),
            parent: parent_val,
            effect: effect.to_string(),
            capability: capability.to_string(),
            idempotency_key: idem,
            status: TaskStatus::Queued,
            node: None,
            submitted_at: now_ms(),
        };
        let _ = self
            .client
            .execute(
                "INSERT INTO tasks \
                 (id,name,parent,effect,capability,idempotency_key,status,node,submitted_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[
                    &rec.id,
                    &rec.name,
                    &rec.parent,
                    &rec.effect,
                    &rec.capability,
                    &rec.idempotency_key,
                    &status_to_str(&rec.status),
                    &rec.node,
                    &rec.submitted_at,
                ],
            )
            .await;
        let _ = self.tx.send(TaskEvent::Submitted(rec.clone()));
        rec
    }

    async fn set_status(&self, id: &str, status: TaskStatus) -> Option<TaskRecord> {
        let rec = self.get_record(id).await?;
        let updated = TaskRecord { status, ..rec };
        let _ = self
            .client
            .execute(
                "UPDATE tasks SET status=$1 WHERE id=$2",
                &[&status_to_str(&status), &id],
            )
            .await;
        let _ = self.tx.send(TaskEvent::StatusChanged(updated.clone()));
        Some(updated)
    }

    async fn assign_node(&self, id: &str, node: &str) {
        if let Some(mut rec) = self.get_record(id).await {
            rec.node = Some(node.to_string());
            let _ = self
                .client
                .execute(
                    "UPDATE tasks SET node=$1 WHERE id=$2",
                    &[&rec.node, &id],
                )
                .await;
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        match self.get_record(id).await {
            Some(mut rec) => {
                rec.status = TaskStatus::Canceled;
                let _ = self
                    .client
                    .execute(
                        "UPDATE tasks SET status=$1 WHERE id=$2",
                        &[&status_to_str(&TaskStatus::Canceled), &id],
                    )
                    .await;
                let _ = self.tx.send(TaskEvent::Canceled(id.to_string()));
                true
            }
            None => false,
        }
    }

    async fn get(&self, id: &str) -> Option<TaskRecord> {
        self.get_record(id).await
    }

    async fn children_of(&self, parent: &str) -> Vec<TaskRecord> {
        let rows = self
            .client
            .query(
                "SELECT id,name,parent,effect,capability,idempotency_key,status,node,submitted_at \
                 FROM tasks WHERE parent=$1",
                &[&parent],
            )
            .await
            .ok()
            .unwrap_or_default();
        rows.iter().map(row_to_record).collect()
    }

    async fn list(&self, parent: Option<&str>) -> Vec<TaskRecord> {
        match parent {
            Some(p) => self.children_of(p).await,
            None => {
                let rows = self
                    .client
                    .query(
                        "SELECT id,name,parent,effect,capability,idempotency_key,status,node,submitted_at \
                         FROM tasks",
                        &[],
                    )
                    .await
                    .ok()
                    .unwrap_or_default();
                rows.iter().map(row_to_record).collect()
            }
        }
    }

    async fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }
}

fn row_to_record(row: &tokio_postgres::Row) -> TaskRecord {
    TaskRecord {
        id: row.get("id"),
        name: row.get("name"),
        parent: row.get("parent"),
        effect: row.get("effect"),
        capability: row.get("capability"),
        idempotency_key: row.get("idempotency_key"),
        status: str_to_status(&row.get::<_, String>("status")),
        node: row.get("node"),
        submitted_at: row.get("submitted_at"),
    }
}

fn status_to_str(s: &TaskStatus) -> &'static str {
    match s {
        TaskStatus::Queued => "Queued",
        TaskStatus::Scheduled => "Scheduled",
        TaskStatus::Running => "Running",
        TaskStatus::Succeeded => "Succeeded",
        TaskStatus::Failed => "Failed",
        TaskStatus::Canceled => "Canceled",
    }
}

fn str_to_status(s: &str) -> TaskStatus {
    match s {
        "Scheduled" => TaskStatus::Scheduled,
        "Running" => TaskStatus::Running,
        "Succeeded" => TaskStatus::Succeeded,
        "Failed" => TaskStatus::Failed,
        "Canceled" => TaskStatus::Canceled,
        _ => TaskStatus::Queued,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::exercise_store;

    fn database_url() -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://apple@127.0.0.1:5432/dalin_cp_test".into())
    }

    #[tokio::test]
    #[ignore]
    async fn postgres_store_satisfies_trait() {
        let store = PostgresTaskStore::connect(&database_url(), "node-pg-test")
            .await
            .expect("connect postgres (set DATABASE_URL or run `docker compose up -d`)");
        exercise_store(&store).await;
    }
}
