//! 握手协议层 — 发现、认证、会话管理、心跳

use crate::error::{HandshakeError, Result};
use crate::transport::Transport;
use crate::types::{AgentId, HandshakeState, SessionId, Session, Message, MessageType, PeerInfo};
use std::collections::HashMap;
use std::time::Duration;

/// 握手协议引擎
///
/// 管理 Agent 的发现、握手、会话和心跳生命周期。
pub struct HandshakeProtocol {
    /// 本 Agent 的 ID
    agent_id: AgentId,
    /// 本 Agent 名称
    agent_name: String,
    /// 本 Agent 版本
    agent_version: String,
    /// 传输层
    transport: Box<dyn Transport>,
    /// 当前状态
    state: HandshakeState,
    /// 活跃会话
    sessions: HashMap<SessionId, Session>,
    /// 通信语言
    language: String,
    /// 心跳间隔
    heartbeat_interval: Duration,
    /// 认证 token（可选）
    auth_token: Option<String>,
}

impl HandshakeProtocol {
    /// 创建握手协议引擎
    pub fn new(
        agent_id: AgentId,
        agent_name: impl Into<String>,
        agent_version: impl Into<String>,
        transport: Box<dyn Transport>,
    ) -> Self {
        Self {
            agent_id,
            agent_name: agent_name.into(),
            agent_version: agent_version.into(),
            transport,
            state: HandshakeState::Init,
            sessions: HashMap::new(),
            language: "rust".to_string(),
            heartbeat_interval: Duration::from_secs(30),
            auth_token: None,
        }
    }

    /// 设置认证 token
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// 设置语言
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// 获取当前状态
    #[must_use] 
    pub fn state(&self) -> &HandshakeState {
        &self.state
    }

    /// 获取活跃会话列表
    #[must_use] 
    pub fn sessions(&self) -> &HashMap<SessionId, Session> {
        &self.sessions
    }

    /// 获取传输层引用
    #[must_use] 
    pub fn transport(&self) -> &dyn Transport {
        &*self.transport
    }

    /// 获取传输层可变引用
    pub fn transport_mut(&mut self) -> &mut dyn Transport {
        &mut *self.transport
    }

    /// 启动协议引擎
    ///
    /// 1. 启动传输层
    /// 2. 广播发现消息
    /// 3. 进入 Discovered 状态
    pub fn start(&mut self) -> Result<()> {
        self.transport.start()?;
        self.state = HandshakeState::Discovered;
        Ok(())
    }

    /// 停止协议引擎
    ///
    /// 1. 关闭所有会话
    /// 2. 广播下线消息
    /// 3. 停止传输层
    pub fn stop(&mut self) -> Result<()> {
        // 广播下线
        for session in self.sessions.values() {
            let msg = Message::new(
                MessageType::Disconnect,
                self.agent_id.clone(),
                session.peer_id.clone(),
                serde_json::json!({"reason": "agent shutting down"}),
            );
            let _ = self.transport.send(&session.peer_id.to_string(), &msg);
        }
        self.sessions.clear();
        self.transport.stop()?;
        self.state = HandshakeState::Closed;
        Ok(())
    }

    /// 发现其他 Agent
    ///
    /// 返回当前所有可见的 peer
    pub fn discover(&self) -> Result<Vec<PeerInfo>> {
        let peers = self.transport.discover()?;
        Ok(peers)
    }

    /// 向指定 peer 发起握手
    ///
    /// 1. 发送 `HANDSHAKE_REQ`
    /// 2. 等待 `HANDSHAKE_RESP`
    /// 3. 建立 Session
    pub fn handshake(&mut self, peer: &PeerInfo) -> Result<Session> {
        if self.state == HandshakeState::Closed {
            return Err(HandshakeError::Protocol("Protocol is closed".to_string()));
        }

        self.state = HandshakeState::Connecting;

        // 构建握手请求
        let mut payload = serde_json::json!({
            "version": "1.0",
            "agent_name": self.agent_name,
            "agent_version": self.agent_version,
            "language": self.language,
            "requested_capabilities": [],
        });

        // 可选认证
        if let Some(token) = &self.auth_token {
            payload["auth"] = serde_json::json!({
                "method": "token",
                "credentials": token,
            });
        }

        let req = Message::new(
            MessageType::HandshakeReq,
            self.agent_id.clone(),
            peer.agent_id.clone(),
            payload,
        );

        // 发送握手请求
        self.transport.send(&peer.endpoint, &req)?;

        // 创建会话（等待响应确认）
        let session = Session::new(
            peer.agent_id.clone(),
            &peer.agent_name,
            peer.capabilities.clone(),
        );
        let session_id = session.id.clone();
        self.sessions.insert(session_id.clone(), session.clone());
        self.state = HandshakeState::Connected;

        Ok(session)
    }

