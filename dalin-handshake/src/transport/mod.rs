//! 传输层 — Transport trait 与内置实现

use crate::error::Result;
use crate::types::{Message, PeerInfo};

pub mod file;
pub mod tcp;
pub mod unix;

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

    /// 将另一个 `MemoryTransport` 连接为 peer
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentId, MessageType};

    #[test]
    fn test_memory_transport_create() {
        let transport = MemoryTransport::new("test-agent");
        assert_eq!(transport.kind(), "memory");
        assert_eq!(transport.endpoint(), "test-agent");
    }

    #[test]
    fn test_memory_transport_start_stop() {
        let mut transport = MemoryTransport::new("start-stop-agent");
        transport.start().expect("start should succeed");
        transport.stop().expect("stop should succeed");
    }

    #[test]
    fn test_memory_transport_send_recv() {
        let transport = MemoryTransport::new("test-agent");
        let msg = Message::new(
            MessageType::Ping,
            AgentId::new("alice"),
            AgentId::new("bob"),
            serde_json::json!({"data": 42}),
        );
        transport.send("bob", &msg).expect("send should succeed");

        let recv = transport.recv().expect("recv should succeed");
        assert!(recv.is_some(), "should receive the message we just sent");
        if let Some(received) = recv {
            assert_eq!(received.msg_type, MessageType::Ping);
            assert_eq!(received.payload["data"], 42);
        }
    }

    #[test]
    fn test_memory_transport_recv_empty() {
        let transport = MemoryTransport::new("empty-agent");
        let result = transport.recv().expect("recv on empty should not error");
        assert!(result.is_none(), "no messages in empty transport");
    }

    #[test]
    fn test_memory_transport_broadcast() {
        let transport = MemoryTransport::new("broadcaster");
        let msg = Message::new(
            MessageType::Data,
            AgentId::new("broadcaster"),
            AgentId::new("*"),
            serde_json::json!({"broadcast": true}),
        );
        transport.broadcast(&msg).expect("broadcast should succeed");

        let recv = transport.recv().expect("recv should succeed");
        assert!(recv.is_some(), "should receive broadcast");
    }

    #[test]
    fn test_memory_transport_link_and_discover() {
        let alice = MemoryTransport::new("alice");
        let bob = MemoryTransport::new("bob");

        alice.link(&bob);

        let peers = alice.discover().expect("discover should succeed");
        assert_eq!(peers.len(), 1, "alice should see bob");
        assert_eq!(peers[0].agent_name, "bob");
    }

    #[test]
    fn test_memory_transport_link_bidirectional() {
        let alice = MemoryTransport::new("alice");
        let bob = MemoryTransport::new("bob");

        alice.link(&bob);

        let alice_peers = alice.discover().expect("alice discover");
        let bob_peers = bob.discover().expect("bob discover");
        assert_eq!(alice_peers.len(), 1, "alice sees bob");
        assert_eq!(bob_peers.len(), 1, "bob sees alice");
        assert_eq!(alice_peers[0].agent_name, "bob");
        assert_eq!(bob_peers[0].agent_name, "alice");
    }

    #[test]
    fn test_memory_transport_push_and_recv() {
        let transport = MemoryTransport::new("receiver");
        let msg = Message::new(
            MessageType::Data,
            AgentId::new("sender"),
            AgentId::new("receiver"),
            serde_json::json!({"pushed": true}),
        );
        transport.push(msg);

        let recv = transport.recv().expect("recv should succeed");
        assert!(recv.is_some(), "should receive pushed message");
        if let Some(received) = recv {
            assert_eq!(received.payload["pushed"], true);
        }
    }

    #[test]
    fn test_two_memory_transports_linked_communication() {
        let alice = MemoryTransport::new("alice");
        let bob = MemoryTransport::new("bob");

        alice.link(&bob);

        // Alice sends a Data message to Bob
        let data_msg = Message::new(
            MessageType::Data,
            AgentId::new("alice"),
            AgentId::new("bob"),
            serde_json::json!({"msg": "hello from alice", "value": 99}),
        );
        bob.push(data_msg);

        // Bob should receive the message sent to its inbox
        let recv = bob.recv().expect("bob recv");
        assert!(recv.is_some(), "bob should receive alice's message");
        if let Some(received) = recv {
            assert_eq!(received.payload["msg"], "hello from alice");
            assert_eq!(received.payload["value"], 99);
        }
    }
}
