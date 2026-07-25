#!/usr/bin/env python3
"""
Agent Handshake Protocol (AHP) Python Client
=============================================

通用 Agent 握手协议的 Python 实现。让 Dalin X (Python) 或其他 Python Agent
能与 Rust Agent 通过文件传输或 TCP 通信。

用法:
    # 作为文件传输客户端 (与 Rust FileTransport 互通)
    python ahp_client.py --transport file --base-dir /tmp/dalin/agents --agent-id my-py-agent

    # 作为 TCP 客户端
    python ahp_client.py --transport tcp --bind 127.0.0.1:9877 --agent-id my-py-agent
"""

import json
import os
import shutil
import socket
import struct
import sys
import time
import uuid
from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional


# ═════════════════════════════════════════════════════════════════
#  类型定义
# ═════════════════════════════════════════════════════════════════

@dataclass
class Message:
    id: str
    type: str  # discovery, discovery_ack, handshake_req, handshake_resp, data, ping, pong, error, disconnect
    version: str
    from_: str
    to: str
    session_id: Optional[str]
    timestamp: str
    payload: dict
    correlation_id: Optional[str] = None

    @classmethod
    def new(cls, msg_type: str, from_id: str, to_id: str, payload: dict) -> "Message":
        return cls(
            id=str(uuid.uuid4()),
            type=msg_type,
            version="1.0",
            from_=from_id,
            to=to_id,
            session_id=None,
            timestamp=datetime.now(timezone.utc).isoformat(),
            payload=payload,
        )

    @classmethod
    def ping(cls, from_id: str, to_id: str, session_id: str) -> "Message":
        msg = cls.new("ping", from_id, to_id, {})
        msg.session_id = session_id
        return msg

    @classmethod
    def pong(cls, ping_msg: "Message") -> "Message":
        msg = cls.new("pong", ping_msg.to, ping_msg.from_, {})
        msg.session_id = ping_msg.session_id
        msg.correlation_id = ping_msg.id
        return msg

    @classmethod
    def data(cls, from_id: str, to_id: str, session_id: str,
             payload: dict, correlation_id: Optional[str] = None) -> "Message":
        msg = cls.new("data", from_id, to_id, payload)
        msg.session_id = session_id
        msg.correlation_id = correlation_id
        return msg

    def to_dict(self) -> dict:
        d = asdict(self)
        d["from"] = d.pop("from_")
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "Message":
        d["from_"] = d.pop("from")
        return cls(**d)


@dataclass
class PeerInfo:
    agent_id: str
    agent_name: str
    agent_version: str
    language: str
    transport: str
    endpoint: str
    capabilities: list[str]


# ═════════════════════════════════════════════════════════════════
#  传输层 — FileTransport (与 Rust FileTransport 互通)
# ═════════════════════════════════════════════════════════════════

class FileTransport:
    """基于共享目录的文件传输层。

    与 Rust 的 FileTransport 完全互通：
    - 在 base_dir 下创建自己的目录
    - 写入 announce.json 宣告存在
    - 在各 Agent 的 inbox/ 目录读写消息 JSON 文件
    """

    def __init__(self, base_dir: str, agent_id: str):
        self.base_dir = Path(base_dir)
        self.agent_id = agent_id
        self.agent_dir = self.base_dir / agent_id
        self.inbox_dir = self.agent_dir / "inbox"
        self._running = False

    def start(self) -> None:
        self.agent_dir.mkdir(parents=True, exist_ok=True)
        self.inbox_dir.mkdir(parents=True, exist_ok=True)
        self._running = True

        # 写入 announce.json
        announce = {
            "protocol": "ahp/1.0",
            "agent_id": self.agent_id,
            "agent_name": self.agent_id,
            "agent_version": "1.0.0",
            "language": "python",
            "transport": "file",
            "endpoint": str(self.agent_dir),
            "capabilities": [],
            "started_at": datetime.now(timezone.utc).isoformat(),
            "ttl_seconds": 300,
        }
        with open(self.agent_dir / "announce.json", "w") as f:
            json.dump(announce, f, indent=2)

    def stop(self) -> None:
        announce = self.agent_dir / "announce.json"
        if announce.exists():
            announce.unlink()
        self._running = False

    def discover(self) -> list[PeerInfo]:
        """扫描 base_dir 下所有 Agent 目录，读取 announce.json"""
        peers = []
        if not self.base_dir.exists():
            return peers

        for entry in self.base_dir.iterdir():
            if not entry.is_dir():
                continue
            if entry.name == self.agent_id:
                continue  # 跳过自己

            announce_path = entry / "announce.json"
            if not announce_path.exists():
                continue

            try:
                with open(announce_path) as f:
                    data = json.load(f)
                if data.get("protocol") == "ahp/1.0":
                    peers.append(PeerInfo(
                        agent_id=data["agent_id"],
                        agent_name=data.get("agent_name", ""),
                        agent_version=data.get("agent_version", ""),
                        language=data.get("language", ""),
                        transport=data.get("transport", ""),
                        endpoint=data.get("endpoint", ""),
                        capabilities=data.get("capabilities", []),
                    ))
            except (json.JSONDecodeError, KeyError):
                continue

        return peers

    def send(self, target_agent_id: str, msg: Message) -> None:
        """发送消息到目标 Agent 的 inbox"""
        target_inbox = self.base_dir / target_agent_id / "inbox"
        target_inbox.mkdir(parents=True, exist_ok=True)

        filename = f"msg-{msg.id}.json"
        msg_path = target_inbox / filename
        with open(msg_path, "w") as f:
            json.dump(msg.to_dict(), f, indent=2)

    def recv(self) -> Optional[Message]:
        """从自己的 inbox 读取最早的消息"""
        if not self.inbox_dir.exists():
            return None

        json_files = sorted(
            [f for f in self.inbox_dir.iterdir() if f.suffix == ".json"],
            key=lambda f: f.stat().st_mtime,
        )

        if not json_files:
            return None

        msg_path = json_files[0]
        try:
            with open(msg_path) as f:
                data = json.load(f)
            msg_path.unlink()  # 读取后删除
            return Message.from_dict(data)
        except (json.JSONDecodeError, KeyError):
            return None