    /// 处理接收到的消息
    ///
    /// 自动路由到对应 handler：
    /// - `HandshakeReq` → `HandshakeResp`
    /// - Ping → Pong
    /// - Data → 返回消息
    /// - Disconnect → 清理会话
    pub fn handle_message(&mut self, msg: Message) -> Result<Option<Message>> {
        match msg.msg_type {
            MessageType::HandshakeReq => self.handle_handshake_req(msg),
            MessageType::HandshakeResp => self.handle_handshake_resp(msg),
            MessageType::Ping => self.handle_ping(msg),
            MessageType::Pong => self.handle_pong(msg),
            MessageType::Data => Ok(Some(msg)),
            MessageType::Disconnect => self.handle_disconnect(msg),
            _ => Ok(None),
        }
    }

    /// 处理握手请求 → 返回握手响应
    fn handle_handshake_req(&mut self, msg: Message) -> Result<Option<Message>> {
        // 检查协议版本
        let version = msg
            .payload
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if version != "1.0" {
            let resp = Message::new(
                MessageType::HandshakeResp,
                self.agent_id.clone(),
                msg.from.clone(),
                serde_json::json!({
                    "status": "rejected",
                    "reason": format!("Unsupported version: {}", version),
                }),
            );
            return Ok(Some(resp));
        }

        // 可选认证
        if let Some(auth) = msg.payload.get("auth")
            && let (Some(token), Some(method)) = (
                auth.get("credentials").and_then(|c| c.as_str()),
                auth.get("method").and_then(|m| m.as_str()),
            )
        {
            // 验证 token（简化：检查格式）
            if method == "token" && !token.starts_with("ahp_tkn_") {
                let resp = Message::new(
                    MessageType::HandshakeResp,
                    self.agent_id.clone(),
                    msg.from.clone(),
                    serde_json::json!({
                        "status": "rejected",
                        "reason": "Invalid auth token",
                    }),
                );
                return Ok(Some(resp));
            }
        }

        // 获取请求的能力
        let requested: Vec<String> = msg
            .payload
            .get("requested_capabilities")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();

        // 创建会话
        let session = Session::new(
            msg.from.clone(),
            msg.payload
                .get("agent_name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown"),
            requested.clone(),
        );
        let session_id = session.id.clone();
        self.sessions.insert(session_id.clone(), session);

        // 构建响应
        let resp = Message::new(
            MessageType::HandshakeResp,
            self.agent_id.clone(),
            msg.from.clone(),
            serde_json::json!({
                "status": "accepted",
                "session_id": session_id.to_string(),
                "granted_capabilities": requested,
                "keep_alive_interval": self.heartbeat_interval.as_secs(),
            }),
        );

        // 更新状态
        self.state = HandshakeState::Connected;

        Ok(Some(resp))
    }

    /// 处理握手响应
    fn handle_handshake_resp(&mut self, msg: Message) -> Result<Option<Message>> {
        let status = msg
            .payload
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("rejected");

        if status == "accepted" {
            if let Some(session_id_str) = msg.payload.get("session_id").and_then(|s| s.as_str()) {
                let session_id = SessionId::new(session_id_str);
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.state = HandshakeState::Active;
                    self.state = HandshakeState::Active;
                }
            }
            return Ok(Some(msg));
        }

