//! TCP 传输 — 基于 tokio 的网络通信

use crate::error::{HandshakeError, Result};
use crate::types::{Message, PeerInfo};
use crate::transport::Transport;

/// TCP 传输
///
/// 提供跨机器的 Agent 通信。需要 tokio 运行时。
///
/// 默认实现为 stub，完整实现在 `tcp` feature 下启用。
pub struct TcpTransport {
    bind_addr: String,
    #[allow(dead_code)]
    agent_id: String,
    running: bool,
}

impl TcpTransport {
    /// 创建 TCP 传输
    ///
    /// `bind_addr`: 监听地址，如 `0.0.0.0:9876`
    /// `agent_id`: 本 Agent 标识
    pub fn new(bind_addr: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            agent_id: agent_id.into(),
            running: false,
        }
    }
}

impl Transport for TcpTransport {
    fn kind(&self) -> &'static str {
        "tcp"
    }

    fn endpoint(&self) -> &str {
        &self.bind_addr
    }

    fn start(&mut self) -> Result<()> {
        #[cfg(all(feature = "tcp", target_family = "unix"))]
        {
            // 同步 TCP 监听（简化版）
            let listener = std::net::TcpListener::bind(&self.bind_addr)
                .map_err(|e| HandshakeError::Io(format!("TCP bind: {}", e)))?;
            listener.set_nonblocking(true)
                .map_err(|e| HandshakeError::Io(e.to_string()))?;
            self.running = true;
            Ok(())
        }
        #[cfg(not(all(feature = "tcp", target_family = "unix")))]
        {
            Err(HandshakeError::Transport(
                "TCP transport requires 'tcp' feature + Unix platform".to_string()
            ))
        }
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn send(&self, _target: &str, _msg: &Message) -> Result<()> {
        #[cfg(all(feature = "tcp", target_family = "unix"))]
        {
            let stream = std::net::TcpStream::connect(_target)
                .map_err(|e| HandshakeError::Io(format!("TCP connect: {}", e)))?;
            let data = serde_json::to_vec(_msg)?;
            use std::io::Write;
            let mut stream = stream;
            let len = (data.len() as u32).to_be_bytes();
            stream.write_all(&len)
                .map_err(|e| HandshakeError::Io(format!("TCP send: {}", e)))?;
            stream.write_all(&data)
                .map_err(|e| HandshakeError::Io(format!("TCP send: {}", e)))?;
            return Ok(());
        }
        #[cfg(not(all(feature = "tcp", target_family = "unix")))]
        Err(HandshakeError::Transport("TCP send not available".to_string()))
    }

    fn recv(&self) -> Result<Option<Message>> {
        Err(HandshakeError::Transport(
            "TCP recv requires async runtime (use tokio)".to_string()
        ))
    }

    fn broadcast(&self, _msg: &Message) -> Result<()> {
        Err(HandshakeError::Transport(
            "TCP broadcast not implemented".to_string()
        ))
    }

    fn discover(&self) -> Result<Vec<PeerInfo>> {
        Ok(Vec::new())
    }
}