//! Agent Handshake Protocol (AHP) 通用 Agent 握手协议
//!
//! 提供 Agent 间发现、认证、会话管理、消息通信的全链路能力。
//!
//! ## 快速开始
//!
//! ```rust,no_run
//! use dalin_handshake::prelude::*;
//!
//! fn main() -> Result<()> {
//!     // 创建 Agent (使用 MemoryTransport 用于同进程通信)
//!     let mut agent = Agent::builder()
//!         .name("my-agent")
//!         .transport(Box::new(MemoryTransport::new("my-agent")))
//!         .capability("hello", "1.0.0")
//!         .build()?;
//!
//!     // 启动
//!     agent.start()?;
//!
//!     // 发现其他 Agent
//!     let _peers = agent.discover()?;
//!
//!     // 停止
//!     agent.stop()?;
//!     Ok(())
//! }
//! ```

pub mod agent;
pub mod error;
pub mod protocol;
pub mod transport;
pub mod types;

pub mod prelude {
    //! 便捷导入：use dalin_handshake::prelude::*;
    pub use crate::agent::Agent;
    pub use crate::agent::AgentBuilder;
    pub use crate::error::*;
    pub use crate::protocol::*;
    pub use crate::transport::file::FileTransport;
    pub use crate::transport::tcp::TcpTransport;
    pub use crate::transport::unix::UnixTransport;
    pub use crate::transport::*;
    pub use crate::types::*;
}