        // 被拒绝
        let reason = msg
            .payload
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown");
        Err(HandshakeError::Auth(format!(
            "Handshake rejected: {reason}"
        )))
    }

    /// 处理 Ping → 返回 Pong
    fn handle_ping(&mut self, msg: Message) -> Result<Option<Message>> {
        let pong = Message::pong(&msg);
        // 更新会话心跳
        if let Some(session_id) = &msg.session_id
            && let Some(session) = self.sessions.get_mut(session_id)
        {
            session.heartbeat();
        }
        Ok(Some(pong))
    }

    /// 处理 Pong → 更新心跳
    fn handle_pong(&mut self, msg: Message) -> Result<Option<Message>> {
        if let Some(session_id) = &msg.session_id
            && let Some(session) = self.sessions.get_mut(session_id)
        {
            session.heartbeat();
        }
        Ok(None)
    }

    /// 处理断开连接
    fn handle_disconnect(&mut self, msg: Message) -> Result<Option<Message>> {
        if let Some(session_id) = &msg.session_id {
            self.sessions.remove(session_id);
        }
        Ok(None)
    }

    /// 发送心跳（Ping）
    pub fn send_heartbeat(&self, session_id: &SessionId) -> Result<()> {
        if let Some(session) = self.sessions.get(session_id) {
            let ping = Message::ping(
                self.agent_id.clone(),
                session.peer_id.clone(),
                session_id.clone(),
            );
            self.transport.send(&session.peer_id.to_string(), &ping)?;
        }
        Ok(())
    }

    /// 发送业务数据消息
    pub fn send_data(
        &self,
        session_id: &SessionId,
        payload: serde_json::Value,
        correlation_id: Option<String>,
    ) -> Result<()> {
        if let Some(session) = self.sessions.get(session_id) {
            let msg = Message::data(
                self.agent_id.clone(),
                session.peer_id.clone(),
                session_id.clone(),
                payload,
                correlation_id,
            );
            self.transport.send(&session.peer_id.to_string(), &msg)?;
            Ok(())
        } else {
            Err(HandshakeError::Session(format!(
                "Session not found: {session_id}"
            )))
        }
    }

    /// 接收消息（非阻塞）
    pub fn recv(&self) -> Result<Option<Message>> {
        self.transport.recv()
    }

    /// 检查并清理过期会话
    pub fn clean_expired_sessions(&mut self, max_missed: u32) -> Vec<SessionId> {
        let mut expired = Vec::new();
        self.sessions.retain(|id, session| {
            if session.is_heartbeat_expired(max_missed) {
                expired.push(id.clone());
                false
            } else {
                true
            }
        });
        expired
    }

    /// 获取 `agent_id`
    #[must_use] 
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MemoryTransport;

    /// 创建测试用的 MemoryTransport Agent
    fn make_protocol(name: &str) -> HandshakeProtocol {
        let agent_id = AgentId::new(name);
        let transport = MemoryTransport::new(name);
        HandshakeProtocol::new(agent_id, name, "1.0.0", Box::new(transport))
    }

    #[test]
    fn test_protocol_lifecycle() {
        let mut proto = make_protocol("test-agent");
        assert_eq!(proto.state(), &HandshakeState::Init);

        proto.start().expect("Should start");
        assert_eq!(proto.state(), &HandshakeState::Discovered);

        proto.stop().expect("Should stop");
        assert_eq!(proto.state(), &HandshakeState::Closed);
    }

    #[test]
    fn test_handshake_req_resp() {
        let mut alice = make_protocol("alice");
        let mut bob = make_protocol("bob");

        alice.start().unwrap();
        bob.start().unwrap();

        // 模拟握手请求
        let req = Message::new(
            MessageType::HandshakeReq,
            AgentId::new("alice"),
            AgentId::new("bob"),
            serde_json::json!({
                "version": "1.0",
                "agent_name": "Alice",
                "agent_version": "1.0.0",
                "language": "rust",
            }),
        );

        let resp = bob.handle_message(req).expect("Should handle handshake");
        assert!(resp.is_some(), "Should produce response");
        let resp = resp.unwrap();
        assert_eq!(resp.msg_type, MessageType::HandshakeResp);
        assert_eq!(
            resp.payload.get("status").and_then(|s| s.as_str()),
            Some("accepted")
        );
    }

    #[test]
    fn test_ping_pong() {
        let mut proto = make_protocol("ping-agent");
        proto.start().unwrap();

        let peer_id = AgentId::new("peer");
        let session = Session::new(peer_id.clone(), "peer", vec![]);
        let session_id = session.id.clone();
        proto.sessions.insert(session_id.clone(), session);

        // Ping
        let ping = Message::ping(proto.agent_id.clone(), peer_id, session_id.clone());

        // 处理 Ping → 应返回 Pong
        let result = proto.handle_message(ping).expect("Should handle ping");
        assert!(result.is_some(), "Ping should produce Pong");
        let pong = result.unwrap();
        assert_eq!(pong.msg_type, MessageType::Pong);
    }

    #[test]
    fn test_auth_rejection() {
        let mut bob = make_protocol("bob");

        let req = Message::new(
            MessageType::HandshakeReq,
            AgentId::new("alice"),
            AgentId::new("bob"),
            serde_json::json!({
                "version": "1.0",
                "auth": {
                    "method": "token",
                    "credentials": "invalid_token"
                }
            }),
        );

        let result = bob.handle_message(req);
        assert!(result.is_ok(), "Auth should return Ok response");
        let resp = result.unwrap();
        assert!(resp.is_some(), "Should produce response message");
        let resp = resp.unwrap();
        assert_eq!(resp.msg_type, MessageType::HandshakeResp);
        assert_eq!(
            resp.payload.get("status").and_then(|s| s.as_str()),
            Some("rejected"),
            "Invalid auth should be rejected"
        );
    }
}
