#![allow(clippy::all)]
//! Dalin L Language Server -- LSP 3.17 full implementation (lsp-types refactored)
//!
//! Protocol: JSON-RPC over stdio
//! Capabilities: diagnostics, hover, completion, signatureHelp, definition, references, rename

use dalin_compiler::ast::{Program, Stmt};
use dalin_compiler::lexer;
use dalin_compiler::parser;
use dalin_compiler::ty2::SevenChannelInferencer;
use lsp_types::{
    CompletionOptions, HoverProviderCapability, InitializeResult, MarkupContent, OneOf, Position, Range, RenameOptions, ServerCapabilities, SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Read, Write};

// ---------------------------------------------------------------------------
// LSP JSON-RPC helpers (manual framing)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

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
            break; // Header end
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

fn send_response(stdout: &mut std::io::Stdout, resp: &Value) {
    let msg = format!("Content-Length: {}\r\n\r\n{}", resp.to_string().len(), resp);
    let _ = stdout.write_all(msg.as_bytes());
    let _ = stdout.flush();
}

fn send_notification(stdout: &mut std::io::Stdout, notif: &Value) {
    let msg = format!(
        "Content-Length: {}\r\n\r\n{}",
        notif.to_string().len(),
        notif
    );
    let _ = stdout.write_all(msg.as_bytes());
    let _ = stdout.flush();
}

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
        self.documents.insert(uri.to_string(), (version, content.to_string()));
    }

    fn change(&mut self, uri: &str, version: i32, content: &str) {
        self.documents.insert(uri.to_string(), (version, content.to_string()));
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
// Compiler Wrapper — caches Program for completion reuse
// ---------------------------------------------------------------------------

struct LspCompiler {
    doc_manager: DocumentManager,
    last_diagnostics: HashMap<String, Vec<Value>>,
    last_program: HashMap<String, Program>,
}

impl LspCompiler {
    fn new() -> Self {
        Self {
            doc_manager: DocumentManager::new(),
            last_diagnostics: HashMap::new(),
            last_program: HashMap::new(),
        }
    }

    fn compile_file(&mut self, uri: &str) -> Vec<Value> {
        let content = match self.doc_manager.get_content(uri) {
            Some(c) => c.to_string(),
            None => return vec![],
        };

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

        // Cache the program for completion/hover reuse
        self.last_program.insert(uri.to_string(), prog.clone());

        // Step 3: Seven-channel type inference
        let mut infer = SevenChannelInferencer::new();
        infer.infer_program(&prog);

        let mut diags = Vec::new();
        self.collect_errors_to_diags(&mut diags, &infer.effect.errors, "效应违规", "E001");
        self.collect_errors_to_diags(&mut diags, &infer.capability.errors, "能力违规", "E002");
        self.collect_errors_to_diags(&mut diags, &infer.confidence.errors, "置信度不足", "E005");
        self.collect_errors_to_diags(&mut diags, &infer.cognitive_loop.errors, "认知循环违规", "E006");
        self.collect_errors_to_diags(&mut diags, &infer.governance.errors, "治理违规", "E007");
        self.collect_errors_to_diags(&mut diags, &infer.time_constraint.errors, "延迟/超时违规", "E008");

        self.last_diagnostics.insert(uri.to_string(), diags.clone());
        diags
    }

    fn collect_errors_to_diags(&self, diags: &mut Vec<Value>, errors: &[String], prefix: &str, _code: &str) {
        for err in errors {
            let msg = format!("{}: {}", prefix, err);
            let line = extract_line(&msg);
            diags.push(json_diagnostic(&msg, 1, line, 0, line, err.len().min(40)));
        }
    }

    fn get_cached_program(&self, uri: &str) -> Option<&Program> {
        self.last_program.get(uri)
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
    if let Some(start) = error_msg.find('[') {
        if let Some(end) = error_msg.find(':') {
            if start + 1 < end {
                return error_msg[start + 1..end].parse().unwrap_or(1);
            }
        }
    }
    1
}

fn json_diagnostic(msg: &str, severity: u32, start_line: usize, start_char: usize, end_line: usize, end_char: usize) -> Value {
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

// ---------------------------------------------------------------------------
// Keyword / Annotation descriptions for hover
// ---------------------------------------------------------------------------

const KEYWORD_DESCRIPTIONS: &[(&str, &str)] = &[
    ("fn", "函数声明关键字 — 定义一个 Dalin L 函数"),
    ("let", "变量绑定关键字 — 不可变绑定"),
    ("return", "返回值关键字 — 从函数返回"),
    ("if", "条件分支关键字 — 条件表达式"),
    ("else", "条件分支关键字 — 可选的 else 分支"),
    ("match", "模式匹配关键字 — 多分支匹配"),
    ("for", "循环关键字 — for-in 迭代"),
    ("in", "范围/迭代关键字 — 配合 for 使用"),
    ("while", "循环关键字 — while 条件循环"),
    ("spawn", "并发关键字 — 启动后台协程"),
    ("async", "异步关键字 — 声明异步函数"),
    ("try", "异常处理关键字 — try 语句块"),
    ("catch", "异常捕获关键字 — catch 参数绑定"),
    ("use", "导入关键字 — 引入模块/类型"),
    ("trait", "特征声明关键字 — trait 定义"),
    ("assert", "断言关键字 — 运行时检查"),
    ("channel", "通道关键字 — 创建 send/recv 通道"),
    ("mut", "可变性关键字 — 可变绑定修饰"),
    ("ok", "Result Ok 构造函数 — 成功值包装"),
    ("error", "Result Error 构造函数 — 失败值包装"),
    ("export", "导出关键字 — 公开符号"),
    ("pub", "可见性关键字 — 公开成员"),
    ("impl", "实现关键字 — trait/类型实现"),
    ("struct", "结构体关键字 — 定义结构体"),
    ("enum", "枚举关键字 — 定义枚举"),
    ("type", "类型别名关键字 — type = ..."),
    ("const", "常量关键字 — 编译时常量定义"),
    ("mod", "模块关键字 — 声明子模块"),
];

const ANNOTATION_DESCRIPTIONS: &[(&str, &str)] = &[
    ("@pure", "纯函数标注 — 无副作用，仅依赖输入输出"),
    ("@io", "I/O 能力标注 — 包含文件/网络/终端 I/O 操作"),
    ("@async", "异步能力标注 — 异步非阻塞执行"),
    ("@spawn", "并发能力标注 — 可后台执行"),
    ("@cpu", "计算能力标注 — CPU 密集型任务"),
    ("@gpu", "GPU 能力标注 — GPU 并行计算"),
    ("@sfa", "SFA 能力标注 — 专用加速单元"),
    ("@net", "网络能力标注 — 网络请求与响应"),
    ("@proven", "可信级 — 代码经过形式化验证"),
    ("@verified", "验证级 — 通过编译器验证"),
    ("@inferred", "推断级 — 由类型推断器确认"),
    ("@generated", "生成级 — 由代码生成器产出"),
    ("@uncertain", "不确定级 — 置信度较低"),
    ("@perceive", "感知阶段 — 数据采集与理解"),
    ("@reason", "推理阶段 — 分析与决策推理"),
    ("@decide", "决策阶段 — 选择行动方案"),
    ("@act", "行动阶段 — 执行决策"),
    ("@loop", "认知循环 — 完整感知-推理-决策-行动闭环"),
    ("@gov(none)", "治理级别 0 — 无需审批"),
    ("@gov(prepare)", "治理级别 1 — 准备阶段"),
    ("@gov(approve)", "治理级别 2 — 需要审批"),
    ("@gov(execute)", "治理级别 3 — 执行阶段"),
];

fn find_keyword_description(word: &str) -> Option<&'static str> {
    KEYWORD_DESCRIPTIONS.iter().find(|(k, _)| *k == word).map(|(_, desc)| *desc)
}

fn find_annotation_description(word: &str) -> Option<&'static str> {
    ANNOTATION_DESCRIPTIONS.iter().find(|(k, _)| *k == word).map(|(_, desc)| *desc)
}

// ---------------------------------------------------------------------------
// Completion Engine — AST-aware with prefix filtering
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
                "let".into(), "fn".into(), "return".into(), "if".into(), "else".into(),
                "match".into(), "for".into(), "in".into(), "while".into(), "spawn".into(),
                "async".into(), "try".into(), "catch".into(), "use".into(), "trait".into(),
                "assert".into(), "channel".into(), "mut".into(), "ok".into(), "error".into(),
                "export".into(), "pub".into(), "impl".into(), "struct".into(), "enum".into(),
                "type".into(), "const".into(), "mod".into(),
            ],
        }
    }

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
                Stmt::StructDef { name, .. } => { self.defined_identifiers.insert(name.clone()); }
                Stmt::EnumDef { name, .. } => { self.defined_identifiers.insert(name.clone()); }
                Stmt::TraitDef { name, .. } => { self.defined_identifiers.insert(name.clone()); }
                _ => {}
            }
        }
    }

    /// Extract the word at cursor position for prefix matching.
    fn extract_word_at(content: &str, cursor_pos: usize) -> String {
        let bytes = content.as_bytes();
        let pos = if cursor_pos <= bytes.len() { cursor_pos } else { bytes.len() };
        let mut start = pos;
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            start -= 1;
        }
        if start == pos {
            String::new()
        } else {
            String::from_utf8_lossy(&bytes[start..pos]).to_string()
        }
    }

    /// Convert LSP Position (u64 line/char) to byte offset in content.
    fn position_to_byte_offset(content: &str, pos: &Position) -> usize {
        let line_idx = pos.line as usize;
        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() { return content.len(); }
        let current_line = lines[line_idx];
        let mut char_count = 0;
        for ch in current_line.chars() {
            if char_count >= pos.character as usize { break; }
            char_count += ch.len_utf8();
        }
        let mut offset: usize = 0;
        for i in 0..line_idx {
            offset += lines[i].len() + 1;
        }
        offset + char_count
    }

    fn provide_completions(&self, current_text: &str, cursor_pos: usize) -> Vec<Value> {
        let prefix = Self::extract_word_at(current_text, cursor_pos);
        let prefix_lower = prefix.to_lowercase();
        let mut items = Vec::new();

        // Keywords — filter by prefix
        for kw in &self.keywords {
            if !kw.is_empty() && (prefix_lower.is_empty() || kw.to_lowercase().starts_with(&prefix_lower)) {
                let relevance = if kw.to_lowercase() == prefix_lower { "0" } else { "1" };
                items.push(json!({
                    "label": kw,
                    "kind": 14u32, // KEYWORD
                    "detail": format!("关键字: {}", kw),
                    "sortText": format!("{}{}", relevance, kw),
                }));
            }
        }

        // Filtered identifier completions
        for id in &self.defined_identifiers {
            if id.is_empty() { continue; }
            if prefix_lower.is_empty() || id.to_lowercase().starts_with(&prefix_lower) {
                let exact_match = id.to_lowercase() == prefix_lower;
                let item_kind = if id.starts_with(|c: char| c.is_uppercase()) { 7u32 } else { 6u32 };
                items.push(json!({
                    "label": id,
                    "kind": item_kind,
                    "detail": "已定义标识符",
                    "sortText": format!("{}{}", if exact_match { "0" } else { "1" }, id.to_lowercase()),
                }));
            }
        }

        // @ attributes
        let attrs = [
            "@pure", "@io", "@async", "@spawn", "@cpu", "@gpu", "@sfa", "@net",
            "@proven", "@verified", "@inferred", "@generated", "@uncertain",
            "@latency(ms)", "@timeout(s)", "@throughput(/s)",
            "@perceive", "@reason", "@decide", "@act", "@loop",
            "@gov(none)", "@gov(prepare)", "@gov(approve)", "@gov(execute)",
        ];
        for attr in attrs {
            if prefix.is_empty() || attr.to_lowercase().starts_with(&prefix_lower) {
                items.push(json!({
                    "label": attr,
                    "kind": 15u32, // SNIPPET
                    "detail": "七通道标注",
                    "sortText": format!("20_{}", attr),
                }));
            }
        }

        items.sort_by(|a, b| {
            let sort_a = a["sortText"].as_str().unwrap_or("");
            let sort_b = b["sortText"].as_str().unwrap_or("");
            sort_a.cmp(sort_b)
        });

        items
    }
}

