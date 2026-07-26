/// Dalin L 3.0 — DAP Debug Server
///
/// Debug Adapter Protocol server that communicates with `VSCode` over stdin/stdout.
/// Provides: breakpoints, stepping, stack traces, variable inspection.
mod protocol;

use dalin_compiler::ast::Program;
use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use dalin_dlvm::{BytecodeCompiler, VmError};
use protocol::{
    Breakpoint, Capabilities, DapEvent, DapRequest, DapResponse, Scope, Source, SourceBreakpoint,
    StackFrame, Thread, Variable,
};
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

/// `DebugVM` wraps the DLVM and provides debugging capabilities
pub struct DebugVm {
    functions: Vec<dalin_dlvm::BytecodeFunction>,
    stopped: bool,
    stop_reason: String,
    current_line: usize,
}

impl DebugVm {
    #[must_use]
    pub fn new(functions: Vec<dalin_dlvm::BytecodeFunction>) -> Self {
        Self {
            functions,
            stopped: false,
            stop_reason: "initialized".to_string(),
            current_line: 1,
        }
    }

    /// Get current function name
    #[must_use]
    pub fn get_current_function(&self) -> &str {
        if self.functions.is_empty() {
            "<none>"
        } else {
            &self.functions[0].name
        }
    }

    /// Step one instruction
    pub fn step(&mut self) {
        self.stopped = true;
        self.stop_reason = "step".to_string();
        self.current_line += 1;
    }

    /// Continue execution
    pub fn continue_execution(&mut self) -> Result<(), VmError> {
        self.stopped = false;
        self.stop_reason = "continue".to_string();
        Ok(())
    }

    /// Pause execution
    pub fn pause(&mut self) {
        self.stopped = true;
        self.stop_reason = "pause".to_string();
    }

    /// Get current state
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Get stop reason
    #[must_use]
    pub fn get_stop_reason(&self) -> &str {
        &self.stop_reason
    }

    /// Get current line number
    #[must_use]
    pub fn get_current_line(&self) -> usize {
        self.current_line
    }
}

pub struct DebugServer {
    program: Option<Program>,
    debug_vm: Option<DebugVm>,
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            program: None,
            debug_vm: None,
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
        let program_path = req
            .arguments
            .get("program")
            .and_then(|v| v.as_str())
            .unwrap_or("src/main.dal")
            .to_string();

