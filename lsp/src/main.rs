#![allow(clippy::all)]
//! Dalin L Language Server -- LSP 3.17 full implementation
//!
//! Protocol: JSON-RPC over stdio
//! Capabilities: diagnostics, hover, completion, signatureHelp, didOpen/didChange/didClose
#![allow(clippy::all, unused)]
//!
//! Build: `cargo build --bin dalin-ls -p dalin-ls`
//! Run:   `dalin-ls`

use dalin_compiler::ast::{Program, Stmt};
use dalin_compiler::lexer;
use dalin_compiler::parser;
use dalin_compiler::ty2::SevenChannelInferencer;
use serde_json::{Value, json};

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::str;

// ---------------------------------------------------------------------------
// Document Manager (in-memory text document storage)
// ---------------------------------------------------------------------------

struct DocumentManager {
    documents: HashMap<String, (i32, String)>,
}

impl DocumentManager {
    fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    fn open(&mut self, uri: &str, version: i32, content: &str) {
        self.documents
            .insert(uri.to_string(), (version, content.to_string()));
    }

    fn change(&mut self, uri: &str, version: i32, content: &str) {
        self.documents
            .insert(uri.to_string(), (version, content.to_string()));
    }

    fn close(&mut self, uri: &str) {
        self.documents.remove(uri);
    }

    fn get_content(&self, uri: &str) -> Option<&str> {
        self.documents.get(uri).map(|(_, c)| c.as_str())
    }

    #[allow(dead_code)]
    fn get_version(&self, uri: &str) -> Option<i32> {
        self.documents.get(uri).map(|(v, _)| *v)
    }
}

// ---------------------------------------------------------------------------
// Compiler Wrapper (diagnostic engine powered by dalin-compiler)
// ---------------------------------------------------------------------------

/// Wraps the compiler pipeline for LSP use.
/// compile_file(uri) => list of diagnostic JSON objects ready for publishDiagnostics
struct LspCompiler {
    doc_manager: DocumentManager,
    last_diagnostics: HashMap<String, Vec<Value>>,
}

impl LspCompiler {
    fn new() -> Self {
        Self {
            doc_manager: DocumentManager::new(),
            last_diagnostics: HashMap::new(),
        }
    }

    fn compile_file(&mut self, uri: &str) -> Vec<Value> {
        let content = match self.doc_manager.get_content(uri) {
            Some(c) => c.to_string(),
            None => return vec![],
        };

        let stmts = extract_statements(&content);
        _ = stmts; // used for positioning, tracked via program

        // Step 1: Lexer
        let mut lex = lexer::Lexer::new(&content);
        let tokens = match lex.tokenize() {
            Ok(t) => t,
            Err(e) => {
                let err_str = e.to_string();
                let ln = extract_line(&err_str);
                let msg = format!("词法错误: {}", err_str);
                return vec![json_diagnostic(&msg, 1, ln, 0, ln, 20)];
            }
        };

        // Step 2: Parser
        let mut parser = parser::Parser::new(tokens);
        let prog = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                let err_str = e.to_string();
                let ln = extract_line(&err_str);
                let msg = format!("语法错误: {}", err_str);
                return vec![json_diagnostic(&msg, 1, ln, 0, ln, 40)];
            }
        };

        // Step 3: Seven-channel type inference
        let mut infer = SevenChannelInferencer::new();
        infer.infer_program(&prog);

        let mut diags = Vec::new();
        self.collect_errors_to_diags(&mut diags, &infer.effect.errors, "效应违规", "E001");
        self.collect_errors_to_diags(&mut diags, &infer.capability.errors, "能力违规", "E002");
        self.collect_errors_to_diags(&mut diags, &infer.confidence.errors, "置信度不足", "E005");
        self.collect_errors_to_diags(
            &mut diags,
            &infer.cognitive_loop.errors,
            "认知循环违规",
            "E006",
        );
        self.collect_errors_to_diags(&mut diags, &infer.governance.errors, "治理违规", "E007");
        self.collect_errors_to_diags(
            &mut diags,
            &infer.time_constraint.errors,
            "延迟/超时违规",
            "E008",
        );

        self.last_diagnostics.insert(uri.to_string(), diags.clone());
        diags
    }

    /// Helper: collect channel errors into diagnostic JSON objects
    fn collect_errors_to_diags(
        &self,
        diags: &mut Vec<Value>,
        errors: &[String],
        prefix: &str,
        _code: &str,
    ) {
        for err in errors {
            let msg = format!("{}: {}", prefix, err);
            let line = extract_line(&msg);
            diags.push(json_diagnostic(&msg, 1, line, 0, line, err.len().min(40)));
        }
    }

    #[allow(dead_code)]
    fn workspace_diagnostics(&mut self) -> Vec<Value> {
        let uris: Vec<String> = self.doc_manager.documents.keys().cloned().collect();
        let mut all_diags = Vec::new();
        for uri in uris {
            let diags = self.compile_file(&uri);
            all_diags.extend(diags);
        }
        all_diags
    }
}

