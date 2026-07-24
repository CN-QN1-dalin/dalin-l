#![allow(non_camel_case_types, dead_code)]
/// Dalin L 3.0 — DAP Protocol Types
///
/// Minimal DAP type definitions covering the essential debugging workflow:
/// Initialize → Launch → SetBreakpoints → StackTrace → Scopes → Variables → Continue/Step
use serde::{Deserialize, Serialize};

// ── Base DAP Protocol ──

/// A generic DAP message (envelope)
#[derive(Debug, Deserialize, Serialize)]
pub struct DapMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(rename = "type")]
    pub msg_type: String,
}

/// Request from client
#[derive(Debug, Deserialize)]
pub struct DapRequest {
    pub seq: u64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub command: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// Response to client
#[derive(Debug, Serialize)]
pub struct DapResponse {
    pub seq: u64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub request_seq: u64,
    pub success: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Event sent to client
#[derive(Debug, Serialize)]
pub struct DapEvent {
    pub seq: u64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

// ── Capabilities ──

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub supports_configuration_done_request: bool,
    pub supports_function_breakpoints: bool,
    pub supports_conditional_breakpoints: bool,
    pub supports_hit_conditional_breakpoints: bool,
    pub supports_evaluate_for_hover: bool,
    pub supports_step_back: bool,
    pub supports_set_variable: bool,
    pub supports_restart_frame: bool,
    pub supports_goto_targets_request: bool,
    pub supports_step_in_targets_request: bool,
    pub supports_completions_request: bool,
    pub supports_modules_request: bool,
    pub support_terminate_debuggee: bool,
    pub supports_delayed_stack_trace_loading: bool,
    pub supports_log_points: bool,
    pub supports_terminate_threads_request: bool,
    pub supports_set_expression: bool,
    pub supports_terminate_request: bool,
    pub supports_data_breakpoints: bool,
    pub supports_read_memory_request: bool,
    pub supports_disassemble_request: bool,
    pub exception_breakpoint_filters: Vec<ExceptionFilter>,
}

#[derive(Debug, Serialize)]
pub struct ExceptionFilter {
    pub filter: String,
    pub label: String,
    #[serde(default)]
    pub default: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            supports_configuration_done_request: true,
            supports_function_breakpoints: false,
            supports_conditional_breakpoints: false,
            supports_hit_conditional_breakpoints: false,
            supports_evaluate_for_hover: true,
            supports_step_back: false,
            supports_set_variable: false,
            supports_restart_frame: false,
            supports_goto_targets_request: false,
            supports_step_in_targets_request: false,
            supports_completions_request: false,
            supports_modules_request: false,
            support_terminate_debuggee: true,
            supports_delayed_stack_trace_loading: false,
            supports_log_points: false,
            supports_terminate_threads_request: false,
            supports_set_expression: false,
            supports_terminate_request: true,
            supports_data_breakpoints: false,
            supports_read_memory_request: false,
            supports_disassemble_request: false,
            exception_breakpoint_filters: vec![ExceptionFilter {
                filter: "all".into(),
                label: "All Exceptions".into(),
                default: false,
            }],
        }
    }
}

// ── Breakpoint Types ──

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceBreakpoint {
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Breakpoint {
    pub id: u64,
    pub verified: bool,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub source: Source,
}

impl Breakpoint {
    pub fn new(id: u64, line: usize, source: Source) -> Self {
        Self {
            id,
            verified: true,
            line,
            column: None,
            message: None,
            source,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub name: String,
    pub path: String,
}

// ── Stack & Variable Types ──

#[derive(Debug, Clone, Serialize)]
pub struct StackFrame {
    pub id: u64,
    pub name: String,
    pub source: Option<Source>,
    pub line: usize,
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

impl StackFrame {
    pub fn new(id: u64, name: impl Into<String>, line: usize) -> Self {
        Self {
            id,
            name: name.into(),
            source: None,
            line,
            column: 0,
            end_line: None,
            end_column: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Scope {
    pub name: String,
    pub variables_reference: u64,
    pub expensive: bool,
}

impl Scope {
    pub fn new(name: impl Into<String>, var_ref: u64) -> Self {
        Self {
            name: name.into(),
            variables_reference: var_ref,
            expensive: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type")]
    pub var_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables_reference: Option<u64>,
}

impl Variable {
    pub fn new(name: impl Into<String>, value: impl Into<String>, var_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            var_type: var_type.into(),
            variables_reference: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub id: u64,
    pub name: String,
}

impl Thread {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}
