//! Dalin L 编译错误增强 — 七通道违规位置标注
//!
//! 把错误信息与源码位置关联，支持格式化输出。
//!
//! 示例输出：
//! ```text
//! error[E001]: 效应违规: io 不能出现在 pure 上下文中
//!   --> main.dalin:3:15
//!    |
//!  3 |     let x = read_line()
//!    |             ^^^^^^^^^^ 效应: io > pure
//! ```

use std::fmt;

/// 源码位置
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub filename: String,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.filename, self.line, self.column)
    }
}

/// 七通道编译错误
#[derive(Debug, Clone)]
pub enum ChannelError {
    EffectViolation {
        location: SourceLocation,
        context: String,
        required: String,
        detail: String,
    },
    CapabilityViolation {
        location: SourceLocation,
        context: String,
        required: String,
        detail: String,
    },
    ConfidenceViolation {
        location: SourceLocation,
        actual: String,
        required: String,
        detail: String,
    },
    CognitiveLoopViolation {
        location: SourceLocation,
        context: String,
        required: String,
        detail: String,
    },
    GovernanceViolation {
        location: SourceLocation,
        required: String,
        actual: String,
        detail: String,
    },
    LatencyViolation {
        location: SourceLocation,
        declared_ms: u64,
        actual_ms: u64,
        detail: String,
    },
    TypeError {
        location: SourceLocation,
        message: String,
    },
    SyntaxError {
        location: SourceLocation,
        message: String,
    },
}

impl ChannelError {
    pub fn code(&self) -> &str {
        match self {
            ChannelError::EffectViolation { .. } => "E001",
            ChannelError::CapabilityViolation { .. } => "E002",
            ChannelError::TypeError { .. } => "E003",
            ChannelError::SyntaxError { .. } => "E004",
            ChannelError::ConfidenceViolation { .. } => "E005",
            ChannelError::CognitiveLoopViolation { .. } => "E006",
            ChannelError::GovernanceViolation { .. } => "E007",
            ChannelError::LatencyViolation { .. } => "E008",
        }
    }