fn extract_line(error_msg: &str) -> usize {
    // 尝试从 "[line:col]" 格式提取行号
    if let Some(start) = error_msg.find('[') {
        if let Some(end) = error_msg.find(':') {
            if start + 1 < end {
                return error_msg[start + 1..end].parse().unwrap_or(1);
            }
        }
    }
    1
}

fn json_diagnostic(
    msg: &str,
    severity: u32,
    start_line: usize,
    start_char: usize,
    end_line: usize,
    end_char: usize,
) -> Value {
    json!({
        "range": {
            "start": { "line": start_line - 1, "character": start_char },
            "end": { "line": end_line - 1, "character": end_char },
        },
        "severity": severity,
        "message": msg.to_string(),
        "source": "dalin-ls".to_string(),
    })
}

fn extract_statements(content: &str) -> Vec<Stmt> {
    // For simple positioning we just count lines; detailed extraction isn't needed yet
    content
        .lines()
        .filter(|l| l.contains("fn ") || l.contains("let "))
        .count();
    Vec::new()
}

// ---------------------------------------------------------------------------
// Completion Engine
// ---------------------------------------------------------------------------

struct CompletionEngine {
    defined_identifiers: HashSet<String>,
    keywords: Vec<String>,
}

impl CompletionEngine {
    fn new() -> Self {
        Self {
            defined_identifiers: HashSet::new(),
            keywords: vec![
                "let".into(),
                "fn".into(),
                "return".into(),
                "if".into(),
                "else".into(),
                "match".into(),
                "for".into(),
                "in".into(),
                "while".into(),
                "spawn".into(),
                "async".into(),
                "try".into(),
                "catch".into(),
                "use".into(),
                "trait".into(),
                "assert".into(),
                "channel".into(),
                "mut".into(),
                "ok".into(),
                "error".into(),
                "export".into(),
                "pub".into(),
                "impl".into(),
                "struct".into(),
                "enum".into(),
                "type".into(),
                "const".into(),
                "mod".into(),
            ],
        }
    }

    #[allow(dead_code)]
    fn populate_from_ast(&mut self, prog: &Program) {
        for stmt in &prog.statements {
            match stmt {
                Stmt::Let { name, .. } | Stmt::Const { name, .. } => {
                    self.defined_identifiers.insert(name.clone());
                }
                Stmt::Fn { name, params, .. } => {
                    self.keywords.push(name.clone());
                    for param in params {
                        self.defined_identifiers.insert(param.name.clone());
                    }
                }
                Stmt::StructDef { .. } => {}
                Stmt::EnumDef { .. } => {}
                _ => {}
            }
        }
    }

    fn provide_completions(&self, current_text: &str, _cursor_pos: usize) -> Vec<Value> {
        let mut items = Vec::new();

        // Keywords
        for kw in &self.keywords {
            if !kw.is_empty() && !current_text.ends_with('_') {
                items.push(json!({
                    "label": kw,
                    "kind": 14,  // Keyword
                    "detail": format!("关键字: {}", kw),
                    "sortText": format!("00_{}", kw),
                }));
            }
        }

        // Identifiers
        for id in &self.defined_identifiers {
            items.push(json!({
                "label": id,
                "kind": 10,  // Variable
                "detail": "已定义标识符",
                "sortText": format!("10_{}", id),
            }));
        }

        // @ attributes (seven-channel annotations)
        let attrs = [
            "@pure",
            "@io",
            "@async",
            "@spawn",
            "@cpu",
            "@gpu",
            "@sfa",
            "@net",
            "@proven",
            "@verified",
            "@inferred",
            "@generated",
            "@uncertain",
            "@latency(ms)",
            "@timeout(s)",
            "@throughput(/s)",
            "@perceive",
            "@reason",
            "@decide",
            "@act",
            "@loop",
            "@gov(none)",
            "@gov(prepare)",
            "@gov(approve)",
            "@gov(execute)",
        ];
        for attr in attrs {
            items.push(json!({
                "label": attr,
                "kind": 15,  // Snippet
                "detail": "七通道标注",
                "sortText": format!("20_{}", attr),
            }));
        }

        items
    }
}

// ---------------------------------------------------------------------------
// Hover Provider
// ---------------------------------------------------------------------------

struct HoverProvider;

