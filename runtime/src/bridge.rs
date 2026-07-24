/// Dalin L 3.0 — DalinX Bridge
///
/// Unix domain socket server that connects DalinX (Python cognitive architecture)
/// to the Dalin L runtime. Protocol: JSON messages over a Unix socket.
///
/// Messages from DalinX:
///   {"type":"set_phase","phase":"reason"}        — set cognitive loop phase
///   {"type":"set_gov","level":"approve"}          — set governance level
///   {"type":"set_confidence","threshold":"verified"} — set confidence threshold
///   {"type":"execute","code":"..."}               — execute Dalin L code
///   {"type":"ping"}                               — health check
///
/// Responses:
///   {"type":"ok","data":...}
///   {"type":"err","message":"..."}
use crate::cognitive::{CognitiveLoopPhase, ConfidenceGate, ConfidenceLevel, GovernanceChecker};
use crate::interpreter::Interpreter;
use dalin_compiler::ty2::GovernanceLevel;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};

const SOCKET_PATH: &str = "/tmp/dalinx.sock";

#[derive(Debug, Deserialize)]
pub struct DalinXMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub threshold: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BridgeResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// DalinX bridge server — connects Python cognitive architecture to Rust runtime
pub struct DalinXBridge {
    pub interpreter: Interpreter,
    listener: Option<UnixListener>,
}

impl Default for DalinXBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl DalinXBridge {
    pub fn new() -> Self {
        Self {
            interpreter: Interpreter::new(),
            listener: None,
        }
    }

    /// Start the Unix socket server
    pub fn start(&mut self) -> Result<(), String> {
        // Remove stale socket if it exists
        let _ = std::fs::remove_file(SOCKET_PATH);
        let listener = UnixListener::bind(SOCKET_PATH)
            .map_err(|e| format!("Cannot bind {}: {}", SOCKET_PATH, e))?;
        self.listener = Some(listener);
        Ok(())
    }

    /// Accept one connection and handle messages
    pub fn accept_once(&mut self) -> Result<(), String> {
        let listener = self.listener.as_ref().ok_or("Bridge not started")?;
        match listener.accept() {
            Ok((stream, _addr)) => self.handle_stream(stream),
            Err(e) => Err(format!("Accept error: {}", e)),
        }
    }