    /// Format error with source code snippet and caret indicator.
    /// ```text
    /// error[E004]: 语法错误
    ///   --> main.dal:3:15
    ///    |
    ///  3 |     let x = broken_fn(
    ///    |             ^^^^^^^^^ 期待 ')'
    /// ```
    pub fn format_with_source(&self, source: &str) -> String {
        let loc = match self {
            ChannelError::EffectViolation { location, .. } => location,
            ChannelError::CapabilityViolation { location, .. } => location,
            ChannelError::ConfidenceViolation { location, .. } => location,
            ChannelError::CognitiveLoopViolation { location, .. } => location,
            ChannelError::GovernanceViolation { location, .. } => location,
            ChannelError::LatencyViolation { location, .. } => location,
            ChannelError::TypeError { location, .. } => location,
            ChannelError::SyntaxError { location, .. } => location,
        };

        let base = format!("{}", self);
        let line_str = source.lines().nth(loc.line.saturating_sub(1));
        match line_str {
            Some(line_content) => {
                let caret = " ".repeat(loc.column.saturating_sub(1)) + &"^".repeat(
                    line_content.len().saturating_sub(loc.column.saturating_sub(1)).max(1)
                );
                format!(
                    "{base}   |\n  {:>3} | {}\n   | {}\n",
                    loc.line,
                    line_content,
                    caret
                )
            }
            None => base,
        }
    }
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelError::EffectViolation {
                location,
                context,
                required,
                detail,
            } => {
                writeln!(f, "error[{}]: 效应违规", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | 上下文效应: {context}, 需要: {required}")?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::CapabilityViolation {
                location,
                context,
                required,
                detail,
            } => {
                writeln!(f, "error[{}]: 能力违规", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | 上下文能力: {context}, 需要: {required}")?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::ConfidenceViolation {
                location,
                actual,
                required,
                detail,
            } => {
                writeln!(f, "error[{}]: 置信度不足", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | 实际: {actual}, 需要: {required}")?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::CognitiveLoopViolation {
                location,
                context,
                required,
                detail,
            } => {
                writeln!(f, "error[{}]: 认知循环违规", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | 上下文: {context}, 需要: {required}")?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::GovernanceViolation {
                location,
                required,
                actual,
                detail,
            } => {
                writeln!(f, "error[{}]: 治理违规", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | 需要: {required}, 当前: {actual}")?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::LatencyViolation {
                location,
                declared_ms,
                actual_ms,
                detail,
            } => {
                writeln!(f, "error[{}]: 延迟超限", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(
                    f,
                    "   | 声明: {}ms, 实际: {}ms (超限 {}ms)",
                    declared_ms,
                    actual_ms,
                    actual_ms.saturating_sub(*declared_ms)
                )?;
                writeln!(f, "   | {detail}")
            }
            ChannelError::TypeError { location, message } => {
                writeln!(f, "error[{}]: 类型错误", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | {message}")
            }
            ChannelError::SyntaxError { location, message } => {
                writeln!(f, "error[{}]: 语法错误", self.code())?;
                writeln!(f, "  --> {}", location)?;
                writeln!(f, "   |")?;
                writeln!(f, "   | {message}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_error_format() {
        let err = ChannelError::EffectViolation {
            location: SourceLocation {
                line: 3,
                column: 15,
                filename: "test.dalin".into(),
            },
            context: "pure".into(),
            required: "io".into(),
            detail: "pure 上下文中禁止 IO 操作".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E001"));
        assert!(msg.contains("test.dalin:3:15"));
    }

    #[test]
    fn capability_error_format() {
        let err = ChannelError::CapabilityViolation {
            location: SourceLocation {
                line: 5,
                column: 10,
                filename: "worker.dalin".into(),
            },
            context: "cpu".into(),
            required: "sfa".into(),
            detail: "sfa 能力需要 SFA 注意力路由".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E002"));
        assert!(msg.contains("sfa"));
    }

    #[test]
    fn confidence_error_format() {
        let err = ChannelError::ConfidenceViolation {
            location: SourceLocation {
                line: 10,
                column: 5,
                filename: "ai.dalin".into(),
            },
            actual: "Generated".into(),
            required: "Verified".into(),
            detail: "LLM 生成代码需要 verify 调用".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E005"));
        assert!(msg.contains("Generated"));
        assert!(msg.contains("Verified"));
    }

    #[test]
    fn cognitive_loop_error_format() {
        let err = ChannelError::CognitiveLoopViolation {
            location: SourceLocation {
                line: 15,
                column: 8,
                filename: "agent.dalin".into(),
            },
            context: "Perceive".into(),
            required: "Act".into(),
            detail: "感知阶段不能执行操作".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E006"));
        assert!(msg.contains("Perceive"));
    }

    #[test]
    fn governance_error_format() {
        let err = ChannelError::GovernanceViolation {
            location: SourceLocation {
                line: 20,
                column: 12,
                filename: "pay.dalin".into(),
            },
            required: "approve".into(),
            actual: "prepare".into(),
            detail: "扣款需要审批权限".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E007"));
        assert!(msg.contains("approve"));
    }

    #[test]
    fn latency_error_format() {
        let err = ChannelError::LatencyViolation {
            location: SourceLocation {
                line: 25,
                column: 1,
                filename: "rt.dalin".into(),
            },
            declared_ms: 50,
            actual_ms: 120,
            detail: "调用链超限 70ms".into(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("E008"));
        assert!(msg.contains("50ms"));
        assert!(msg.contains("120ms"));
        assert!(msg.contains("70ms"));
    }

    #[test]
    fn source_snippet_format() {
        let err = ChannelError::SyntaxError {
            location: SourceLocation {
                line: 3,
                column: 15,
                filename: "test.dal".into(),
            },
            message: "期待 ')' 但遇到 '}'".into(),
        };
        let src = "let x = 42\nfn f() {\n    let y = broken(\n}\n";
        let msg = err.format_with_source(src);
        assert!(msg.contains("broken"), "should show source line: {msg}");
        assert!(msg.contains("^^"), "should have caret: {msg}");
        assert!(msg.contains("E004"), "should have error code: {msg}");
    }

    #[test]
    fn source_snippet_first_line() {
        let err = ChannelError::TypeError {
            location: SourceLocation {
                line: 1,
                column: 1,
                filename: "test.dal".into(),
            },
            message: "类型不匹配".into(),
        };
        let src = "let x = \"hello\"\nlet y = x + 1\n";
        let msg = err.format_with_source(src);
        assert!(msg.contains("let x"));
        assert!(msg.contains("^^^"));
    }

    #[test]
    fn source_snippet_out_of_range() {
        let err = ChannelError::SyntaxError {
            location: SourceLocation {
                line: 999,
                column: 1,
                filename: "empty.dal".into(),
            },
            message: "未预期的文件结尾".into(),
        };
        let src = "";
        let msg = err.format_with_source(src);
        assert!(msg.contains("E004"));
        assert!(!msg.contains("^^")); // no caret for out-of-range
    }
}