impl HoverProvider {
    fn provide_hover(&self, content: &str, line: usize, character: usize) -> Option<Value> {
        let lines: Vec<&str> = content.lines().collect();
        if line >= lines.len() {
            return None;
        }

        let current_line = lines[line];
        let word_start = current_line[..character]
            .rfind(|c: char| c.is_alphanumeric() || c == '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let word_end = current_line[character..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + character)
            .unwrap_or(character);

        if word_start == word_end {
            return None;
        }

        let word = &current_line[word_start..word_end];

        // Check for seven-channel annotation
        if word.starts_with("@") {
            return Some(json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("### 七通道标注: `{}`\n\n有效值: `@pure`, `@io`, `@async`, `@spawn`\n能力: `@cpu`, `@gpu`, `@sfa`, `@net`\n置信度: `@proven`, `@verified`, `@inferred`, `@generated`, `@uncertain`\n治理: `@gov(none)`, `@gov(prepare)`, `@gov(approve)`, `@gov(execute)`", word),
                },
            }));
        }

        // Keywords
        let keywords = [
            "fn", "let", "return", "if", "else", "match", "for", "in", "while", "spawn", "async",
            "try", "catch", "use", "trait", "assert", "channel", "mut", "ok", "error", "export",
            "pub", "impl", "struct", "enum", "type", "const", "mod",
        ];
        if keywords.contains(&word) {
            return Some(json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("### 关键字: `{}`\n\n这是 Dalin L 的语言保留字。", word),
                },
            }));
        }

        // Plain identifier
        Some(json!({
            "contents": {
                "kind": "markdown",
                "value": format!("### 标识符: `{}`", word),
            },
        }))
    }
}

// ---------------------------------------------------------------------------
// Signature Help Provider
// ---------------------------------------------------------------------------

struct SignatureHelpProvider;

impl SignatureHelpProvider {
    fn provide_signature_help(&self, content: &str) -> Option<Value> {
        let lines: Vec<&str> = content.lines().collect();
        let mut signatures = Vec::new();

        for line in &lines {
            if line.trim().starts_with("fn ") {
                let trimmed = line.trim();
                if let Some(paren_start) = trimmed.find('(') {
                    let after_fn = &trimmed[3..paren_start];
                    let func_name = after_fn.trim();

                    if let Some(paren_end) = trimmed.rfind(')') {
                        let params_str = &trimmed[paren_start + 1..paren_end];
                        let params: Vec<String> = params_str
                            .split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        let sig = format!("{}({})", func_name, params.join(", "));
                        signatures.push(json!({
                            "label": sig,
                            "parameters": params.iter().map(|p| json!({"label": p})).collect::<Vec<_>>(),
                        }));
                    }
                }
            }
        }

        if !signatures.is_empty() {
            return Some(json!({
                "activeSignature": 0,
                "activeParameter": 0,
                "signatures": signatures,
            }));
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Main LSP Server (JSON-RPC over stdio)
// ---------------------------------------------------------------------------

/// Reads a complete LSP message from stdin, parsing the Content-Length header
/// and reading exactly the specified number of bytes.
fn read_lsp_message(reader: &mut BufReader<std::io::Stdin>) -> Option<String> {
    let mut header = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        header.clear();
        if reader.read_line(&mut header).ok()? == 0 {
            return None; // EOF
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break; // 空行 = header 结束
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.trim().parse::<usize>().ok();
        }
    }

    let len = content_length?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).ok()?;
    Some(String::from_utf8(buf).ok()?)
}

fn main() {
    let mut compiler = LspCompiler::new();
    let completion_engine = CompletionEngine::new();
    let hover_provider = HoverProvider;
    let signature_helper = SignatureHelpProvider;

    let mut reader = BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    let mut shutdown_received = false;

    // Read LSP messages from stdin using standard Content-Length framing
    while let Some(line) = read_lsp_message(&mut reader) {
        // Parse JSON-RPC request
        if let Ok(req) = serde_json::from_str::<Value>(&line) {
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

            match method {
                // --- Initialization ---
                "initialize" => {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req.get("id"),
                        "result": {
                            "capabilities": {
                                "textDocumentSync": 1,  // Incremental sync
                                "hoverProvider": true,
                                "completionProvider": {
                                    "triggerCharacters": [".", "(", " ", "@", "#"]
                                },
                                "diagnosticProvider": {
                                    "interFileDependencies": false,
                                    "workspaceDiagnostics": true
                                },
                                "signatureHelpProvider": {
                                    "triggerCharacters": ["(", ","]
                                },
                            }
                        }
                    });
                    send_response(&mut stdout, &resp);
                }

                "initialized" => {
                    // Server acknowledged initialization
                    let resp = json!({"jsonrpc": "2.0"});
                    send_response(&mut stdout, &resp);
                }

                // --- Document Lifecycle ---
                "textDocument/didOpen" => {
                    let params = req.get("params").and_then(|p| p.get("textDocument"));
                    if let Some(doc) = params {
                        let uri = doc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let text = doc.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        let version =
                            doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as i32;

                        compiler.doc_manager.open(uri, version, text);

                        // Auto-push diagnostics
                        let diags = compiler.compile_file(uri);
                        let notify = json!({
                            "jsonrpc": "2.0",
                            "method": "textDocument/publishDiagnostics",
                            "params": {"uri": uri, "diagnostics": diags},
                        });
                        send_notification(&mut stdout, &notify);
                    }
                }

                "textDocument/didChange" => {
                    let params = req.get("params").and_then(|p| p.get("textDocument"));
                    if let Some(doc) = params {
                        let uri = doc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let changes = req
                            .get("params")
                            .and_then(|p| p.get("contentChanges"))
                            .and_then(|c| c.as_array());
                        let changes = match changes {
                            Some(c) => c,
                            None => &vec![],
                        };

                        if let Some(last_change) = changes.last() {
                            let text = last_change
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("");
                            let version =
                                doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                            compiler.doc_manager.change(uri, version, text);

                            // Push updated diagnostics
                            let diags = compiler.compile_file(uri);
                            let notify = json!({
                                "jsonrpc": "2.0",
                                "method": "textDocument/publishDiagnostics",
                                "params": {"uri": uri, "diagnostics": diags},
                            });
                            send_notification(&mut stdout, &notify);
                        }
                    }
                }

                "textDocument/didClose" => {
                    let params = req.get("params");
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc
                        .and_then(|d| d.get("uri").and_then(|u| u.as_str()))
                        .unwrap_or("");
                    compiler.doc_manager.close(uri);
                    compiler.last_diagnostics.remove(uri);
                }

                // --- Code Completions ---
                "textDocument/completion" => {
                    let params = req.get("params");
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc
                        .and_then(|d| d.get("uri").and_then(|u| u.as_str()))
                        .unwrap_or("");
                    let _position = text_doc.and_then(|d| d.get("position"));

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(
                                &mut stdout,
                                &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": json!([])}),
                            );
                            continue;
                        }
                    };

                    // Recompile to update completions
                    let _ = compiler.compile_file(uri);

                    let completions = completion_engine.provide_completions(&content, 0);
                    send_response(
                        &mut stdout,
                        &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": completions}),
                    );
                }