// ---------------------------------------------------------------------------
// Hover Provider
// ---------------------------------------------------------------------------

struct HoverProvider;

impl HoverProvider {
    fn provide_hover(&self, content: &str, line: usize, character: usize, defined_ids: &HashSet<String>) -> Option<Value> {
        let lines: Vec<&str> = content.lines().collect();
        if line >= lines.len() || character >= lines[line].chars().count() {
            return None;
        }

        let current_line = lines[line];
        let chars: Vec<char> = current_line.chars().collect();
        let cursor = character;
        let mut start = cursor;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_alphanumeric() || c == '_' { start -= 1; } else { break; }
        }
        let mut end = cursor;
        while end < chars.len() {
            let c = chars[end];
            if c.is_alphanumeric() || c == '_' { end += 1; } else { break; }
        }
        if start == end { return None; }
        let word = &current_line[start..end];

        // Check for seven-channel annotation
        if word.starts_with('@') {
            if let Some(desc) = find_annotation_description(word) {
                return Some(json!({
                    "contents": MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: format!("### 七通道标注: `{}`\n\n**描述**: {}\n\n**有效值**:\n- 能力: `@pure`, `@io`, `@async`, `@spawn`, `@cpu`, `@gpu`, `@sfa`, `@net`\n- 置信度: `@proven`, `@verified`, `@inferred`, `@generated`, `@uncertain`\n- 认知循环: `@perceive`, `@reason`, `@decide`, `@act`, `@loop`\n- 治理: `@gov(none)`, `@gov(prepare)`, `@gov(approve)`, `@gov(execute)`", word, desc),
                    }
                }));
            }
            return Some(json!({
                "contents": MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!("### 七通道标注: `{}`\n\n未识别的标注。", word),
                }
            }));
        }

        // Keywords
        if let Some(desc) = find_keyword_description(word) {
            return Some(json!({
                "contents": MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!("### 关键字: `{}`\n\n**描述**: {}", word, desc),
                }
            }));
        }

        // Identifiers found in source
        if defined_ids.contains(word) {
            return Some(json!({
                "contents": MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!("### 标识符: `{}`\n\n在当前文件中定义。", word),
                }
            }));
        }

        Some(json!({
            "contents": MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: format!("### 标识符: `{}`\n\n未在当前文件中找到定义。", word),
            }
        }))
    }
}

