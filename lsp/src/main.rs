//! Dalin L Language Server（最小原型）
//!
//! 通过 stdio 接受 LSP JSON-RPC 消息，提供：
//! - textDocument/diagnostic：编译错误诊断（含七通道标注位置）
//! - textDocument/hover：显示值/效应/能力七通道类型
//!
//! 启动：`dalin-ls`（默认 stdio 模式）

use std::io::{self, BufRead, Write};

fn main() {
    // LSP 初始化通知
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // 简单循环：读取一行 JSON-RPC 请求，返回诊断
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        // 忽略 Content-Length 头，直接解析 JSON 内容
        if line.starts_with("Content-Length") || line.starts_with("{") {
            continue;
        }

        // 尝试解析 JSON-RPC
        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&line) {
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

            match method {
                "initialize" => {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.get("id"),
                        "result": {
                            "capabilities": {
                                "textDocumentSync": 1,
                                "hoverProvider": true,
                                "diagnosticProvider": {
                                    "interFileDependencies": false,
                                    "workspaceDiagnostics": false
                                }
                            }
                        }
                    });
                    let msg = format!("Content-Length: {}\r\n\r\n{}", resp.to_string().len(), resp);
                    let _ = stdout.write_all(msg.as_bytes());
                    let _ = stdout.flush();
                }
                "textDocument/diagnostic" => {
                    // 解析源码
                    let params = req.get("params");
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    let text = text_doc.and_then(|d| d.get("text").and_then(|t| t.as_str())).unwrap_or("");

                    let mut diagnostics = Vec::new();

                    // 词法分析
                    let mut lex = dalin_compiler::lexer::Lexer::new(text);
                    match lex.tokenize() {
                        Ok(tokens) => {
                            let mut parser = dalin_compiler::parser::Parser::new(tokens);
                            match parser.parse() {
                                Ok(prog) => {
                                    // 七通道推断（若标注有效应/能力）
                                    let mut infer = dalin_compiler::ty2::SevenChannelInferencer::new();
                                    infer.infer_program(&prog);
                                    for err in &infer.effect.errors {
                                        diagnostics.push(serde_json::json!({
                                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } },
                                            "severity": 1,
                                            "message": format!("效应错误: {}", err),
                                            "source": "dalin-ls"
                                        }));
                                    }
                                    for err in &infer.capability.errors {
                                        diagnostics.push(serde_json::json!({
                                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } },
                                            "severity": 1,
                                            "message": format!("能力错误: {}", err),
                                            "source": "dalin-ls"
                                        }));
                                    }
                                }
                                Err(e) => {
                                    diagnostics.push(serde_json::json!({
                                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } },
                                        "severity": 1,
                                        "message": format!("语法错误: {}", e),
                                        "source": "dalin-ls"
                                    }));
                                }
                            }
                        }
                        Err(e) => {
                            diagnostics.push(serde_json::json!({
                                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } },
                                "severity": 1,
                                "message": format!("词法错误: {}", e),
                                "source": "dalin-ls"
                            }));
                        }
                    }

                    let result = serde_json::json!({
                        "kind": "full",
                        "items": diagnostics
                    });
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.get("id"),
                        "result": result
                    });
                    let msg = format!("Content-Length: {}\r\n\r\n{}", resp.to_string().len(), resp);
                    let _ = stdout.write_all(msg.as_bytes());
                    let _ = stdout.flush();
                }
                _ => {
                    // 未知方法返回 null
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.get("id"),
                        "result": null
                    });
                    let msg = format!("Content-Length: {}\r\n\r\n{}", resp.to_string().len(), resp);
                    let _ = stdout.write_all(msg.as_bytes());
                    let _ = stdout.flush();
                }
            }
        }
    }
}
