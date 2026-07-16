//! 控制面 daemon（cpd）
//!
//! 启动 gRPC 控制面服务。关键设计：
//!   - **存储后端可配置**：读 `TASK_STORE` 环境变量（默认 `memory://`），
//!     支持 `redis://host:port` / `etcd://h1:2379,h2:2379`。服务层只认
//!     `Arc<dyn TaskStore>`，切换后端零改 control-plane 代码。
//!   - **节点表动态获取**：从 Agent Registry 的 `fresh_nodes()` 读取，
//!     每 30s 同步一次，不再硬编码。
//!   - **事件总线集群感知**：读 `NATS_URLS`（逗号分隔多 seed，默认
//!     `nats://localhost:4222`），连不上则降级到内存总线，单机也能跑通。
//!   - **派发总线**：NATS（生产）或内存（测试/单机）。

use std::sync::Arc;
use std::time::Duration;

use control_plane::agent_registry::{spawn_cleanup_task, NodeRegistry};
use control_plane::dispatch::InMemoryDispatchBroker;
use control_plane::scheduler::{CapabilityScheduler, Node};
use control_plane::server;
use control_plane::store::TaskStore;
use control_plane::store_factory::{backend_name, build_task_store, StoreFactoryError};
use control_plane::transport::{EventBus, InMemoryEventBus, NatsEventBus};

/// 节点表同步间隔
const NODE_SYNC_INTERVAL: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: std::net::SocketAddr = "127.0.0.1:50051".parse()?;
    let nats_urls =
        std::env::var("NATS_URLS").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let task_store_url =
        std::env::var("TASK_STORE").unwrap_or_else(|_| "memory://".to_string());
    let node_id = std::env::var("NODE_ID")
        .unwrap_or_else(|_| format!("cpd-{}", uuid::Uuid::new_v4()));

    // Agent Registry（节点注册/心跳）
    let node_registry = Arc::new(NodeRegistry::new());
    spawn_cleanup_task(node_registry.clone());

    // 调度器：初始为空，从 Registry 同步
    let scheduler = Arc::new(std::sync::Mutex::new(CapabilityScheduler::new(vec![])));

    // 定期从 Registry 同步节点表到调度器
    let sched = scheduler.clone();
    let reg = node_registry.clone();
    tokio::spawn(async move {
        // 首次同步
        {
            let mut s = sched.lock().unwrap();
            let nodes: Vec<Node> = reg.fresh_nodes();
            if !nodes.is_empty() {
                s.sync_nodes(nodes);
                eprintln!("[cpd] initial sync: {} nodes", s.node_count());
            }
        }
        loop {
            tokio::time::sleep(NODE_SYNC_INTERVAL).await;
            let mut s = sched.lock().unwrap();
            let nodes: Vec<Node> = reg.fresh_nodes();
            s.sync_nodes(nodes);
            eprintln!("[cpd] sync: {} active nodes", s.node_count());
        }
    });

    // 存储后端
    let store: Arc<dyn TaskStore> = match build_task_store(&task_store_url, &node_id).await {
        Ok(s) => s,
        Err(StoreFactoryError::UnknownScheme(s)) => {
            eprintln!("❌ 未知 TASK_STORE scheme：{}", s);
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("❌ 存储后端初始化失败（{}）：{}", backend_name(&task_store_url), e);
            std::process::exit(2);
        }
    };

    // 事件总线
    let bus: Arc<dyn EventBus> = match NatsEventBus::connect(&nats_urls).await {
        Ok(nats) => Arc::new(nats),
        Err(e) => {
            eprintln!("⚠️  NATS 连接失败（{}），降级到内存事件总线", e);
            Arc::new(InMemoryEventBus::new())
        }
    };

    // 派发总线（NATS 或内存）
    let dispatch_broker = Arc::new(InMemoryDispatchBroker::new());

    println!(
        "🌐 控制面 daemon 启动：addr={} node_id={} store={} nats_seeds=[{}]",
        addr, node_id, backend_name(&task_store_url), nats_urls
    );
    server::serve(addr, store, scheduler, bus, node_registry, dispatch_broker).await?;
    Ok(())
}