// ---------------------------------------------------------------------------
// Definition Provider
// ---------------------------------------------------------------------------

struct DefinitionProvider;

impl DefinitionProvider {
    fn word_at_position(content: &str, line: usize, character: usize) -> Option<(String, usize, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        if line >= lines.len() { return None; }
        let current_line = lines[line];
        let chars: Vec<char> = current_line.chars().collect();
        if character >= chars.len() { return None; }
        let cursor = character;
        let mut start = cursor;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_alphanumeric() || c == '_' || c == '@' { start -= 1; } else { break; }
        }
        let mut end = cursor;
        while end < chars.len() {
            let c = chars[end];
            if c.is_alphanumeric() || c == '_' { end += 1; } else { break; }
        }
        if start == end { return None; }
        Some((chars[start..end].iter().collect(), start, end))
    }

    fn find_definition(&self, content: &str, line: usize, character: usize) -> Option<Range> {
        let (word, _, _) = Self::word_at_position(content, line, character)?;
        let lines: Vec<&str> = content.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let trimmed = l.trim();
            if trimmed.starts_with("fn ") {
                let after_fn = &trimmed[3..];
                if let Some(paren) = after_fn.find('(') {
                    let fn_name = after_fn[..paren].trim();
                    if fn_name == word {
                        let col = l.find(fn_name).unwrap_or(0);
                        return Some(Range {
                            start: Position { line: i as u32, character: col as u32 },
                            end: Position { line: i as u32, character: (col + fn_name.len()) as u32 },
                        });
                    }
                }
            }
            if trimmed.starts_with("let ") {
                let rest = &trimmed[4..];
                let var_name = rest.split(|c: char| c == '=' || c == ':').next().unwrap_or("").trim();
                if var_name == word {
                    let col = l.find(var_name).unwrap_or(0);
                    return Some(Range {
                        start: Position { line: i as u32, character: col as u32 },
                        end: Position { line: i as u32, character: (col + var_name.len()) as u32 },
                    });
                }
            }
            if trimmed.starts_with("const ") {
                let rest = &trimmed[6..];
                let var_name = rest.split(|c: char| c == '=' || c == ':').next().unwrap_or("").trim();
                if var_name == word {
                    let col = l.find(var_name).unwrap_or(0);
                    return Some(Range {
                        start: Position { line: i as u32, character: col as u32 },
                        end: Position { line: i as u32, character: (col + var_name.len()) as u32 },
                    });
                }
            }
        }
        None
    }

    fn find_references(&self, content: &str, line: usize, character: usize) -> Vec<Range> {
        let (word, _, _) = match Self::word_at_position(content, line, character) {
            Some(w) => w, None => return vec![],
        };
        let lines: Vec<&str> = content.lines().collect();
        let mut refs = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let mut search_start = 0usize;
            while let Some(pos) = l[search_start..].find(&word) {
                let abs_pos = search_start + pos;
                let before_ok = abs_pos == 0 || {
                    let c = l.as_bytes().get(abs_pos - 1).copied().unwrap_or(b' ');
                    !c.is_ascii_alphanumeric() && c != b'_'
                };
                let after_ok = {
                    let c = l.as_bytes().get(abs_pos + word.len()).copied().unwrap_or(b' ');
                    !c.is_ascii_alphanumeric() && c != b'_'
                };
                if before_ok && after_ok {
                    refs.push(Range {
                        start: Position { line: i as u32, character: abs_pos as u32 },
                        end: Position { line: i as u32, character: (abs_pos + word.len()) as u32 },
                    });
                }
                search_start = abs_pos + 1;
                if search_start >= l.len() { break; }
            }
        }
        refs
    }
}