                // --- Hover ---
                "textDocument/hover" => {
                    let params = req.get("params");
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc
                        .and_then(|d| d.get("uri").and_then(|u| u.as_str()))
                        .unwrap_or("");
                    let position = text_doc.and_then(|d| d.get("position"));
                    let line = position
                        .as_ref()
                        .and_then(|l| l.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as usize;
                    let character = position
                        .as_ref()
                        .and_then(|c| c.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as usize;

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(
                                &mut stdout,
                                &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": null}),
                            );
                            continue;
                        }
                    };

                    let hover = hover_provider.provide_hover(&content, line, character);
                    let result_value = hover.map(|h| h);
                    send_response(
                        &mut stdout,
                        &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": result_value}),
                    );
                }

                // --- Signature Help ---
                "textDocument/signatureHelp" => {
                    let params = req.get("params");
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc
                        .and_then(|d| d.get("uri").and_then(|u| u.as_str()))
                        .unwrap_or("");

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(
                                &mut stdout,
                                &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": null}),
                            );
                            continue;
                        }
                    };

                    let sig_help = signature_helper.provide_signature_help(&content);
                    send_response(
                        &mut stdout,
                        &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": sig_help}),
                    );
                }

                // --- Shutdown / Exit ---
                "shutdown" => {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": req["id"],
                        "result": null
                    });
                    writeln!(stdout, "{}", response).ok();
                    stdout.flush().ok();
                    shutdown_received = true;
                }
                "exit" => {
                    break; // 退出主循环
                }

                // --- Unknown methods ---
                _ => {
                    send_response(
                        &mut stdout,
                        &json!({"jsonrpc": "2.0", "id": req.get("id"), "result": null}),
                    );
                }
            }
        }
    }
}

/// Send a response back to the LSP client via stdout
fn send_response(stdout: &mut std::io::Stdout, resp: &Value) {
    let msg = format!("Content-Length: {}\r\n\r\n{}", resp.to_string().len(), resp);
    let _ = stdout.write_all(msg.as_bytes());
    let _ = stdout.flush();
}

/// Send a notification back to the LSP client
fn send_notification(stdout: &mut std::io::Stdout, notif: &Value) {
    let msg = format!(
        "Content-Length: {}\r\n\r\n{}",
        notif.to_string().len(),
        notif
    );
    let _ = stdout.write_all(msg.as_bytes());
    let _ = stdout.flush();
}
