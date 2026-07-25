//! 传输层 — Transport trait 与内置实现

use crate::error::Result;
use crate::types::{Message, PeerInfo};

pub mod file;
pub mod unix;
pub mod tcp;

/// 传输层接口
///
/// 所有传输实现必须实现此 trait：
/// - `FileTransport`: 基于共享目录的文件轮询
/// - `UnixTransport`: 基于 Unix Domain Socket（需 tokio）
/// - `TcpTransport`: 基于 TCP 网络（需 tokio）
pub trait Transport: Send + Sync {
    /// 传输方式名称
    fn kind(&self) -> &'static str;

    /// 本端端点地址
    fn endpoint(&self) -> &str;

    /// 启动传输层（开始监听）
    fn start(&mut self) -> Result<()>;

    /// 停止传输层
    fn stop(&mut self) -> Result<()>;

    /// 发送消息到指定端点
    fn send(&self, target: &str, msg: &Message) -> Result<()>;

    /// 接收消息（非阻塞，无消息时返回 None）
    fn recv(&self) -> Result<Option<Message>>;

    /// 广播消息（发现等）
    fn broadcast(&self, msg: &Message) -> Result<()>;

    /// 发现所有在线 Agent
    fn discover(&self) -> Result<Vec<PeerInfo>>;
}

/// 内存传输 — 用于同一进程内多 Agent 通信
#[derive(Debug, Clone)]
pub struct MemoryTransport {
    name: String,
    channel: std::sync::Arc<std::sync::Mutex<Vec<Message>>>,
    peers: std::sync::Arc<std::sync::Mutex<Vec<PeerInfo>>>,
}

impl MemoryTransport {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channel: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            peers: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// 将另一个 MemoryTransport 连接为 peer
    pub fn link(&self, other: &MemoryTransport) {
        let peer_info = PeerInfo {
            agent_id: crate::types::AgentId::new(&self.name),
            agent_name: self.name.clone(),
            agent_version: "1.0.0".to_string(),
            language: "rust".to_string(),
            transport: crate::types::TransportKind::Memory,
            endpoint: self.name.clone(),
            capabilities: Vec::new(),
        };
        {
            let mut peers = other.peers.lock().unwrap();
            peers.push(peer_info);
        }
        let peer_info = PeerInfo {
            agent_id: crate::types::AgentId::new(&other.name),
            agent_name: other.name.clone(),
            agent_version: "1.0.0".to_string(),
            language: "rust".to_string(),
            transport: crate::types::TransportKind::Memory,
            endpoint: other.name.clone(),
            capabilities: Vec::new(),
        };
        {
            let mut peers = self.peers.lock().unwrap();
            peers.push(peer_info);
        }
    }

    /// 将消息推入通道（供其他 Agent 调用）
    pub fn push(&self, msg: Message) {
        let mut channel = self.channel.lock().unwrap();
        channel.push(msg);
    }
}

impl Transport for MemoryTransport {
    fn kind(&self) -> &'static str {
        "memory"
    }

    fn endpoint(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn send(&self, _target: &str, msg: &Message) -> Result<()> {
        // 在内存传输中，send 意味着目标 peer 必须提前 link
        // 简化：将消息存入自己的通道，由外部桥接器转发
        let mut channel = self.channel.lock().unwrap();
        channel.push(msg.clone());
        Ok(())
    }

    fn recv(&self) -> Result<Option<Message>> {
        let mut channel = self.channel.lock().unwrap();
        Ok(channel.pop())
    }

    fn broadcast(&self, msg: &Message) -> Result<()> {
        let mut channel = self.channel.lock().unwrap();
        channel.push(msg.clone());
        Ok(())
    }

    fn discover(&self) -> Result<Vec<PeerInfo>> {
        let peers = self.peers.lock().unwrap();
        Ok(peers.clone())
    }
}