// ---------------------------------------------------------------------------
// Signature Help Provider
// ---------------------------------------------------------------------------

struct SignatureHelpProvider;

impl SignatureHelpProvider {
    fn gather_signatures(content: &str) -> Vec<(String, Vec<String>)> {
        let mut sigs = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") {
                let after_fn = &trimmed[3..];
                if let Some(paren_start) = after_fn.find('(') {
                    let func_name = after_fn[..paren_start].trim();
                    if let Some(paren_end) = after_fn.rfind(')') {
                        let params_str = &after_fn[paren_start + 1..paren_end];
                        let params: Vec<String> = params_str
                            .split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        sigs.push((func_name.to_string(), params));
                    }
                }
            }
        }
        sigs
    }

    fn find_active_call(content: &str, line: usize, character: usize) -> Option<(String, usize)> {
        let lines: Vec<&str> = content.lines().collect();
        if line >= lines.len() { return None; }
        let current_line = lines[line];
        let clamped = if character <= current_line.len() {
            &current_line[..character]
        } else {
            current_line
        };
        let paren_pos = clamped.rfind('(')?;
        let before_paren = clamped[..paren_pos].trim();
        let func_name = before_paren.split_whitespace().last()?;
        let after_paren = &clamped[paren_pos + 1..];
        let active_param = after_paren.matches(',').count();
        Some((func_name.to_string(), active_param))
    }

    fn provide_signature_help(&self, content: &str, line: usize, character: usize) -> Option<Value> {
        let all_sigs = Self::gather_signatures(content);
        if all_sigs.is_empty() { return None; }

        let (active_func, active_param) =
            Self::find_active_call(content, line, character).unwrap_or((String::new(), 0));

        let signatures: Vec<Value> = all_sigs.iter().map(|(name, params)| {
            json!({
                "label": format!("{}({})", name, params.join(", ")),
                "documentation": format!("函数: {}", name),
                "parameters": params.iter().map(|p| json!({"label": p})).collect::<Vec<_>>(),
            })
        }).collect();

        let active_sig = if !active_func.is_empty() {
            all_sigs.iter().position(|(n, _)| n == &active_func).unwrap_or(0) as u32
        } else {
            0
        };

        Some(json!({
            "activeSignature": active_sig,
            "activeParameter": active_param.min(signatures.len()),
            "signatures": signatures,
        }))
    }
}

