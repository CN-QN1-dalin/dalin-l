# Agent Handshake Protocol (AHP) — 通用 Agent 握手协议

> **版本**: 1.0.0-draft  
> **状态**: 草案  
> **兼容性**: Dalin L (Rust) / Dalin X (Python) / 第三方 Agent

---

## 目录

1. [概述](#1-概述)
2. [传输层](#2-传输层)
3. [握手层](#3-握手层)
4. [消息层](#4-消息层)
5. [生命周期](#5-生命周期)
6. [安全模型](#6-安全模型)
7. [SDK 快速开始](#7-sdk-快速开始)
8. [附录](#8-附录)

---

## 1. 概述

### 1.1 设计目标

| 目标 | 说明 |
|------|------|
| **通用性** | 任何 Agent（Dalin L、Dalin X、第三方）均可实现 |
| **轻量** | 零外部依赖，核心协议仅定义 JSON 消息格式 |
| **多传输** | 文件、Unix Socket、TCP、内存通道 |
| **自发现** | Agent 上线后自动广播自身能力 |
| **可认证** | 可选 Token 或密钥认证 |
| **跨语言** | Rust SDK + Python 桥接，协议本身语言无关 |

### 1.2 核心概念

```
AgentA                    AgentB
  │                         │
  │─── Discovery ──────────→│  AgentA 广播自身信息
  │←─── Discovery Ack ──────│  AgentB 回应
  │                         │
  │─── Handshake Req ──────→│  请求建立会话
  │←─── Handshake Resp ─────│  能力协商 + 会话建立
  │                         │
  │═══ Session (双向通信) ═══│  持续消息交换
  │─── Heartbeat ──────────→│  保活
  │←─── Heartbeat ──────────│
  │                         │
  │─── Disconnect ─────────→│  优雅断开
```

### 1.3 术语

| 术语 | 说明 |
|------|------|
| **Agent** | 实现了 AHP 的任何实体 |
| **Session** | 一次握手建立的持续通信上下文 |
| **Capability** | Agent 对外暴露的能力声明 |
| **Transport** | 底层消息传输通道 |
| **Endpoint** | Agent 的通信地址（文件路径、socket 路径、地址:端口） |

---

## 2. 传输层

传输层负责消息的物理投递。AHP 定义了三种传输模式：

### 2.1 文件传输 (FileTransport)

基于共享目录的异步消息传递。适合**同机、无需持久连接**的场景。

**工作原理**：
```
/var/run/dalin/agents/
├── agent-a/
│   ├── announce.json      # AgentA 的广播信息
│   ├── capabilities.json  # AgentA 的能力声明
│   ├── inbox/
│   │   ├── msg-001.json   # 收到的消息
│   │   └── ...
│   └── outbox/
│       ├── msg-001.json   # 发送中的消息
│       └── ...
└── agent-b/
    ├── announce.json
    ├── capabilities.json
    ├── inbox/
    └── outbox/
```

**消息格式**：每个消息是一个 JSON 文件，文件名 `msg-{uuid}.json`

**优点**：零网络依赖，调试友好  
**缺点**：延迟高（需轮询），不适合高频通信

### 2.2 Unix Domain Socket 传输 (UnixTransport)

基于 Unix Socket 的同步/异步通信。适合**同机、高频**场景。

**工作原理**：
```
AgentA ──[unix:/tmp/dalin/agent-a.sock]──→ AgentB
AgentA ←──[unix:/tmp/dalin/agent-b.sock]─── AgentB
```

**帧格式**：`[4-byte-length][JSON-payload]`

**优点**：低延迟，双向实时  
**缺点**：仅限同机

### 2.3 TCP 传输 (TcpTransport)

基于 TCP 的网络通信。适合**跨机器**场景。

**工作原理**：
```
AgentA ──[tcp:192.168.1.10:9876]──→ AgentB
AgentA ←──[tcp:192.168.1.20:9876]─── AgentB
```

**帧格式**：`[4-byte-length][JSON-payload]`

**优点**：跨网络  
**缺点**：需处理网络故障

---

## 3. 握手层

### 3.1 发现 (Discovery)

Agent 上线时广播自身信息：

```json
{
  "protocol": "ahp/1.0",
  "agent_id": "dalin-l-3-abc123",
  "agent_name": "DalinL-Compiler",
  "agent_version": "3.0.0",
  "language": "rust",
  "transport": "unix",
  "endpoint": "/tmp/dalin/agent-dalin-l.sock",
  "capabilities": ["compile", "type_check", "profile", "evolve"],
  "started_at": "2026-07-25T10:00:00Z",
  "ttl_seconds": 300
}
```

### 3.2 能力声明 (Capabilities)

Agent 详细描述自身能力：

```json
{
  "agent_id": "dalin-l-3-abc123",
  "capabilities": [
    {
      "name": "compile",
      "version": "3.0.0",
      "description": "Compile .dal source files to bytecode",
      "input_schema": {
        "type": "object",
        "properties": {
          "source": { "type": "string" },
          "options": { "type": "object" }
        }
      },
      "output_schema": {
        "type": "object",
        "properties": {
          "bytecode": { "type": "string", "format": "hex" },
          "warnings": { "type": "array" }
        }
      }
    },
    {
      "name": "type_check",
      "version": "3.0.0",
      "description": "Perform type checking on Dalin L source code",
      "input_schema": { "type": "object" },
      "output_schema": { "type": "object" }
    }
  ]
}
```

### 3.3 握手请求 (Handshake Request)

```
AgentA ───────────────────────────────── AgentB
  │                                         │
  │  HANDSHAKE_REQ                          │
  │  {                                      │
  │    "type": "handshake_req",             │
  │    "session_id": "sess-xyz",           │
  │    "version": "1.0",                   │
  │    "auth": { "token": "..." },         │  ← 可选
  │    "requested_capabilities": ["compile"]│
  │  }                                      │
  │────────────────────────────────────────→│
  │                                         │
  │  HANDSHAKE_RESP                         │
  │  {                                      │
  │    "type": "handshake_resp",            │
  │    "session_id": "sess-xyz",           │
  │    "status": "accepted",               │
  │    "granted_capabilities": ["compile"], │
  │    "keep_alive_interval": 30            │
  │  }                                      │
  │←────────────────────────────────────────│
```

### 3.4 握手状态机

```
         ┌──────────┐
         │  INIT    │ ← Agent 创建
         └────┬─────┘
              │ discover()
              ↓
         ┌──────────┐
         │ DISCOVER │ ← 广播自身能力
         └────┬─────┘
              │ handshake()
              ↓
         ┌──────────┐
         │ CONNECT  │ ← 会话建立
         └────┬─────┘
              │ heartbeat timeout
         ┌────┴─────┐
         │  ACTIVE  │ ← 正常通信
         └────┬─────┘
              │ disconnect()
         ┌────┴─────┐
         │  CLOSED  │ ← 会话结束
         └──────────┘
```

---

## 4. 消息层

### 4.1 消息格式

所有消息使用 JSON 编码，遵循统一格式：

```json
{
  "id": "msg-001",
  "type": "data",
  "version": "1.0",
  "from": "agent-a",
  "to": "agent-b",
  "session_id": "sess-xyz",
  "timestamp": "2026-07-25T10:00:00.123Z",
  "payload": {},
  "correlation_id": null
}
```

### 4.2 消息类型

| 类型 | 方向 | 说明 |
|------|------|------|
| `discovery` | 广播 | 发现 Announcement |
| `discovery_ack` | 单播 | 发现响应 |
| `handshake_req` | 请求 | 握手请求 |
| `handshake_resp` | 响应 | 握手响应 |
| `data` | 双向 | 业务数据 |
| `ping` | 双向 | 心跳 |
| `pong` | 响应 | 心跳响应 |
| `error` | 双向 | 错误通知 |
| `disconnect` | 通知 | 断开连接 |

### 4.3 心跳 (Heartbeat)

```
AgentA ── ping ──→ AgentB
AgentA ←── pong ── AgentB
```

- 频率：握手时协商（默认 30s）
- 超时：3 次未收到 pong 视为断开
- 重连：自动重新发起握手

### 4.4 业务数据

```json
{
  "id": "msg-042",
  "type": "data",
  "from": "dalin-x",
  "to": "dalin-l",
  "session_id": "sess-xyz",
  "timestamp": "2026-07-25T10:01:00.000Z",
  "payload": {
    "method": "compile",
    "params": {
      "source": "fn main() @ pure @ cpu { return 42 }",
      "options": { "optimize": true }
    }
  },
  "correlation_id": "req-001"
}
```

---

## 5. 生命周期

### 5.1 Agent 上线

```
1. Agent 启动 → 生成 agent_id
2. 创建传输层端点（文件目录 / socket / 端口）
3. 广播 Discovery 消息
4. 监听来自其他 Agent 的握手请求
5. 进入 ACTIVE 状态
```

### 5.2 会话建立

```
1. AgentA 发现 AgentB
2. AgentA 发送 HANDSHAKE_REQ
3. AgentB 验证 auth（可选）
4. AgentB 返回 HANDSHAKE_RESP (accepted/rejected)
5. 双方进入 ACTIVE 状态
6. 启动心跳定时器
```

### 5.3 会话保持

```
1. 每 30s 发送 ping
2. 收到 pong → 续期
3. 连续 3 次未收到 pong → 超时断开
4. 自动重连（最多 3 次）
```

### 5.4 正常断开

```
1. AgentA 发送 DISCONNECT
2. AgentB 确认 → 清理会话资源
3. AgentA 清理会话资源
4. 进入 CLOSED 状态
```

### 5.5 异常断开

```
1. 心跳超时 → 认为对方断开
2. 清理会话资源
3. 进入 CLOSED 状态
4. 可选：自动重连
```

---

## 6. 安全模型

### 6.1 认证

可选 Token 认证：

```json
{
  "type": "handshake_req",
  "auth": {
    "method": "token",
    "credentials": "ahp_tkn_abc123xyz"
  }
}
```

### 6.2 消息完整性

- 可选：HMAC-SHA256 签名
- 签名字段 `signature` 添加到消息顶层
- 签名内容：`id + type + from + to + payload + secret`

### 6.3 传输安全

- 文件传输：依赖文件系统权限（0600）
- Unix Socket：依赖文件系统权限（0700）
- TCP：建议使用 TLS（不在本协议规范内）

---

## 7. SDK 快速开始

### 7.1 Rust (Dalin L)

```rust
use dalin_handshake::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // 创建 Agent
    let agent = Agent::builder()
        .name("DalinL-Compiler")
        .transport(UnixTransport::new("/tmp/dalin/agent.sock"))
        .capability("compile", "3.0.0")
        .capability("type_check", "3.0.0")
        .build()?;

    // 启动（自动广播 + 监听）
    agent.start().await?;

    // 发现其他 Agent
    let peers = agent.discover().await?;

    // 握手连接
    let session = agent.handshake(&peers[0]).await?;

    // 发送消息
    session.send(json!({
        "method": "compile",
        "params": { "source": "fn main() { return 42 }" }
    })).await?;

    // 接收消息
    while let Some(msg) = session.recv().await {
        println!("Received: {:?}", msg);
    }

    Ok(())
}
```

### 7.2 Python (Dalin X)

```python
from dalin_handshake import Agent, FileTransport, UnixTransport

# 创建 Agent
agent = Agent(
    name="DalinX-Cognitive",
    transport=UnixTransport("/tmp/dalin/agent-dalinx.sock"),
    capabilities=["infer", "evolve", "analyze"]
)

# 启动
agent.start()

# 发现
for peer in agent.discover():
    print(f"Found: {peer.name}")

# 握手
session = agent.handshake(peer)
session.send({
    "method": "analyze",
    "params": {"data": [...], "threshold": 0.8}
})

# 响应
response = session.recv()
print(response)
```

---

## 8. 附录

### 8.1 错误码

| 错误码 | 说明 |
|--------|------|
| `ERR_VERSION_MISMATCH` | 协议版本不兼容 |
| `ERR_AUTH_FAILED` | 认证失败 |
| `ERR_CAPABILITY_NOT_FOUND` | 请求的能力不存在 |
| `ERR_SESSION_EXPIRED` | 会话已过期 |
| `ERR_INTERNAL` | 内部错误 |

### 8.2 保留端口

| 用途 | 默认端口 | 说明 |
|------|----------|------|
| Dalin L 编译器 | 9876 | TCP 传输 |
| Dalin X 认知引擎 | 9877 | TCP 传输 |
| 控制面 | 9878 | TCP 传输 |

### 8.3 默认文件路径

| 用途 | 路径 |
|------|------|
| 共享目录 | `/var/run/dalin/agents/` |
| Unix Socket 目录 | `/tmp/dalin/` |
| 日志 | `/var/log/dalin/handshake/` |