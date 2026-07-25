//! 文件传输 — 基于共享目录的异步消息传递
//!
//! Agent 在共享目录中创建子目录，通过 JSON 文件交换消息。
//! 适合零依赖、同机、调试友好的场景。

use crate::error::{HandshakeError, Result};
use crate::types::*;
use crate::transport::Transport;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 文件传输
///
/// 目录结构：
/// ```text
/// /var/run/dalin/agents/
/// ├── agent-a/
/// │   ├── announce.json
/// │   ├── capabilities.json
/// │   └── inbox/
/// └── agent-b/
///     ├── announce.json
///     └── inbox/
/// ```
pub struct FileTransport {
    /// 共享目录根路径
    base_dir: PathBuf,
    /// 本 Agent 的目录
    agent_dir: PathBuf,
    /// Agent 标识
    agent_id: String,
    /// 本 Agent 的发现信息
    discovery: Option<Discovery>,
    /// 已发现的 peer
    peers: Mutex<Vec<PeerInfo>>,
}

impl FileTransport {
    /// 创建文件传输
    ///
    /// `base_dir`: 共享目录路径，如 `/var/run/dalin/agents/` 或 `./tmp/dalin-agents/`
    /// `agent_id`: 本 Agent 的唯一标识
    pub fn new(base_dir: impl Into<PathBuf>, agent_id: impl Into<String>) -> Self {
        let base = base_dir.into();
        let aid: String = agent_id.into();
        let agent_dir = base.join(&aid);
        Self {
            base_dir: base,
            agent_dir,
            agent_id: aid,
            discovery: None,
            peers: Mutex::new(Vec::new()),
        }
    }

    /// 设置发现信息
    pub fn with_discovery(mut self, discovery: Discovery) -> Self {
        self.discovery = Some(discovery);
        self
    }

    /// 获取本 Agent 目录路径
    pub fn agent_dir(&self) -> &Path {
        &self.agent_dir
    }

    /// 获取 peer 的目录路径
    fn peer_dir(&self, peer_id: &str) -> PathBuf {
        self.base_dir.join(peer_id)
    }

    /// 获取 peer 的 inbox 路径
    fn peer_inbox(&self, peer_id: &str) -> PathBuf {
        self.peer_dir(peer_id).join("inbox")
    }

    /// 写入 JSON 文件
    fn write_json(&self, path: &Path, value: &impl serde::Serialize) -> Result<()> {
        let json = serde_json::to_string_pretty(value)?;
        std::fs::write(path, json).map_err(|e| HandshakeError::Io(e.to_string()))?;
        Ok(())
    }

    /// 读取 JSON 文件
    fn read_json<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<Option<T>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| HandshakeError::Io(e.to_string()))?;
        let value: T = serde_json::from_str(&content)?;
        Ok(Some(value))
    }

    /// 扫描目录下的所有 Agent 子目录
    fn scan_agents(&self) -> Result<Vec<PeerInfo>> {
        let mut peers = Vec::new();
        let read_dir = match std::fs::read_dir(&self.base_dir) {
            Ok(d) => d,
            Err(_) => return Ok(peers),
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            // 跳过自己
            if dir_name == self.agent_id {
                continue;
            }

            // 读取 announce.json
            let announce_path = path.join("announce.json");
            if let Some(discovery) = self.read_json::<Discovery>(&announce_path)?
                && discovery.protocol == "ahp/1.0" {
                    peers.push(PeerInfo {
                        agent_id: discovery.agent_id,
                        agent_name: discovery.agent_name,
                        agent_version: discovery.agent_version,
                        language: discovery.language,
                        transport: discovery.transport,
                        endpoint: discovery.endpoint,
                        capabilities: discovery.capabilities,
                    });
                }
        }

        Ok(peers)
    }
}

impl Transport for FileTransport {
    fn kind(&self) -> &'static str {
        "file"
    }

    fn endpoint(&self) -> &str {
        self.agent_dir.to_str().unwrap_or("unknown")
    }

    fn start(&mut self) -> Result<()> {
        // 创建 Agent 目录结构
        std::fs::create_dir_all(&self.agent_dir)
            .map_err(|e| HandshakeError::Io(format!("Failed to create agent dir: {}", e)))?;
        std::fs::create_dir_all(self.agent_dir.join("inbox"))
            .map_err(|e| HandshakeError::Io(format!("Failed to create inbox: {}", e)))?;

        // 写入 announce.json
        if let Some(discovery) = &self.discovery {
            self.write_json(&self.agent_dir.join("announce.json"), discovery)?;
        }

        // 初始扫描
        let peers = self.scan_agents()?;
        *self.peers.lock().unwrap() = peers;

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // 删除 announce.json 表示下线
        let announce = self.agent_dir.join("announce.json");
        if announce.exists() {
            std::fs::remove_file(&announce)
                .map_err(|e| HandshakeError::Io(e.to_string()))?;
        }
        Ok(())
    }

    fn send(&self, target: &str, msg: &Message) -> Result<()> {
        let inbox = self.peer_inbox(target);
        std::fs::create_dir_all(&inbox)
            .map_err(|e| HandshakeError::Io(e.to_string()))?;

        let filename = format!("msg-{}.json", msg.id);
        let msg_path = inbox.join(&filename);
        self.write_json(&msg_path, msg)?;
        Ok(())
    }

    fn recv(&self) -> Result<Option<Message>> {
        let inbox = self.agent_dir.join("inbox");
        if !inbox.exists() {
            return Ok(None);
        }

        let mut entries: Vec<_> = std::fs::read_dir(&inbox)
            .map_err(|e| HandshakeError::Io(e.to_string()))?
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();

        // 按修改时间排序，取最早的
        entries.sort_by_key(|e| e.path().metadata().ok().map(|m| m.modified().ok()));
        if let Some(entry) = entries.into_iter().next() {
            let path = entry.path();
            if let Some(msg) = self.read_json::<Message>(&path)? {
                // 删除已读取的消息
                std::fs::remove_file(&path)
                    .map_err(|e| HandshakeError::Io(e.to_string()))?;
                return Ok(Some(msg));
            }
        }

        Ok(None)
    }

    fn broadcast(&self, msg: &Message) -> Result<()> {
        // 遍历所有 peer 目录，发送消息
        let peers = self.scan_agents()?;
        for peer in &peers {
            let inbox = self.peer_inbox(&peer.agent_id.to_string());
            std::fs::create_dir_all(&inbox)
                .map_err(|e| HandshakeError::Io(e.to_string()))?;
            let filename = format!("msg-{}.json", msg.id);
            let msg_path = inbox.join(&filename);
            self.write_json(&msg_path, msg)?;
        }
        Ok(())
    }

    fn discover(&self) -> Result<Vec<PeerInfo>> {
        let peers = self.scan_agents()?;
        *self.peers.lock().unwrap() = peers.clone();
        Ok(peers)
    }
}