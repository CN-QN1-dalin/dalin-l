# Agent Handshake Protocol (AHP) 🤝

> **一句话**: Agent 之间握个手，知道对面是谁、能干什么、怎么聊天。

## 这玩意儿是干啥的？

你有两个（或多个）AI Agent，想让它们自己发现对方、建立连接、互相发消息。

**没有 AHP 之前**：
```
Agent A 和 Agent B 各跑各的，老死不相往来。
如果想让它们说话，你得自己写 socket 代码、自己定消息格式、
自己处理心跳、自己管会话生命周期。
```

**有了 AHP**：
```
Agent A 上线 → 广播 "我在这，我叫 dalin-l，我会编译代码"
Agent B 上线 → 广播 "我在这，我叫 dalin-x，我会认知推理"
              ↓
A 发现 B → 握手 → 建立会话 → 互相调用能力
```

## 怎么工作的？（30 秒版）

```
Agent A                          Agent B
  │                                │
  │── 广播自己的身份和能力 ───────→│   发现
  │←─ 回应 ───────────────────────│
  │                                │
  │── 请求建立会话 ───────────────→│   握手
  │←─ 同意，给你 session ID ──────│
  │                                │
  │══ 通过 session 双向通信 ═══════│   通信
  │── 心跳保活 ──────────────────→│
  │←─ pong ──────────────────────│
  │                                │
  │── 下线通知 ──────────────────→│   关闭
```

## 支持几种聊天方式？

| 方式 | 适用场景 | 一句话 |
|------|---------|--------|
| **文件传输** | 同一台机器，调试友好 | 往共享目录写 JSON 文件 |
| **Unix Socket** | 同一台机器，低延迟 | 本地 IPC，毫秒级响应 |
| **TCP** | 不同机器，跨网络 | 走网络，可跨服务器 |
| **内存通道** | 同一个进程内 | 两个 Agent 在同一个程序里 |
| **自定义** | 你有特殊需求 | 实现 Transport trait 即可 |

## 怎么用？（Rust 版）

```rust
use dalin_handshake::prelude::*;

// 1. 创建你的 Agent
let mut agent = Agent::builder()
    .name("dalin-l-compiler")           // 名字
    .transport(Box::new(                 // 传输方式
        FileTransport::new("/tmp/dalin-agents", "dalin-l")
    ))
    .capability("compile", "3.0.0")     // 我会编译代码
    .capability("type_check", "3.0.0")  // 我会类型检查
    .build()?;

// 2. 上线
agent.start()?;

// 3. 看看周围有哪些 Agent
let peers = agent.discover()?;
for p in &peers {
    println!(" 发现: {} ({}) — 能力: {:?}", 
        p.agent_name, p.language, p.capabilities);
}

// 4. 跟其中一个握手
if let Some(peer) = peers.first() {
    let session = agent.handshake(peer)?;
    
    // 5. 发消息
    agent.send(&session.id, json!({
        "action": "compile",
        "code": "fn hello() { return 42 }"
    }))?;
    
    // 6. 收消息
    while let Some(msg) = agent.recv()? {
        println!("收到: {:?}", msg.payload);
    }
}

// 7. 下线
agent.stop()?;
```

## 怎么用？（Python 版 — 给 Dalin X 用）

```python
from ahp_client import Agent

# 1. 创建 Agent
agent = Agent.with_file_transport("dalin-x", "/tmp/dalin-agents")

# 2. 上线
agent.start()

# 3. 发现
peers = agent.discover()
for p in peers:
    print(f"发现: {p.agent_name} ({p.language})")

# 4. 握手
if peers:
    session_id = agent.handshake(peers[0])
    agent.send_data(session_id, {"action": "infer", "input": "..."})

# 5. 下线
agent.stop()
```

## 目录结构

```
dalin-handshake/
├── README.md              ← 你在看的就是这个
├── Cargo.toml             ← Rust crate 配置
├── python/
│   └── ahp_client.py      ← Python 客户端（给 Dalin X 用）
└── src/
    ├── lib.rs             ← crate 入口 + prelude
    ├── types.rs           ← 核心类型 (AgentId, Session, Message...)
    ├── error.rs           ← 错误类型
    ├── protocol.rs        ← 握手协议引擎
    ├── agent.rs           ← 高层 Agent API (builder 模式)
    └── transport/
        ├── mod.rs         ← Transport trait + MemoryTransport
        ├── file.rs        ← 文件传输 (与 Python 互通)
        ├── unix.rs        ← Unix Socket (feature-gated)
        └── tcp.rs         ← TCP 网络 (feature-gated)
```

## 协议规范

详细的 JSON 消息格式、握手状态机、安全模型见：
[`docs/handshake-protocol.md`](../docs/handshake-protocol.md)

## 从 Agent 视角看

> **我是一个 Agent，我该如何接入这个协议？**

1. 实现一个 Transport（或者直接用已有的 File/TCP）
2. 启动时广播自己的 `announce.json`（或等效的发现消息）
3. 实现 `HandshakeReq → HandshakeResp` 的消息处理
4. 收到 `Data` 消息就干活，收到 `Ping` 就回 `Pong`
5. 下线前发 `Disconnect`

就这么简单。协议本身不规定你的业务逻辑——只负责让 Agent 们互相找到、互相认识、互相说话。
