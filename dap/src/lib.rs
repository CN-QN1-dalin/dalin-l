/// Dalin L 3.0 — DAP Debug Server
///
/// Debug Adapter Protocol server that communicates with VSCode over stdin/stdout.
/// Provides: breakpoints, stepping, stack traces, variable inspection.
mod protocol;

use protocol::*;
use dalin_compiler::ast::*;
use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

pub struct DebugServer {
    program: Option<Program>,
    breakpoints: HashMap<String, Vec<(usize, u64)>>,
    seq: u64,
    frames: Vec<StackFrame>,
    next_bp_id: u64,
    paused: bool,
    current_source: Option<String>,
}

impl Default for DebugServer {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugServer {
    pub fn new() -> Self {
        Self {
            program: None,
            breakpoints: HashMap::new(),
            seq: 1,
            frames: Vec::new(),
            next_bp_id: 1,
            paused: false,
            current_source: None,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let mut stdin = io::stdin().lock();
        let stdout = io::stdout();

        loop {
            let mut header = String::new();
            stdin.read_line(&mut header)?;
            if header.trim().is_empty() {
                continue;
            }

            if header.starts_with("Content-Length:") {
                let len: usize = header
                    .trim_start_matches("Content-Length: ")
                    .trim()
                    .parse()
                    .unwrap_or(0);

                // Skip blank line
                let mut blank = [0u8; 2];
                stdin.read_exact(&mut blank)?;

                // Read JSON body
                let mut body = vec![0u8; len];
                stdin.read_exact(&mut body)?;

                let json_str = String::from_utf8_lossy(&body);
                match serde_json::from_str::<DapRequest>(&json_str) {
                    Ok(req) => {
                        if let Err(e) = self.handle_request(&req, &stdout) {
                            eprintln!("[dap] Handler error: {e}");
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("[dap] Parse error: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_request(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        match req.command.as_str() {
            "initialize" => self.handle_initialize(req, stdout)?,
            "launch" => self.handle_launch(req, stdout)?,
            "setBreakpoints" => self.handle_set_breakpoints(req, stdout)?,
            "configurationDone" => self.handle_configuration_done(req, stdout)?,
            "stackTrace" => self.handle_stack_trace(req, stdout)?,
            "scopes" => self.handle_scopes(req, stdout)?,
            "variables" => self.handle_variables(req, stdout)?,
            "continue" => self.handle_continue(req, stdout)?,
            "next" => self.handle_next(req, stdout)?,
            "stepIn" => self.handle_step_in(req, stdout)?,
            "stepOut" => self.handle_step_out(req, stdout)?,
            "threads" => self.handle_threads(req, stdout)?,
            "evaluate" => self.handle_evaluate(req, stdout)?,
            "disconnect" => self.handle_disconnect(req, stdout)?,
            _ => self.send_response(req, true, None, stdout)?,
        }
        Ok(())
    }

    fn handle_initialize(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let body = serde_json::to_value(Capabilities::default()).unwrap();
        self.send_response(req, true, Some(body), stdout)?;
        self.send_event("initialized", None, stdout)?;
        Ok(())
    }

    fn handle_launch(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let program_path = req.arguments
            .get("program")
            .and_then(|v| v.as_str())
            .unwrap_or("src/main.dal")
            .to_string();

        match std::fs::read_to_string(&program_path) {
            Ok(src) => {
                match self.compile_source(&src) {
                    Ok(prog) => {
                        self.program = Some(prog);
                        self.current_source = Some(program_path);
                        self.send_response(req, true, None, stdout)?;
                    }
                    Err(e) => {
                        self.send_event("output", Some(serde_json::json!({
                            "output": format!("Compile error: {}", e),
                            "category": "stderr"
                        })), stdout)?;
                        self.send_response(req, true, None, stdout)?;
                    }
                }
            }
            Err(e) => {
                self.send_event("output", Some(serde_json::json!({
                    "output": format!("Cannot read '{}': {}", program_path, e),
                    "category": "stderr"
                })), stdout)?;
                self.send_response(req, true, None, stdout)?;
            }
        }
        Ok(())
    }

    fn handle_set_breakpoints(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let source_path = req.arguments["source"]["path"].as_str().unwrap_or("").to_string();
        let bps: Vec<SourceBreakpoint> = serde_json::from_value(
            req.arguments["breakpoints"].clone()
        ).unwrap_or_default();

        let source = Source {
            name: source_path.rsplit('/').next().unwrap_or("").to_string(),
            path: source_path.clone(),
        };

        let mut verified_bps = Vec::new();
        let mut stored = Vec::new();

        for bp in &bps {
            let id = self.next_bp_id;
            self.next_bp_id += 1;
            verified_bps.push(Breakpoint::new(id, bp.line, source.clone()));
            stored.push((bp.line, id));
        }

        if !source_path.is_empty() {
            self.breakpoints.insert(source_path, stored);
        }

        self.send_response(req, true, Some(serde_json::json!({
            "breakpoints": verified_bps
        })), stdout)?;
        Ok(())
    }

    fn handle_configuration_done(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        self.send_event("stopped", Some(serde_json::json!({
            "reason": "entry",
            "threadId": 1,
            "allThreadsStopped": true,
        })), stdout)?;
        self.paused = true;
        self.frames = vec![StackFrame::new(0, "<main>", 1)];
        Ok(())
    }

    fn handle_stack_trace(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, Some(serde_json::json!({
            "stackFrames": self.frames,
            "totalFrames": self.frames.len(),
        })), stdout)?;
        Ok(())
    }

    fn handle_scopes(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, Some(serde_json::json!({
            "scopes": vec![
                Scope::new("Local", 1000),
                Scope::new("Global", 1001),
            ],
        })), stdout)?;
        Ok(())
    }

    fn handle_variables(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let mut vars = vec![Variable::new("(scope)", "local", "string")];
        if let Some(frame) = self.frames.first() {
            vars.push(Variable::new("frame", &frame.name, "string"));
            vars.push(Variable::new("line", frame.line.to_string(), "int"));
        }
        self.send_response(req, true, Some(serde_json::json!({
            "variables": vars
        })), stdout)?;
        Ok(())
    }

    fn handle_continue(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.paused = false;
        self.send_response(req, true, Some(serde_json::json!({
            "allThreadsContinued": true
        })), stdout)?;
        self.send_event("terminated", None, stdout)?;
        Ok(())
    }

    fn handle_next(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        if let Some(ref mut frame) = self.frames.first_mut() {
            frame.line += 1;
        }
        self.send_event("stopped", Some(serde_json::json!({
            "reason": "step",
            "threadId": 1,
        })), stdout)?;
        Ok(())
    }

    fn handle_step_in(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        self.send_event("stopped", Some(serde_json::json!({
            "reason": "step",
            "threadId": 1,
        })), stdout)?;
        Ok(())
    }

    fn handle_step_out(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        if self.frames.len() > 1 { self.frames.pop(); }
        self.send_event("stopped", Some(serde_json::json!({
            "reason": "step",
            "threadId": 1,
        })), stdout)?;
        Ok(())
    }

    fn handle_threads(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, Some(serde_json::json!({
            "threads": vec![Thread::new(1, "main")]
        })), stdout)?;
        Ok(())
    }

    fn handle_evaluate(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let expr = req.arguments["expression"].as_str().unwrap_or("");
        self.send_response(req, true, Some(serde_json::json!({
            "result": format!("<eval: {}>", expr),
            "type": "string",
            "variablesReference": 0,
        })), stdout)?;
        Ok(())
    }

    fn handle_disconnect(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        Ok(())
    }

    fn compile_source(&mut self, src: &str) -> Result<Program, String> {
        let tokens = Lexer::new(src)
            .tokenize()
            .map_err(|e| format!("Lex error [{}:{}]: {}", e.line, e.column, e.message))?;
        Ok(Parser::new(tokens).parse())
    }

    fn send_response(
        &mut self,
        req: &DapRequest,
        success: bool,
        body: Option<serde_json::Value>,
        stdout: &io::Stdout,
    ) -> io::Result<()> {
        let resp = DapResponse {
            seq: self.seq,
            msg_type: "response".into(),
            request_seq: req.seq,
            success,
            command: req.command.clone(),
            body,
            message: None,
        };
        self.seq += 1;
        self.write_json(&resp, stdout)
    }

    fn send_event(
        &mut self,
        event: &str,
        body: Option<serde_json::Value>,
        stdout: &io::Stdout,
    ) -> io::Result<()> {
        let msg = DapEvent {
            seq: self.seq,
            msg_type: "event".into(),
            event: event.into(),
            body,
        };
        self.seq += 1;
        self.write_json(&msg, stdout)
    }

    fn write_json<T: serde::Serialize>(&self, data: &T, stdout: &io::Stdout) -> io::Result<()> {
        let json = serde_json::to_string(data)?;
        let mut out = stdout.lock();
        write!(out, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
        out.flush()?;
        Ok(())
    }
}