        match std::fs::read_to_string(&program_path) {
            Ok(src) => match self.compile_and_debug(&src, &program_path) {
                Ok(()) => {
                    self.send_response(req, true, None, stdout)?;
                }
                Err(e) => {
                    self.send_event(
                        "output",
                        Some(serde_json::json!({
                            "output": format!("Compile error: {}", e),
                            "category": "stderr"
                        })),
                        stdout,
                    )?;
                    self.send_response(req, true, None, stdout)?;
                }
            },
            Err(e) => {
                self.send_event(
                    "output",
                    Some(serde_json::json!({
                        "output": format!("Cannot read '{}': {}", program_path, e),
                        "category": "stderr"
                    })),
                    stdout,
                )?;
                self.send_response(req, true, None, stdout)?;
            }
        }
        Ok(())
    }

    fn compile_and_debug(&mut self, src: &str, path: &str) -> Result<(), String> {
        // Tokenize
        let tokens = Lexer::new(src)
            .tokenize()
            .map_err(|e| format!("Lex error [{}:{}]: {}", e.line, e.column, e.message))?;

        // Parse
        let (prog, _) = Parser::new(tokens)
            .parse()
            .map_err(|e| format!("Parse error [{}:{}]: {}", e.line, e.column, e.message))?;
        self.program = Some(prog.clone());
        self.current_source = Some(path.to_string());

        // Compile to bytecode using BytecodeCompiler from dalin-dlvm
        let mut compiler = BytecodeCompiler::new();
        let functions = compiler.compile(&prog);

        if functions.is_empty() {
            return Err("Compilation produced no functions".to_string());
        }

        // Create debug VM
        self.debug_vm = Some(DebugVm::new(functions));
        self.paused = true;

        Ok(())
    }

    fn handle_set_breakpoints(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let source_path = req.arguments["source"]["path"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let bps: Vec<SourceBreakpoint> =
            serde_json::from_value(req.arguments["breakpoints"].clone()).unwrap_or_default();

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

        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "breakpoints": verified_bps
            })),
            stdout,
        )?;
        Ok(())
    }

    fn handle_configuration_done(
        &mut self,
        req: &DapRequest,
        stdout: &io::Stdout,
    ) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        self.send_event(
            "stopped",
            Some(serde_json::json!({
                "reason": "entry",
                "threadId": 1,
                "allThreadsStopped": true,
            })),
            stdout,
        )?;
        self.paused = true;

        // Update frames from debug VM if available
        if let Some(ref debug_vm) = self.debug_vm {
            self.frames = vec![StackFrame::new(
                0,
                debug_vm.get_current_function(),
                debug_vm.get_current_line(),
            )];
        } else {
            self.frames = vec![StackFrame::new(0, "<main>", 1)];
        }

        Ok(())
    }

    fn handle_stack_trace(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "stackFrames": self.frames,
                "totalFrames": self.frames.len(),
            })),
            stdout,
        )?;
        Ok(())
    }

    fn handle_scopes(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "scopes": vec![
                    Scope::new("Local", 1000),
                    Scope::new("Global", 1001),
                ],
            })),
            stdout,
        )?;
        Ok(())
    }

    fn handle_variables(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let mut vars = vec![Variable::new("(scope)", "local", "string")];

        // Placeholder for future integration with VM locals
        vars.push(Variable::new("debug_mode", "enabled", "bool"));

        if let Some(frame) = self.frames.first() {
            vars.push(Variable::new("frame", &frame.name, "string"));
            vars.push(Variable::new("line", frame.line.to_string(), "int"));
        }

        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "variables": vars
            })),
            stdout,
        )?;
        Ok(())
    }

    fn handle_continue(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.paused = false;

        // Continue execution in debug VM
        if let Some(ref mut debug_vm) = self.debug_vm {
            match debug_vm.continue_execution() {
                Ok(()) => {
                    self.send_response(
                        req,
                        true,
                        Some(serde_json::json!({
                            "allThreadsContinued": true
                        })),
                        stdout,
                    )?;
                    // Simulate termination after continue
                    self.send_event("terminated", None, stdout)?;
                }
                Err(e) => {
                    eprintln!("[dap] VM continue error: {e}");
                    self.send_response(req, false, None, stdout)?;
                }
            }
        } else {
            self.send_response(
                req,
                true,
                Some(serde_json::json!({
                    "allThreadsContinued": true
                })),
                stdout,
            )?;
            self.send_event("terminated", None, stdout)?;
        }

        Ok(())
    }

    fn handle_next(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;

        if let Some(ref mut debug_vm) = self.debug_vm {
            debug_vm.step();
            if let Some(ref mut frame) = self.frames.first_mut() {
                frame.line = debug_vm.get_current_line();
            }
        } else if let Some(ref mut frame) = self.frames.first_mut() {
            frame.line += 1;
        }

        self.send_event(
            "stopped",
            Some(serde_json::json!({
                "reason": "step",
                "threadId": 1,
            })),
            stdout,
        )?;

        Ok(())
    }

    fn handle_step_in(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;

        if let Some(ref mut debug_vm) = self.debug_vm {
            debug_vm.step();
        }

        self.send_event(
            "stopped",
            Some(serde_json::json!({
                "reason": "step",
                "threadId": 1,
            })),
            stdout,
        )?;

        Ok(())
    }

    fn handle_step_out(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;

        if let Some(ref mut debug_vm) = self.debug_vm {
            debug_vm.step();
        }

        if self.frames.len() > 1 {
            self.frames.pop();
        }

        self.send_event(
            "stopped",
            Some(serde_json::json!({
                "reason": "step",
                "threadId": 1,
            })),
            stdout,
        )?;

        Ok(())
    }

    fn handle_threads(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "threads": vec![Thread::new(1, "main")]
            })),
            stdout,
        )?;
        Ok(())
    }

    fn handle_evaluate(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        let expr = req.arguments["expression"].as_str().unwrap_or("");

        // Placeholder for future expression evaluation against VM state
        self.send_response(
            req,
            true,
            Some(serde_json::json!({
                "result": format!("<eval: {}>", expr),
                "type": "string",
                "variablesReference": 0,
            })),
            stdout,
        )?;

        Ok(())
    }

    fn handle_disconnect(&mut self, req: &DapRequest, stdout: &io::Stdout) -> io::Result<()> {
        self.send_response(req, true, None, stdout)?;
        Ok(())
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
