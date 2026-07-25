//! Unix Domain Socket 传输 — 基于 tokio 的本地 IPC

use crate::error::{HandshakeError, Result};
use crate::types::{Message, PeerInfo};
use crate::transport::Transport;

/// Unix Domain Socket 传输
///
/// 提供低延迟、双向实时通信。需要 tokio 运行时和 unix 平台。
///
/// 默认实现为 stub，完整实现在 `unix` feature 下启用。
pub struct UnixTransport {
    socket_path: String,
    #[allow(dead_code)]
    agent_id: String,
    #[allow(dead_code)]
    running: bool,
}

impl UnixTransport {
    /// 创建 Unix Socket 传输
    ///
    /// `socket_path`: Unix socket 文件路径，如 `/tmp/dalin/agent-a.sock`
    /// `agent_id`: 本 Agent 标识
    pub fn new(socket_path: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            agent_id: agent_id.into(),
            running: false,
        }
    }
}

impl Transport for UnixTransport {
    fn kind(&self) -> &'static str {
        "unix"
    }

    fn endpoint(&self) -> &str {
        &self.socket_path
    }

    fn start(&mut self) -> Result<()> {
        #[cfg(all(feature = "unix", target_family = "unix"))]
        {
            let _ = std::fs::remove_file(&self.socket_path);
            _ = std::os::unix::net::UnixListener::bind(&self.socket_path)
                .map_err(|e| HandshakeError::Io(format!("Failed to bind: {}", e)))?;
            self.running = true;
            Ok(())
        }
        #[cfg(not(all(feature = "unix", target_family = "unix")))]
        {
            Err(HandshakeError::Transport(
                "Unix transport requires 'unix' feature + Unix platform".to_string()
            ))
        }
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }

    fn send(&self, _target: &str, _msg: &Message) -> Result<()> {
        #[cfg(all(feature = "unix", target_family = "unix"))]
        {
            // 连接到目标 socket 并发送 JSON 帧
            let stream = std::os::unix::net::UnixStream::connect(_target)
                .map_err(|e| HandshakeError::Io(format!("Unix connect: {}", e)))?;
            let data = serde_json::to_vec(_msg)?;
            use std::io::Write;
            let mut stream = stream;
            let len = (data.len() as u32).to_be_bytes();
            stream.write_all(&len)
                .map_err(|e| HandshakeError::Io(format!("Unix send: {}", e)))?;
            stream.write_all(&data)
                .map_err(|e| HandshakeError::Io(format!("Unix send: {}", e)))?;
            return Ok(());
        }
        #[cfg(not(all(feature = "unix", target_family = "unix")))]
        Err(HandshakeError::Transport("Unix send not available".to_string()))
    }

    fn recv(&self) -> Result<Option<Message>> {
        Err(HandshakeError::Transport(
            "Unix recv requires async runtime (use tokio)".to_string()
        ))
    }

    fn broadcast(&self, _msg: &Message) -> Result<()> {
        Err(HandshakeError::Transport(
            "Unix broadcast not implemented".to_string()
        ))
    }

    fn discover(&self) -> Result<Vec<PeerInfo>> {
        Ok(Vec::new())
    }
}