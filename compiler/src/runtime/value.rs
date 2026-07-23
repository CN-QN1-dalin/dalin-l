/// Dalin L 3.0 — Runtime Value and Error types
///
/// Core data types: RuntimeValue (all runtime values) and RuntimeError (all runtime errors).
use std::fmt;

use crate::ty2::{Capability, CognitiveLoop, GovernanceLevel};

// ═══════════════════════════════════════════
//  RuntimeValue — 运行时值表示
// ═══════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
    None,
    Array(Vec<RuntimeValue>),
    Struct(String, Vec<(String, RuntimeValue)>),
    /// Result 类型: (is_ok, ok_value, err_value)
    Result(bool, Option<Box<RuntimeValue>>, Option<Box<RuntimeValue>>),
    /// Option 类型: (is_some, value)
    Option(bool, Option<Box<RuntimeValue>>),
    /// 闭包/函数引用
    Func(String),
}

impl fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeValue::Int(v) => write!(f, "{}", v),
            RuntimeValue::Float(v) => write!(f, "{}", v),
            RuntimeValue::String(s) => write!(f, "\"{}\"", s),
            RuntimeValue::Bool(b) => write!(f, "{}", b),
            RuntimeValue::Char(c) => write!(f, "'{}'", c),
            RuntimeValue::None => write!(f, "none"),
            RuntimeValue::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            RuntimeValue::Struct(name, fields) => {
                write!(f, "{} {{ ", name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, " }}")
            }
            RuntimeValue::Result(is_ok, ok_v, err_v) => {
                if *is_ok {
                    if let Some(v) = ok_v {
                        write!(f, "ok({})", v)
                    } else {
                        write!(f, "ok")
                    }
                } else if let Some(e) = err_v {
                    write!(f, "err({})", e)
                } else {
                    write!(f, "err")
                }
            }
            RuntimeValue::Option(is_some, v) => {
                if *is_some {
                    if let Some(v) = v {
                        write!(f, "some({})", v)
                    } else {
                        write!(f, "some")
                    }
                } else {
                    write!(f, "none")
                }
            }
            RuntimeValue::Func(name) => write!(f, "<fn {}>", name),
        }
    }
}

// ═══════════════════════════════════════════
//  RuntimeError — 运行时错误
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum RuntimeError {
    UndefinedVariable(String),
    UndefinedFunction(String),
    TypeError {
        expected: String,
        actual: String,
        detail: String,
    },
    DivisionByZero,
    EffectViolation {
        declared: Capability, // simplified from Effect for now
        required: Capability,
        fn_name: String,
    },
    CognitiveLoopViolation {
        declared: CognitiveLoop,
        required: CognitiveLoop,
        fn_name: String,
    },
    GovernanceViolation {
        declared: GovernanceLevel,
        required: GovernanceLevel,
        fn_name: String,
    },
    TimeoutExceeded {
        constraint_ms: u64,
        elapsed_ms: u64,
        fn_name: String,
    },
    LatencyViolation {
        declared_ms: u64,
        actual_ms: u64,
        fn_name: String,
    },
    AssertionFailed {
        message: String,
    },
    RuntimePanic(String),
    StepBudgetExceeded {
        step_count: u64,
        max_steps: u64,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::UndefinedVariable(name) => {
                write!(f, "undefined variable: {}", name)
            }
            RuntimeError::UndefinedFunction(name) => {
                write!(f, "undefined function: {}", name)
            }
            RuntimeError::TypeError {
                expected,
                actual,
                detail,
            } => {
                write!(
                    f,
                    "type error: expected {}, got {} ({})",
                    expected, actual, detail
                )
            }
            RuntimeError::DivisionByZero => write!(f, "division by zero"),
            RuntimeError::EffectViolation {
                declared,
                required,
                fn_name,
            } => {
                write!(
                    f,
                    "effect violation in '{}': declared {}, required {}",
                    fn_name, declared, required
                )
            }
            RuntimeError::CognitiveLoopViolation {
                declared,
                required,
                fn_name,
            } => {
                write!(
                    f,
                    "cognitive loop violation in '{}': declared {:?}, required {:?}",
                    fn_name, declared, required
                )
            }
            RuntimeError::GovernanceViolation {
                declared,
                required,
                fn_name,
            } => {
                write!(
                    f,
                    "governance violation in '{}': session={}, required={}",
                    fn_name, declared, required
                )
            }
            RuntimeError::TimeoutExceeded {
                constraint_ms,
                elapsed_ms,
                fn_name,
            } => {
                write!(
                    f,
                    "timeout exceeded in '{}': elapsed {}ms > limit {}ms",
                    fn_name, elapsed_ms, constraint_ms
                )
            }
            RuntimeError::LatencyViolation {
                declared_ms,
                actual_ms,
                fn_name,
            } => {
                write!(
                    f,
                    "latency violation in '{}': actual {}ms > declared {}ms",
                    fn_name, actual_ms, declared_ms
                )
            }
            RuntimeError::AssertionFailed { message } => {
                write!(f, "assertion failed: {}", message)
            }
            RuntimeError::RuntimePanic(msg) => write!(f, "runtime panic: {}", msg),
            RuntimeError::StepBudgetExceeded {
                step_count,
                max_steps,
            } => {
                write!(
                    f,
                    "step budget exceeded: {} / {}",
                    step_count, max_steps
                )
            }
        }
    }
}

// ═══════════════════════════════════════════
//  RuntimeResult — 快捷类型别名
// ═══════════════════════════════════════════

pub type RuntimeResult<T> = Result<T, RuntimeError>;