# ═════════════════════════════════════════════════════════════════
#  传输层 — TCP Transport
# ═════════════════════════════════════════════════════════════════

class TcpTransport:
    """基于 TCP 的传输层。

    使用长度前缀 (4-byte big-endian) + JSON 的消息格式。
    与 Rust 的 TcpTransport 互通。
    """

    def __init__(self, bind_addr: str, agent_id: str):
        self.bind_addr = bind_addr
        self.agent_id = agent_id
        self._server: Optional[socket.socket] = None
        self._running = False
        self._recv_buffer: list[Message] = []

    def start(self) -> None:
        host, port_str = self.bind_addr.rsplit(":", 1)
        port = int(port_str)
        self._server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._server.bind((host, port))
        self._server.listen(5)
        self._server.setblocking(False)
        self._running = True

    def stop(self) -> None:
        self._running = False
        if self._server:
            self._server.close()
            self._server = None

    def discover(self) -> list[PeerInfo]:
        # TCP 环境下，发现需要注册中心或 mDNS，简化实现
        return []

    def send(self, target_addr: str, msg: Message) -> None:
        """连接到目标地址并发送消息"""
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(5.0)
            host, port_str = target_addr.rsplit(":", 1)
            s.connect((host, int(port_str)))

            data = json.dumps(msg.to_dict()).encode("utf-8")
            # 长度前缀 (4-byte big-endian)
            s.sendall(struct.pack("!I", len(data)))
            s.sendall(data)
            s.close()
        except Exception as e:
            print(f"[WARN] TCP send failed to {target_addr}: {e}", file=sys.stderr)

    def recv(self) -> Optional[Message]:
        """从接收缓冲区取最早的消息"""
        if self._recv_buffer:
            return self._recv_buffer.pop(0)

        # 尝试接收新连接
        if self._server is None:
            return None

        try:
            while self._running:
                conn, _ = self._server.accept()
                conn.settimeout(1.0)

                # 读取长度前缀
                len_bytes = conn.recv(4)
                if not len_bytes or len(len_bytes) < 4:
                    conn.close()
                    continue

                msg_len = struct.unpack("!I", len_bytes)[0]

                # 读取消息体
                data = b""
                while len(data) < msg_len:
                    chunk = conn.recv(msg_len - len(data))
                    if not chunk:
                        break
                    data += chunk

                conn.close()

                if data:
                    msg_dict = json.loads(data.decode("utf-8"))
                    self._recv_buffer.append(Message.from_dict(msg_dict))
        except BlockingIOError:
            pass  # 没有新连接
        except Exception:
            pass

        return self._recv_buffer.pop(0) if self._recv_buffer else None


# ═════════════════════════════════════════════════════════════════
#  高层 Agent API
# ═════════════════════════════════════════════════════════════════

