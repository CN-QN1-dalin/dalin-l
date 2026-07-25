//! 高层 Agent API — 构建器模式 + 运行时封装

use crate::error::Result;
use crate::protocol::HandshakeProtocol;
use crate::types::*;
use crate::transport::Transport;

/// 高层 Agent 实例
///
/// 封装握手协议、传输层、会话管理，提供简洁的 Agent API。
///
/// ```rust,ignore
/// use dalin_handshake::prelude::*;
///
/// let mut agent = Agent::builder()
///     .name("my-agent")
///     .transport(Box::new(MemoryTransport::new("my-agent")))
///     .capability("compile", "3.0.0")
///     .build().unwrap();
///
/// agent.start().unwrap();
/// let _ = agent.discover().unwrap();
/// ```
pub struct Agent {
    /// 协议引擎
    protocol: HandshakeProtocol,
    /// 能力列表
    capabilities: Vec<Capability>,
    /// 是否已启动
    started: bool,
}

impl Agent {
    /// 创建 Agent 构建器
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    /// 启动 Agent
    ///
    /// 1. 启动传输层
    /// 2. 广播发现消息
    /// 3. 开始监听
    pub fn start(&mut self) -> Result<()> {
        self.protocol.start()?;
        self.started = true;
        Ok(())
    }

    /// 停止 Agent
    pub fn stop(&mut self) -> Result<()> {
        self.protocol.stop()?;
        self.started = false;
        Ok(())
    }

    /// 发现其他 Agent
    pub fn discover(&self) -> Result<Vec<PeerInfo>> {
        self.protocol.discover()
    }

    /// 向指定 peer 发起握手
    pub fn handshake(&mut self, peer: &PeerInfo) -> Result<Session> {
        self.protocol.handshake(peer)
    }

    /// 发送业务数据
    pub fn send(&self, session: &SessionId, payload: serde_json::Value) -> Result<()> {
        self.protocol.send_data(session, payload, None)
    }

    /// 接收消息（非阻塞）
    pub fn recv(&self) -> Result<Option<Message>> {
        self.protocol.recv()
    }

    /// 处理接收到的消息
    pub fn handle(&mut self, msg: Message) -> Result<Option<Message>> {
        self.protocol.handle_message(msg)
    }

    /// 获取本 Agent 的 ID
    pub fn id(&self) -> &AgentId {
        self.protocol.agent_id()
    }

    /// 获取能力列表
    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    /// 获取活跃会话
    pub fn sessions(&self) -> &std::collections::HashMap<SessionId, Session> {
        self.protocol.sessions()
    }

    /// 是否已启动
    pub fn is_started(&self) -> bool {
        self.started
    }
}

/// Agent 构建器
pub struct AgentBuilder {
    agent_id: Option<AgentId>,
    agent_name: Option<String>,
    agent_version: String,
    language: String,
    transport: Option<Box<dyn Transport>>,
    capabilities: Vec<Capability>,
    auth_token: Option<String>,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            agent_id: None,
            agent_name: None,
            agent_version: "1.0.0".to_string(),
            language: "rust".to_string(),
            transport: None,
            capabilities: Vec::new(),
            auth_token: None,
        }
    }

    /// 设置 Agent ID（默认自动生成 UUID）
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(AgentId::new(id));
        self
    }

    /// 设置 Agent 名称
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    /// 设置 Agent 版本
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.agent_version = version.into();
        self
    }

    /// 设置语言
    pub fn language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// 设置传输层
    pub fn transport(mut self, transport: Box<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// 添加能力声明
    pub fn capability(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.capabilities.push(Capability::new(name, version));
        self
    }

    /// 设置认证 token
    pub fn auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// 构建 Agent 实例
    pub fn build(self) -> Result<Agent> {
        let agent_id = self.agent_id.unwrap_or_else(AgentId::generate);
        let agent_name = self.agent_name.unwrap_or_else(|| "agent".to_string());
        let transport = self.transport.ok_or_else(|| {
            crate::error::HandshakeError::Protocol("Transport is required".to_string())
        })?;

        let mut protocol = HandshakeProtocol::new(
            agent_id,
            agent_name,
            &self.agent_version,
            transport,
        );

        if let Some(token) = self.auth_token {
            protocol = protocol.with_auth(token);
        }

        protocol = protocol.with_language(&self.language);

        Ok(Agent {
            protocol,
            capabilities: self.capabilities,
            started: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MemoryTransport;

    #[test]
    fn test_agent_builder() {
        let agent = Agent::builder()
            .name("test-agent")
            .version("1.0.0")
            .transport(Box::new(MemoryTransport::new("test-agent")))
            .capability("compile", "3.0.0")
            .capability("type_check", "3.0.0")
            .build()
            .expect("Should build agent");

        assert_eq!(agent.capabilities().len(), 2);
        assert!(!agent.is_started());
    }

    #[test]
    fn test_agent_lifecycle() {
        let mut agent = Agent::builder()
            .name("lifecycle-test")
            .transport(Box::new(MemoryTransport::new("lifecycle-test")))
            .build()
            .expect("Should build");

        assert!(!agent.is_started());
        agent.start().expect("Should start");
        assert!(agent.is_started());
        agent.stop().expect("Should stop");
        assert!(!agent.is_started());
    }

    #[test]
    fn test_agent_discover() {
        let mut agent = Agent::builder()
            .name("discover-test")
            .transport(Box::new(MemoryTransport::new("discover-test")))
            .build()
            .expect("Should build");

        agent.start().expect("Should start");
        let peers = agent.discover().expect("Should discover");
        // 没有其他 Agent，应该为空
        assert!(peers.is_empty());
    }
}