    /// Handle a single Unix socket connection (multi-message)
    fn handle_stream(&mut self, stream: UnixStream) -> Result<(), String> {
        let mut reader = BufReader::new(&stream);
        let mut writer = &stream;

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    match serde_json::from_str::<DalinXMessage>(trimmed) {
                        Ok(msg) => {
                            let resp = self.handle_message(&msg);
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            writeln!(writer, "{}", json).map_err(|e| format!("Write error: {}", e))?;
                            writer.flush().map_err(|e| format!("Flush error: {}", e))?;
                        }
                        Err(e) => {
                            let resp = BridgeResponse {
                                msg_type: "err".into(),
                                data: None,
                                message: Some(format!("Parse error: {}", e)),
                            };
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            writeln!(writer, "{}", json).ok();
                            writer.flush().ok();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[dalinx] Read error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn handle_message(&mut self, msg: &DalinXMessage) -> BridgeResponse {
        match msg.msg_type.as_str() {
            "set_phase" => self.handle_set_phase(msg),
            "set_gov" => self.handle_set_gov(msg),
            "set_confidence" => self.handle_set_confidence(msg),
            "execute" => self.handle_execute(msg),
            "ping" => BridgeResponse {
                msg_type: "ok".into(),
                data: Some(serde_json::json!({"status": "alive", "phase": format!("{}", self.interpreter.cognitive_machine.current_phase)})),
                message: None,
            },
            _ => BridgeResponse {
                msg_type: "err".into(),
                data: None,
                message: Some(format!("Unknown message type: {}", msg.msg_type)),
            },
        }
    }

    fn handle_set_phase(&mut self, msg: &DalinXMessage) -> BridgeResponse {
        let phase = match msg.phase.as_deref() {
            Some("perceive") => CognitiveLoopPhase::Perceiving,
            Some("reason") => CognitiveLoopPhase::Reasoning,
            Some("decide") => CognitiveLoopPhase::Deciding,
            Some("act") => CognitiveLoopPhase::Acting,
            Some("loop") => CognitiveLoopPhase::Looping,
            Some("idle") => CognitiveLoopPhase::Idle,
            _ => return BridgeResponse {
                msg_type: "err".into(),
                data: None,
                message: Some(format!("Unknown phase: {:?}", msg.phase)),
            },
        };
        let label = format!("{}", phase);
        self.interpreter.cognitive_machine.advance(phase, "dalinx", 0);
        BridgeResponse {
            msg_type: "ok".into(),
            data: Some(serde_json::json!({"phase": label})),
            message: None,
        }
    }

    fn handle_set_gov(&mut self, msg: &DalinXMessage) -> BridgeResponse {
        let level = match msg.level.as_deref() {
            Some("prepare") => GovernanceLevel::Prepare,
            Some("suggest") => GovernanceLevel::Suggest,
            Some("approve") => GovernanceLevel::Approve,
            Some("execute") => GovernanceLevel::Execute,
            _ => return BridgeResponse {
                msg_type: "err".into(),
                data: None,
                message: Some(format!("Unknown gov level: {:?}", msg.level)),
            },
        };
        let label = format!("{:?}", level);
        self.interpreter.governance_checker = GovernanceChecker::new(level);
        BridgeResponse {
            msg_type: "ok".into(),
            data: Some(serde_json::json!({"level": label})),
            message: None,
        }
    }

    fn handle_set_confidence(&mut self, msg: &DalinXMessage) -> BridgeResponse {
        let level = ConfidenceLevel::from_annotation(msg.threshold.as_deref());
        let label = format!("{}", level);
        self.interpreter.confidence_gate = ConfidenceGate::new(level);
        BridgeResponse {
            msg_type: "ok".into(),
            data: Some(serde_json::json!({"threshold": label})),
            message: None,
        }
    }

    fn handle_execute(&mut self, msg: &DalinXMessage) -> BridgeResponse {
        let code = match msg.code.as_deref() {
            Some(c) => c,
            None => {
                return BridgeResponse {
                    msg_type: "err".into(),
                    data: None,
                    message: Some("No code in execute message".into()),
                }
            }
        };
        match crate::interpreter::run_source(code) {
            Ok(values) => {
                let results: Vec<String> = values.iter().map(|v| format!("{}", v)).collect();
                BridgeResponse {
                    msg_type: "ok".into(),
                    data: Some(serde_json::json!({
                        "results": results,
                        "phase": format!("{}", self.interpreter.cognitive_machine.current_phase),
                    })),
                    message: None,
                }
            }
            Err(e) => {
                BridgeResponse {
                    msg_type: "ok".into(),
                    data: Some(serde_json::json!({
                        "error": format!("{}", e),
                    })),
                    message: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(json: serde_json::Value) -> DalinXMessage {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_bridge_ping() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "ping"})));
        assert_eq!(resp.msg_type, "ok");
    }

    #[test]
    fn test_bridge_set_phase() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "set_phase", "phase": "reason"})));
        assert_eq!(resp.msg_type, "ok");
        assert_eq!(bridge.interpreter.cognitive_machine.current_phase, CognitiveLoopPhase::Reasoning);
    }

    #[test]
    fn test_bridge_set_gov() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "set_gov", "level": "approve"})));
        assert_eq!(resp.msg_type, "ok");
        match &bridge.interpreter.governance_checker.session_level {
            dalin_compiler::ty2::GovernanceLevel::Approve => {},
            _ => panic!("expected Approve gov level"),
        }
    }

    #[test]
    fn test_bridge_set_confidence() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "set_confidence", "threshold": "verified"})));
        assert_eq!(resp.msg_type, "ok");
        assert_eq!(bridge.interpreter.confidence_gate.threshold, ConfidenceLevel::Verified);
    }

    #[test]
    fn test_bridge_execute_simple() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "execute", "code": "fn f() { return 42 }"})));
        assert_eq!(resp.msg_type, "ok");
    }

    #[test]
    fn test_bridge_unknown_type() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "unknown_command"})));
        assert_eq!(resp.msg_type, "err");
    }

    #[test]
    fn test_bridge_invalid_phase() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "set_phase", "phase": "nonexistent"})));
        assert_eq!(resp.msg_type, "err");
    }

    #[test]
    fn test_bridge_execute_no_code() {
        let mut bridge = DalinXBridge::new();
        let resp = bridge.handle_message(&msg(json!({"type": "execute"})));
        assert_eq!(resp.msg_type, "err");
    }
}
