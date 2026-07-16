//! Agent Registry — 节点注册 / 心跳 / 能力声明
//!
//! 管理分布式 Worker 节点的生命周期：
//! - 注册时声明能力集合（Cpu/Gpu/Sfa/Net）与效应支持（Pure/Io/Async/Spawn）
//! - 心跳每 15s 刷新，超过 HEARTBEAT_TTL 未心跳 → 节点标记过期
//! - gRPC `AgentRegistry` 服务提供 RegisterNode / Heartbeat / ListNodes
//! - 调度器通过 `fresh_nodes()` 获取在线节点列表

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_core::Stream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::{
    agent_registry_server::AgentRegistry,
    Empty, NodeAck, NodeBeat, NodeSpec,
};
use crate::scheduler::{Capability as SchCap, Node};

/// 心跳超时：超过此时间未心跳的节点视为下线
const HEARTBEAT_TTL: Duration = Duration::from_secs(30);

/// 后台清理过期间隔
const CLEANUP_INTERVAL: Duration = Duration::from_secs(15);

/// 节点运行时信息
#[derive(Debug, Clone)]
struct NodeEntry {
    spec: NodeSpec,
    last_heartbeat: Instant,
}

/// 节点注册表（线程安全，内存实现）
pub struct NodeRegistry {
    nodes: Mutex<HashMap<String, NodeEntry>>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            nodes: Mutex::new(HashMap::new()),
        }
    }

    /// 注册或更新节点。
    /// 返回 (是否新注册, 消息)。
    pub fn register(&self, spec: NodeSpec) -> (bool, String) {
        let mut nodes = self.nodes.lock().unwrap();
        let is_new = !nodes.contains_key(&spec.id);
        nodes.insert(
            spec.id.clone(),
            NodeEntry {
                spec,
                last_heartbeat: Instant::now(),
            },
        );
        let msg = if is_new {
            format!("node registered")
        } else {
            format!("node updated")
        };
        (is_new, msg)
    }

    /// 接收心跳，刷新节点的 last_heartbeat。
    /// 返回是否成功（false = 节点未注册）。
    pub fn heartbeat(&self, beat: &NodeBeat) -> bool {
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(entry) = nodes.get_mut(&beat.id) {
            entry.last_heartbeat = Instant::now();
            // 可选：心跳时更新能力/效应声明
            if beat.capabilities.is_some() {
                entry.spec.capabilities = beat.capabilities.clone();
            }
            if beat.effects_supported.is_some() {
                entry.spec.effects_supported = beat.effects_supported.clone();
            }
            true
        } else {
            false
        }
    }

    /// 获取所有心跳新鲜的节点。
    pub fn fresh_nodes(&self) -> Vec<Node> {
        let now = Instant::now();
        let nodes = self.nodes.lock().unwrap();
        nodes
            .values()
            .filter(|e| now.duration_since(e.last_heartbeat) < HEARTBEAT_TTL)
            .map(|e| node_spec_to_scheduler_node(&e.spec))
            .collect()
    }

    /// 获取所有注册的节点（含过期）。
    pub fn all_nodes(&self) -> Vec<NodeSpec> {
        let nodes = self.nodes.lock().unwrap();
        nodes.values().map(|e| e.spec.clone()).collect()
    }

    /// 清理过期节点。
    pub fn cleanup(&self) -> usize {
        let now = Instant::now();
        let mut nodes = self.nodes.lock().unwrap();
        let before = nodes.len();
        nodes.retain(|_, e| now.duration_since(e.last_heartbeat) < HEARTBEAT_TTL);
        before - nodes.len()
    }
}

/// 把 proto NodeSpec 转为调度器的 Node。
/// 默认配额 4（避免无限堆积），配额在首次 place 时按需调整。
fn node_spec_to_scheduler_node(spec: &NodeSpec) -> Node {
    let mut caps = std::collections::HashSet::new();
    if spec.capabilities.as_ref().map_or(false, |c| c.cpu) {
        caps.insert(SchCap::Cpu);
    }
    if spec.capabilities.as_ref().map_or(false, |c| c.gpu) {
        caps.insert(SchCap::Gpu);
    }
    if spec.capabilities.as_ref().map_or(false, |c| c.sfa) {
        caps.insert(SchCap::Sfa);
    }
    if spec.capabilities.as_ref().map_or(false, |c| c.net) {
        caps.insert(SchCap::Net);
    }
    Node::new(&spec.id, caps)
}

/// 后台清理任务：定期清理过期节点
pub fn spawn_cleanup_task(registry: Arc<NodeRegistry>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CLEANUP_INTERVAL).await;
            let removed = registry.cleanup();
            if removed > 0 {
                eprintln!("[Registry] cleaned up {removed} stale node(s)");
            }
        }
    });
}

// ── gRPC 服务实现 ──

pub struct AgentRegistryService {
    registry: Arc<NodeRegistry>,
}

impl AgentRegistryService {
    pub fn new(registry: Arc<NodeRegistry>) -> Self {
        Self { registry }
    }
}

#[tonic::async_trait]
impl AgentRegistry for AgentRegistryService {
    async fn register_node(
        &self,
        req: Request<NodeSpec>,
    ) -> Result<Response<NodeAck>, Status> {
        let spec = req.into_inner();
        let (is_new, msg) = self.registry.register(spec);
        Ok(Response::new(NodeAck {
            ok: true,
            message: if is_new {
                format!("registered: {msg}")
            } else {
                format!("updated: {msg}")
            },
        }))
    }

    async fn heartbeat(
        &self,
        req: Request<NodeBeat>,
    ) -> Result<Response<Empty>, Status> {
        let beat = req.into_inner();
        let ok = self.registry.heartbeat(&beat);
        if !ok {
            return Err(Status::not_found(format!("unknown node: {}", beat.id)));
        }
        Ok(Response::new(Empty {}))
    }

    type ListNodesStream = Pin<Box<dyn Stream<Item = Result<NodeSpec, Status>> + Send>>;

    async fn list_nodes(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<Self::ListNodesStream>, Status> {
        let nodes = self.registry.all_nodes();
        let stream = tokio_stream::iter(nodes).map(Ok);
        Ok(Response::new(Box::pin(stream)))
    }
}