class Agent:
    """高层 Agent 封装，支持文件传输和 TCP 传输"""

    def __init__(self, agent_id: str, transport: Any):
        self.agent_id = agent_id
        self.transport = transport
        self._started = False
        self._sessions: dict[str, dict] = {}  # session_id -> session info

    @classmethod
    def with_file_transport(cls, agent_id: str, base_dir: str) -> "Agent":
        return cls(agent_id, FileTransport(base_dir, agent_id))

    @classmethod
    def with_tcp_transport(cls, agent_id: str, bind_addr: str) -> "Agent":
        return cls(agent_id, TcpTransport(bind_addr, agent_id))

    def start(self) -> None:
        self.transport.start()
        self._started = True
        print(f"[AHP] Agent '{self.agent_id}' started")

    def stop(self) -> None:
        # 发送下线通知
        for session_id, session in self._sessions.items():
            msg = Message.new("disconnect", self.agent_id, session["peer_id"],
                              {"reason": "agent shutting down"})
            msg.session_id = session_id
            try:
                self.transport.send(session["peer_endpoint"], msg)
            except Exception:
                pass

        self.transport.stop()
        self._started = False
        self._sessions.clear()
        print(f"[AHP] Agent '{self.agent_id}' stopped")

    def discover(self) -> list[PeerInfo]:
        return self.transport.discover()

    def handshake(self, peer: PeerInfo) -> str:
        """向 peer 发起握手，返回 session_id"""
        # 构建握手请求
        payload = {
            "version": "1.0",
            "agent_name": self.agent_id,
            "agent_version": "1.0.0",
            "language": "python",
            "requested_capabilities": peer.capabilities,
        }
        req = Message.new("handshake_req", self.agent_id, peer.agent_id, payload)

        # 发送
        self.transport.send(peer.endpoint, req)

        # 创建会话
        session_id = str(uuid.uuid4())
        self._sessions[session_id] = {
            "peer_id": peer.agent_id,
            "peer_name": peer.agent_name,
            "peer_endpoint": peer.endpoint,
            "state": "connected",
            "capabilities": peer.capabilities,
            "created_at": datetime.now(timezone.utc).isoformat(),
        }
        return session_id

    def send_data(self, session_id: str, payload: dict,
                  correlation_id: Optional[str] = None) -> None:
        """通过会话发送业务数据"""
        session = self._sessions.get(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        msg = Message.data(self.agent_id, session["peer_id"],
                           session_id, payload, correlation_id)
        self.transport.send(session["peer_endpoint"], msg)

    def handle_message(self, msg: Message) -> Optional[Message]:
        """处理接收到的消息，自动响应"""
        if msg.type == "handshake_req":
            return self._handle_handshake_req(msg)
        elif msg.type == "ping":
            return self._handle_ping(msg)
        elif msg.type == "disconnect":
            self._handle_disconnect(msg)
            return None
        return None  # data 消息由上层处理

    def _handle_handshake_req(self, msg: Message) -> Message:
        session_id = str(uuid.uuid4())
        self._sessions[session_id] = {
            "peer_id": msg.from_,
            "peer_name": msg.payload.get("agent_name", "unknown"),
            "peer_endpoint": msg.from_ if hasattr(self.transport, 'agent_id') else msg.from_,
            "state": "active",
            "capabilities": msg.payload.get("requested_capabilities", []),
            "created_at": datetime.now(timezone.utc).isoformat(),
        }

        resp = Message.new("handshake_resp", self.agent_id, msg.from_, {
            "status": "accepted",
            "session_id": session_id,
            "granted_capabilities": msg.payload.get("requested_capabilities", []),
            "keep_alive_interval": 30,
        })
        resp.session_id = session_id
        return resp

    def _handle_ping(self, msg: Message) -> Message:
        pong = Message.pong(msg)
        if msg.session_id and msg.session_id in self._sessions:
            self._sessions[msg.session_id]["last_heartbeat"] = \
                datetime.now(timezone.utc).isoformat()
        return pong

    def _handle_disconnect(self, msg: Message) -> None:
        if msg.session_id and msg.session_id in self._sessions:
            del self._sessions[msg.session_id]


# ═════════════════════════════════════════════════════════════════
#  CLI 入口
# ═════════════════════════════════════════════════════════════════

def main():
    import argparse
    parser = argparse.ArgumentParser(description="AHP Python Agent Client")
    parser.add_argument("--transport", choices=["file", "tcp"], default="file")
    parser.add_argument("--base-dir", default="/tmp/dalin/agents",
                        help="File transport base directory")
    parser.add_argument("--bind", default="127.0.0.1:9877",
                        help="TCP bind address")
    parser.add_argument("--agent-id", required=True,
                        help="Agent unique identifier")

    args = parser.parse_args()

    if args.transport == "file":
        agent = Agent.with_file_transport(args.agent_id, args.base_dir)
    else:
        agent = Agent.with_tcp_transport(args.agent_id, args.bind)

    agent.start()

    try:
        print(f"[AHP] Agent '{args.agent_id}' running. Press Ctrl+C to stop.")
        print(f"[AHP] Discovering peers...")
        peers = agent.discover()
        print(f"[AHP] Found {len(peers)} peer(s):")
        for p in peers:
            print(f"  - {p.agent_name} ({p.agent_id}) @ {p.endpoint} [{p.transport}]")

        # 主循环
        while True:
            time.sleep(1)

    except KeyboardInterrupt:
        print("\n[AHP] Shutting down...")
    finally:
        agent.stop()


if __name__ == "__main__":
    main()