// ---------------------------------------------------------------------------
// Main LSP Server (JSON-RPC over stdio)
// ---------------------------------------------------------------------------

fn main() {
    let mut compiler = LspCompiler::new();
    let mut completion_engine = CompletionEngine::new();
    let hover_provider = HoverProvider;
    let signature_helper = SignatureHelpProvider;
    let definition_provider = DefinitionProvider;

    let mut reader = BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    let mut shutdown_received = false;

    while let Some(line) = read_lsp_message(&mut reader) {
        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&line) {
            let method = req.method.as_str();

            match method {
                // --- Initialization ---
                "initialize" => {
                    let server_capabilities = ServerCapabilities {
                        text_document_sync: Some(TextDocumentSyncCapability::Options(
                            TextDocumentSyncOptions {
                                open_close: Some(true),
                                change: Some(TextDocumentSyncKind::INCREMENTAL),
                                will_save: None,
                                will_save_wait_until: None,
                                save: None,
                            },
                        )),
                        hover_provider: Some(HoverProviderCapability::Simple(true)),
                        completion_provider: Some(CompletionOptions {
                            trigger_characters: Some(vec![
                                ".".into(), "(".into(), " ".into(), "@".into(), "#".into()
                            ]),
                            all_commit_characters: None,
                            resolve_provider: Some(false),
                            completion_item: None,
                            work_done_progress_options: Default::default(),
                        }),
                        signature_help_provider: Some(SignatureHelpOptions {
                            trigger_characters: Some(vec!["(".into(), ",".into()]),
                            retrigger_characters: None,
                            work_done_progress_options: Default::default(),
                        }),
                        definition_provider: Some(OneOf::Left(true)),
                        references_provider: Some(OneOf::Left(true)),
                        rename_provider: Some(OneOf::Left(true)),
                        document_highlight_provider: None,
                        document_symbol_provider: None,
                        workspace_symbol_provider: None,
                        code_action_provider: None,
                        code_lens_provider: None,
                        document_formatting_provider: None,
                        document_range_formatting_provider: None,
                        document_on_type_formatting_provider: None,
                        document_link_provider: None,
                        color_provider: None,
                        folding_range_provider: None,
                        declaration_provider: None,
                        implementation_provider: None,
                        selection_range_provider: None,
                        call_hierarchy_provider: None,
                        type_definition_provider: None,
                        execute_command_provider: None,
                        inline_value_provider: None,
                        inlay_hint_provider: None,
                        diagnostic_provider: None,
                        moniker_provider: None,
                        linked_editing_range_provider: None,
                        workspace: None,
                        semantic_tokens_provider: None,
                        experimental: None,
                        position_encoding: None,
                    };

                    let result = InitializeResult {
                        capabilities: server_capabilities,
                        server_info: None,
                    };

                    let result_value = serde_json::to_value(&result).unwrap_or(json!(null));
                    let resp = json!({
                        "jsonrpc": "2.0",
                        "id": req.id,
                        "result": result_value,
                    });
                    send_response(&mut stdout, &resp);
                }

                "initialized" => {
                    let resp = json!({"jsonrpc": "2.0"});
                    send_response(&mut stdout, &resp);
                }

                // --- Document Lifecycle ---
                "textDocument/didOpen" => {
                    let params = req.params.as_ref();
                    if let Some(text_doc) = params.and_then(|p| p.get("textDocument")) {
                        let uri = text_doc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let text = text_doc.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        let version = text_doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as i32;

                        compiler.doc_manager.open(uri, version, text);
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
                    let params = req.params.as_ref();
                    if let Some(text_doc) = params.and_then(|p| p.get("textDocument")) {
                        let uri = text_doc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                        let version = text_doc.get("version").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                        let changes = match params.and_then(|p| p.get("contentChanges")).and_then(|c| c.as_array()) {
                            Some(c) => c,
                            None => &vec![],
                        };

                        if let Some(last_change) = changes.last() {
                            let text = last_change.get("text").and_then(|t| t.as_str()).unwrap_or("");
                            compiler.doc_manager.change(uri, version, text);
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
                    let params = req.params.as_ref();
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    compiler.doc_manager.close(uri);
                    compiler.last_diagnostics.remove(uri);
                }

                // --- Code Completions ---
                "textDocument/completion" => {
                    let text_doc = match req.params.as_ref().and_then(|p| p.get("textDocument")) {
                        Some(d) => d, None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": json!([])}));
                            continue;
                        }
                    };
                    let uri = text_doc.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                    let position = text_doc.get("position");
                    let line = position.as_ref().and_then(|l| l.get("line")).and_then(|l| l.as_u64()).unwrap_or(0);
                    let character = position.as_ref().and_then(|c| c.get("character")).and_then(|c| c.as_u64()).unwrap_or(0);

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": json!([])}));
                            continue;
                        }
                    };

                    // Recompile to update diagnostics and cached Program
                    let _ = compiler.compile_file(uri);

                    // Populate completion engine from cached AST
                    if let Some(prog) = compiler.get_cached_program(uri) {
                        completion_engine.populate_from_ast(prog);
                    }

                    let byte_offset = CompletionEngine::position_to_byte_offset(&content, &Position { line: line as u32, character: character as u32 });
                    let completions = completion_engine.provide_completions(&content, byte_offset);
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": completions}));
                }

                // --- Hover ---
                "textDocument/hover" => {
                    let params = req.params.as_ref();
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    let position = text_doc.and_then(|d| d.get("position"));
                    let line = position.as_ref().and_then(|l| l.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                    let character = position.as_ref().and_then(|c| c.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": null}));
                            continue;
                        }
                    };

                    let mut defined_ids = HashSet::new();
                    if let Some(prog) = compiler.get_cached_program(uri) {
                        completion_engine.populate_from_ast(prog);
                        std::mem::swap(&mut defined_ids, &mut completion_engine.defined_identifiers);
                    }

                    let hover = hover_provider.provide_hover(&content, line, character, &defined_ids);
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": hover}));
                }

                // --- Go-to-Definition ---
                "textDocument/definition" => {
                    let params = req.params.as_ref();
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    let position = text_doc.and_then(|d| d.get("position"));
                    let line = position.as_ref().and_then(|l| l.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                    let character = position.as_ref().and_then(|c| c.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": null}));
                            continue;
                        }
                    };

                    let def = definition_provider.find_definition(&content, line, character);
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": def}));
                }

                // --- Find References ---
                "textDocument/references" => {
                    let params = req.params.as_ref();
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    let position = text_doc.and_then(|d| d.get("position"));
                    let line = position.as_ref().and_then(|l| l.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                    let character = position.as_ref().and_then(|c| c.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": []}));
                            continue;
                        }
                    };

                    let refs = definition_provider.find_references(&content, line, character);
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": refs}));
                }

                // --- Signature Help ---
                "textDocument/signatureHelp" => {
                    let params = req.params.as_ref();
                    let text_doc = params.and_then(|p| p.get("textDocument"));
                    let uri = text_doc.and_then(|d| d.get("uri").and_then(|u| u.as_str())).unwrap_or("");
                    let position = text_doc.and_then(|d| d.get("position"));
                    let line = position.as_ref().and_then(|l| l.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                    let character = position.as_ref().and_then(|c| c.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;

                    let content = match compiler.doc_manager.get_content(uri) {
                        Some(c) => c.to_string(),
                        None => {
                            send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": null}));
                            continue;
                        }
                    };

                    let sig_help = signature_helper.provide_signature_help(&content, line, character);
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": sig_help}));
                }

                // --- Shutdown / Exit ---
                "shutdown" => {
                    let response = json!({ "jsonrpc": "2.0", "id": req.id, "result": null });
                    writeln!(stdout, "{}", response).ok();
                    stdout.flush().ok();
                    shutdown_received = true;
                }
                "exit" => {
                    break;
                }
                _ => {
                    send_response(&mut stdout, &json!({"jsonrpc": "2.0", "id": req.id, "result": null}));
                }
            }
        }
    }
}
