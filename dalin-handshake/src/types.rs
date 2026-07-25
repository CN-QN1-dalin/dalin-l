//! AHP 核心类型定义

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ═════════════════════════════════════════════════════════════════
//  标识符
// ═════════════════════════════════════════════════════════════════

/// Agent 唯一标识符
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 生成随机 Agent ID
    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 会话 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 消息 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl MessageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

// ═════════════════════════════════════════════════════════════════
//  能力声明
// ═════════════════════════════════════════════════════════════════

/// Agent 能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

impl Capability {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: None,
            input_schema: None,
            output_schema: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

// ═════════════════════════════════════════════════════════════════
//  传输层
// ═════════════════════════════════════════════════════════════════

/// 传输方式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransportKind {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "unix")]
    Unix,
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "memory")]
    Memory,
    #[serde(untagged)]
    Other(String),
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::File => write!(f, "file"),
            TransportKind::Unix => write!(f, "unix"),
            TransportKind::Tcp => write!(f, "tcp"),
            TransportKind::Memory => write!(f, "memory"),
            TransportKind::Other(s) => write!(f, "{s}"),
        }
    }
}

// ═════════════════════════════════════════════════════════════════
//  发现消息
// ═════════════════════════════════════════════════════════════════

/// Agent 广播的发现消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    pub protocol: String,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub agent_version: String,
    pub language: String,
    pub transport: TransportKind,
    pub endpoint: String,
    pub capabilities: Vec<String>,
    pub started_at: String,
    pub ttl_seconds: u64,
}

impl Discovery {
    pub fn new(
        agent_id: AgentId,
        agent_name: impl Into<String>,
        agent_version: impl Into<String>,
        transport: TransportKind,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            protocol: "ahp/1.0".to_string(),
            agent_id,
            agent_name: agent_name.into(),
            agent_version: agent_version.into(),
            language: "rust".to_string(),
            transport,
            endpoint: endpoint.into(),
            capabilities: Vec::new(),
            started_at: chrono::Utc::now().to_rfc3339(),
            ttl_seconds: 300,
        }
    }
}

// ═════════════════════════════════════════════════════════════════
//  消息
// ═════════════════════════════════════════════════════════════════

/// 消息类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    #[serde(rename = "discovery")]
    Discovery,
    #[serde(rename = "discovery_ack")]
    DiscoveryAck,
    #[serde(rename = "handshake_req")]
    HandshakeReq,
    #[serde(rename = "handshake_resp")]
    HandshakeResp,
    #[serde(rename = "data")]
    Data,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "disconnect")]
    Disconnect,
}

/// 通用消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    pub version: String,
    #[serde(rename = "from")]
    pub from: AgentId,
    #[serde(rename = "to")]
    pub to: AgentId,
    pub session_id: Option<SessionId>,
    pub timestamp: String,
    pub payload: serde_json::Value,
    pub correlation_id: Option<String>,
}

impl Message {
    #[must_use]
    pub fn new(
        msg_type: MessageType,
        from: AgentId,
        to: AgentId,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: MessageId::generate(),
            msg_type,
            version: "1.0".to_string(),
            from,
            to,
            session_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            correlation_id: None,
        }
    }

    /// 创建 ping 消息
    #[must_use]
    pub fn ping(from: AgentId, to: AgentId, session: SessionId) -> Self {
        let mut msg = Self::new(MessageType::Ping, from, to, serde_json::json!({}));
        msg.session_id = Some(session);
        msg
    }

    /// 创建 pong 消息（响应 ping）
    #[must_use]
    pub fn pong(ping: &Message) -> Self {
        let mut msg = Self::new(
            MessageType::Pong,
            ping.to.clone(),
            ping.from.clone(),
            serde_json::json!({}),
        );
        msg.session_id = ping.session_id.clone();
        msg.correlation_id = Some(ping.id.0.clone());
        msg
    }

    /// 创建 data 消息
    #[must_use]
    pub fn data(
        from: AgentId,
        to: AgentId,
        session: SessionId,
        payload: serde_json::Value,
        correlation_id: Option<String>,
    ) -> Self {
        let mut msg = Self::new(MessageType::Data, from, to, payload);
        msg.session_id = Some(session);
        msg.correlation_id = correlation_id;
        msg
    }
}

// ═════════════════════════════════════════════════════════════════
//  握手状态
// ═════════════════════════════════════════════════════════════════

/// 握手状态
#[derive(Debug, Clone, PartialEq)]
pub enum HandshakeState {
    Init,
    Discovered,
    Connecting,
    Connected,
    Active,
    Closed,
}

impl std::fmt::Display for HandshakeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeState::Init => write!(f, "init"),
            HandshakeState::Discovered => write!(f, "discovered"),
            HandshakeState::Connecting => write!(f, "connecting"),
            HandshakeState::Connected => write!(f, "connected"),
            HandshakeState::Active => write!(f, "active"),
            HandshakeState::Closed => write!(f, "closed"),
        }
    }
}

// ═════════════════════════════════════════════════════════════════
//  会话
// ═════════════════════════════════════════════════════════════════

/// 握手会话
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub peer_id: AgentId,
    pub peer_name: String,
    pub state: HandshakeState,
    pub granted_capabilities: Vec<String>,
    pub keep_alive_interval: Duration,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

impl Session {
    pub fn new(peer_id: AgentId, peer_name: impl Into<String>, capabilities: Vec<String>) -> Self {
        Self {
            id: SessionId::generate(),
            peer_id,
            peer_name: peer_name.into(),
            state: HandshakeState::Connected,
            granted_capabilities: capabilities,
            keep_alive_interval: Duration::from_secs(30),
            created_at: chrono::Utc::now(),
            last_heartbeat: chrono::Utc::now(),
        }
    }

    /// 更新心跳时间
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = chrono::Utc::now();
    }

    /// 检查心跳是否超时
    #[must_use]
    pub fn is_heartbeat_expired(&self, max_missed: u32) -> bool {
        let elapsed = chrono::Utc::now() - self.last_heartbeat;
        elapsed
            > chrono::TimeDelta::from_std(self.keep_alive_interval * max_missed)
                .unwrap_or(chrono::TimeDelta::MAX)
    }
}

/// Agent 发现信息
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub agent_version: String,
    pub language: String,
    pub transport: TransportKind,
    pub endpoint: String,
    pub capabilities: Vec<String>,
}
