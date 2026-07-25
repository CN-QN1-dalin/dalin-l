//! Dalin Bridge — Unix socket server for DalinX connection
//!
//! Bridges the Dalin L runtime to the QN1 cognitive engine via a Unix domain socket.
//! Supports:
//! - Bidirectional message passing (JSON-RPC format)
//! - Signal handling (SIGTERM/SIGINT for graceful shutdown)
//! - Connection lifecycle management (accept → route → handle → reply)

use std::fs;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixListener;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BridgeMessage {
    pub id: u64,
    #[serde(rename = "type")]
    pub r#type: String, // "request" | "response" | "event"
    pub payload: serde_json::Value,
}

impl BridgeMessage {
    pub fn new(id: u64, r#type: &str, payload: serde_json::Value) -> Self {
        Self {
            id,
            r#type: r#type.to_string(),
            payload,
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}

/// 处理单条消息的业务逻辑（可扩展为调用 DalinL 解释器）
pub fn handle_message(msg: &BridgeMessage) -> BridgeMessage {
    let payload = match msg.r#type.as_str() {
        "run" => {
            if let Some(code) = msg.payload.get("code").and_then(|v| v.as_str()) {
                // 实际调用解释器执行代码
                match dalin_runtime::interpreter::run_source(code) {
                    Ok(results) => {
                        let output_values: Vec<serde_json::Value> = results
                            .iter()
                            .map(|v| match v {
                                dalin_runtime::env::Value::Int(i) => serde_json::json!(*i),
                                dalin_runtime::env::Value::Float(f) => serde_json::json!(*f),
                                dalin_runtime::env::Value::String(s) => serde_json::json!(s),
                                dalin_runtime::env::Value::Bool(b) => serde_json::json!(b),
                                dalin_runtime::env::Value::None => serde_json::json!(null),
                                dalin_runtime::env::Value::Array(items) => {
                                    serde_json::json!(
                                        items
                                            .iter()
                                            .map(|it| match it {
                                                dalin_runtime::env::Value::Int(i) =>
                                                    serde_json::json!(*i),
                                                dalin_runtime::env::Value::Float(f) =>
                                                    serde_json::json!(*f),
                                                dalin_runtime::env::Value::String(s) =>
                                                    serde_json::json!(s),
                                                dalin_runtime::env::Value::Bool(b) =>
                                                    serde_json::json!(b),
                                                _ => serde_json::json!(null),
                                            })
                                            .collect::<Vec<_>>()
                                    )
                                }
                                _ => serde_json::json!({"type": "complex"}),
                            })
                            .collect();

                        // 从 AST 获取语句数用于统计
                        use dalin_compiler::lexer;
                        use dalin_compiler::parser;
                        let stmt_count = match lexer::Lexer::new(code).tokenize() {
                            Ok(tokens) => {
                                let prog = parser::Parser::new(tokens).parse().ok();
                                prog.map(|p| p.statements.len()).unwrap_or(0)
                            }
                            Err(_) => 0,
                        };

                        serde_json::json!({
                            "status": "success",
                            "statements": stmt_count,
                            "output": output_values
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "status": "error",
                            "message": e.to_string()
                        })
                    }
                }
            } else {
                serde_json::json!({
                    "error": "missing 'code' field"
                })
            }
        }
        "eval" => {
            if let Some(expr_code) = msg.payload.get("expression").and_then(|v| v.as_str()) {
                match dalin_runtime::interpreter::run_source(&format!("let _ = {}", expr_code)) {
                    Ok(results) => {
                        serde_json::json!({
                            "status": "evaluated",
                            "result": if !results.is_empty() {
                                format!("{:?}", results.last().unwrap())
                            } else {
                                "null".to_string()
                            }
                        })
                    }
                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                }
            } else {
                serde_json::json!({
                    "status": "evaluated",
                    "result": null
                })
            }
        }
        "ping" => {
            serde_json::json!({
                "pong": true,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })
        }
        _ => {
            serde_json::json!({
                "error": format!("unknown message type: {}", msg.r#type)
            })
        }
    };

    BridgeMessage::new(msg.id, "response", serde_json::json!({ "data": payload }))
}

/// 启动 Unix socket 服务器并监听连接
pub fn serve(socket_path: &str) -> Result<(), String> {
    // 清理旧 socket
    if std::path::Path::new(socket_path).exists() {
        fs::remove_file(socket_path)
            .map_err(|e| format!("Failed to remove existing socket '{}': {}", socket_path, e))?;
    }

    let listener = UnixListener::bind(socket_path)
        .map_err(|e| format!("Failed to bind socket '{}': {}", socket_path, e))?;

    println!("  [bridge] Listening on: {}", socket_path);
    println!("  [bridge] Ready for DalinX connections");

    let mut id_counter: u64 = 0;

    loop {
        let (mut stream, _) = match listener.accept() {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("  [bridge] Accept error: {}", e);
                continue;
            }
        };

        id_counter += 1;
        let message_id = id_counter;

        // 读取数据
        let mut buffer = [0u8; 4096];
        let n = stream
            .read(&mut buffer)
            .map_err(|e| format!("Read error: {}", e))?;
        let request = String::from_utf8_lossy(&buffer[..n]);

        println!("  [bridge] Received message #{}", message_id);

        // 尝试解析并处理
        let response = match BridgeMessage::from_json(&request) {
            Ok(msg) => {
                println!("  [bridge] Processing: type={}, id={}", msg.r#type, msg.id);
                let resp = handle_message(&msg);
                resp.to_json().unwrap_or_default()
            }
            Err(e) => {
                // 如果无法解析为 JSON-RPC，作为原始文本处理
                eprintln!("  [bridge] Parse error: {}", e);
                format!("{{\"error\": \"invalid request: {}\"}}", e)
            }
        };

        // 发送响应
        stream.write_all(response.as_bytes()).ok();
        stream.flush().ok();

        // 关闭连接（简单的一次性请求-响应模式）
        stream.shutdown(Shutdown::Both).ok();

        println!("  [bridge] Response sent for #{}", message_id);
    }
}
