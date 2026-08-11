use crate::ast::{
    BaseType, EnumVariant, Expr, FieldDef, FnParam, MatchArm, Pattern, Program, Stmt, TraitMethod,
    TypeRef,
};
use crate::token::{
    Token, TokenType,
    TokenType::{
        And, Arrow, At, BitAnd, BitOr, BitXor, BoolLiteral, CharLiteral, Colon, Comma, Dot,
        DoubleArrow, DoubleColon, DoubleDot, DoubleEqual, Eof, Equal, FloatLiteral, Greater,
        GreaterEqual, Ident, IntLiteral, KeywordAssert, KeywordAsync, KeywordBreak, KeywordCatch,
        KeywordChannel, KeywordConst, KeywordContinue, KeywordElse, KeywordEnum, KeywordExport,
        KeywordFn, KeywordFor, KeywordIf, KeywordImpl, KeywordIn, KeywordLet, KeywordMatch,
        KeywordMut, KeywordReturn, KeywordSpawn, KeywordStruct, KeywordTrait, KeywordTry,
        KeywordType, KeywordUse, KeywordWhile, LeftBrace, LeftBracket, LeftParen, Less, LessEqual,
        Minus, Modulo, Not, NotEqual, Or, Pipe, Plus, QuestionMark, RightBrace, RightBracket,
        RightParen, Semicolon, Shl, Shr, Slash, Star, StringLiteral,
    },
};
/// Dalin L — 递归下降语法分析器
use std::fmt::Write;
/// Nine annotation fields returned by `parse_channel_annotations`.
pub type AnnotationResult = Result<
    (
        Option<String>,
        Option<String>,
        Option<String>, // effect, capability, llm_prompt
        Option<String>,
        Option<String>,
        Option<String>, // cognitive_loop, governance, latency
        Option<String>,
        Option<String>, // timeout, throughput
        Option<String>, // confidence
    ),
    ParseError,
>;

/// Multi-error parse result: (program, errors)
/// Errors are collected so compilation can proceed past syntax mistakes.
pub type ParseResult = Result<(Program, Vec<ParseError>), ParseError>;

#[derive(Debug, Clone)]
pub struct RecoveryError {
    pub error: ParseError,
    pub recovered: bool,
}

impl RecoveryError {
    #[must_use]
    pub fn unrecoverable(err: ParseError) -> Self {
        Self {
            error: err,
            recovered: false,
        }
    }
    #[must_use]
    pub fn recoverable(err: ParseError) -> Self {
        Self {
            error: err,
            recovered: true,
        }
    }
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] {} ({})",
            self.error.line,
            self.error.column,
            self.error.message,
            if self.recovered {
                "recovered"
            } else {
                "unrecoverable"
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.line, self.column, self.message)
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    /// 结构体字面量抑制标志。
    ///
    /// `if cond { .. }` / `while cond { .. }` / `for x in it { .. }` / `match t { .. }`
    /// 的头部表达式后面紧跟的 `{` 永远是块的开始，不能被当成 `Ident { field: .. }`。
    /// 与 Rust 的 `no_struct_literal` 限制同源。
    no_struct_lit: bool,
}

impl Parser {
    #[must_use]
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
            no_struct_lit: false,
        }
    }

    /// 在抑制结构体字面量的上下文中解析表达式（if/while/for/match 头部）。
    fn parse_expr_no_struct(&mut self) -> Result<Expr, ParseError> {
        let prev = self.no_struct_lit;
        self.no_struct_lit = true;
        let result = self.parse_expression();
        self.no_struct_lit = prev;
        result
    }

    /// Get all collected parse errors
    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Record an error and decide if recovery is possible
    fn record_error(&mut self, message: String, line: usize, column: usize) {
        self.errors.push(ParseError {
            message,
            line,
            column,
        });
    }

    /// Skip to next synchronization point in statement context.
    /// Sync points: `;`, `fn`, `let`, `mut`, `return`, `}`, `}`, `for`, `while`, `if`, `match`, `struct`, `enum`, `trait`, `impl`, `use`, `export`, `channel`, `spawn`, `assert`, `async`, `try`, `const`, `type`
    fn skip_to_sync(&mut self, stop_at_brace: bool) {
        loop {
            if self.check(Eof) {
                return;
            }
            match self.current().token_type {
                Semicolon => {
                    self.advance(); // consume the ;
                    return;
                }
                RightBrace if stop_at_brace => return,
                KeywordFn | KeywordLet | KeywordMut | KeywordReturn | KeywordFor | KeywordWhile
                | KeywordIf | KeywordMatch | KeywordStruct | KeywordEnum | KeywordTrait
                | KeywordImpl | KeywordUse | KeywordExport | KeywordChannel | KeywordSpawn
                | KeywordAssert | KeywordAsync | KeywordTry | KeywordConst | KeywordType
                | KeywordBreak | KeywordContinue | RightBrace => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Skip to sync in expression context (inside parens/brackets)
    fn skip_expr_to_sync(&mut self) {
        // Simple heuristic: skip tokens until we hit a statement sync point or expression terminator
        loop {
            if self.check(Eof)
                || self.check(Semicolon)
                || self.check(RightParen)
                || self.check(RightBracket)
                || self.check(RightBrace)
            {
                return;
            }
            // Check for statement-level keywords that signal end of expression
            match self.current().token_type {
                KeywordFn | KeywordLet | KeywordMut | KeywordReturn | KeywordFor | KeywordWhile
                | KeywordIf | KeywordMatch | KeywordStruct | KeywordEnum | KeywordTrait
                | KeywordImpl | KeywordUse | KeywordExport | KeywordChannel | KeywordSpawn
                | KeywordAssert | KeywordAsync | KeywordTry | KeywordConst | KeywordType
                | KeywordBreak | KeywordContinue => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn current(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else {
            // Return a proxy EOF
            static EOF: Token = Token {
                token_type: Eof,
                value: String::new(),
                line: 99999,
                column: 0,
            };
            &EOF
        }
    }

    fn peek(&self, offset: usize) -> &Token {
        let idx = self.pos + offset;
        if idx < self.tokens.len() {
            &self.tokens[idx]
        } else {
            static EOF: Token = Token {
                token_type: Eof,
                value: String::new(),
                line: 99999,
                column: 0,
            };
            &EOF
        }
    }

    fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        self.pos += 1;
        tok
    }

    fn check(&self, tt: TokenType) -> bool {
        self.current().token_type == tt
    }

    fn match_token(&mut self, tt: TokenType) -> bool {
        if self.check(tt) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tt: TokenType, name: &str) -> Result<Token, ParseError> {
        if self.check(tt) {
            Ok(self.advance())
        } else {
            let (line, column) = (self.current().line, self.current().column);
            let err = ParseError {
                message: format!(
                    "Expected {} but got {} ({:?})",
                    name,
                    self.current().token_type.name(),
                    self.current().value
                ),
                line,
                column,
            };
            // 记录错误（错误恢复依赖 errors 列表上报，仅返回 Err 会被主循环吞掉）
            self.record_error(err.message.clone(), line, column);
            Err(err)
        }
    }

    /// Get the value of a specific token (for peeking ahead without advancing)
    fn peek_token_value(&self, offset: usize) -> Option<&str> {
        self.tokens.get(self.pos + offset).map(|t| t.value.as_str())
    }

    // ═══════════════════════════════
    //  主要入口
    // ═══════════════════════════════

    pub fn parse(&mut self) -> Result<(Program, Vec<ParseError>), ParseError> {
        let mut prog = Program::new();
        while !self.check(Eof) {
            let before = self.pos;
            match self.parse_statement() {
                Ok(Some(s)) => prog.add(s),
                Err(_) => {
                    // Error already recorded; skip to sync point
                    self.skip_to_sync(false);
                    // 防死循环护栏：错误恢复必须推进 token 游标。
                    // 若 sync 点位于当前 token（如 '}'），主循环会在同一位置无限重试。
                    if self.pos == before {
                        self.advance();
                    }
                }
                Ok(None) => {
                    self.advance();
                }
            }
        }
        let errors = std::mem::take(&mut self.errors);
        Ok((prog, errors))
    }

    fn parse_statement(&mut self) -> Result<Option<Stmt>, ParseError> {
        match self.current().token_type {
            KeywordLet => {
                self.advance();
                Ok(Some(self.parse_let()?))
            }
            KeywordMut => {
                self.advance();
                Ok(Some(self.parse_mut_let()?))
            }
            KeywordFn => {
                self.advance();
                Ok(Some(self.parse_fn()?))
            }
            KeywordIf => {
                self.advance();
                Ok(Some(self.parse_if()?))
            }
            KeywordWhile => {
                self.advance();
                Ok(Some(self.parse_while()?))
            }
            KeywordFor => {
                self.advance();
                Ok(Some(self.parse_for()?))
            }
            KeywordMatch => {
                self.advance();
                Ok(Some(self.parse_match()?))
            }
            KeywordStruct => {
                self.advance();
                Ok(Some(self.parse_struct()?))
            }
            KeywordEnum => {
                self.advance();
                Ok(Some(self.parse_enum()?))
            }
            KeywordTrait => {
                self.advance();
                Ok(Some(self.parse_trait()?))
            }
            KeywordImpl => {
                self.advance();
                Ok(Some(self.parse_impl()?))
            }
            KeywordReturn => {
                self.advance();
                Ok(Some(self.parse_return()?))
            }
            // 循环控制流：裸关键字语句，无操作数。可选尾随分号在此吞掉。
            KeywordBreak => {
                self.advance();
                self.match_token(Semicolon);
                Ok(Some(Stmt::Break))
            }
            KeywordContinue => {
                self.advance();
                self.match_token(Semicolon);
                Ok(Some(Stmt::Continue))
            }
            KeywordUse => {
                self.advance();
                Ok(Some(self.parse_use()?))
            }
            KeywordExport => {
                self.advance();
                Ok(Some(self.parse_export()?))
            }
            KeywordSpawn => {
                self.advance();
                Ok(Some(self.parse_spawn()?))
            }
            KeywordAssert => {
                self.advance();
                Ok(Some(self.parse_assert()?))
            }
            KeywordChannel => {
                self.advance();
                Ok(Some(self.parse_channel()?))
            }
            KeywordAsync => {
                self.advance();
                Ok(Some(self.parse_async_fn()?))
            }
            KeywordTry => {
                self.advance();
                Ok(Some(self.parse_try_catch()?))
            }
            KeywordConst => {
                self.advance();
                Ok(Some(self.parse_const()?))
            }
            KeywordType => {
                self.advance();
                Ok(Some(self.parse_type_alias()?))
            }
            _ => {
                let expr = self.parse_expression()?;
                self.match_token(Semicolon);
                Ok(Some(Stmt::Expr(Box::new(expr))))
            }
        }
    }

    // ═══════════════════════════════
    //  语句解析
    // ═══════════════════════════════

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let mutable = self.match_token(KeywordMut);
        let name_tok = match self.current().token_type {
            Ident => self.advance(),
            _ => {
                self.record_error(
                    "Expected identifier after 'let'".into(),
                    self.current().line,
                    self.current().column,
                );
                return Ok(Stmt::Let {
                    name: String::new(),
                    value: None,
                    type_annotation: None,
                    mutable,
                });
            }
        };
        let name = name_tok.value.clone();
        let type_ann = if self.match_token(Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.match_token(Equal) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        Ok(Stmt::Let {
            name,
            value,
            type_annotation: type_ann,
            mutable,
        })
    }

    fn parse_mut_let(&mut self) -> Result<Stmt, ParseError> {
        let name_tok = match self.current().token_type {
            Ident => self.advance(),
            _ => {
                self.record_error(
                    "Expected identifier after 'mut'".into(),
                    self.current().line,
                    self.current().column,
                );
                return Ok(Stmt::Let {
                    name: String::new(),
                    value: None,
                    type_annotation: None,
                    mutable: true,
                });
            }
        };
        let name = name_tok.value.clone();
        let type_ann = if self.match_token(Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.match_token(Equal) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        Ok(Stmt::Let {
            name,
            value,
            type_annotation: type_ann,
            mutable: true,
        })
    }

    /// 解析效应/能力类型关键字（Dalin L 3.0）
    /// `pure` | `io` | `async` | `spawn` | `cpu` | `gpu` | `sfa` | `net`
    fn parse_effect_or_cap(&mut self) -> Result<String, ParseError> {
        let tok = self.current().clone();
        let valid = [
            "pure",
            "io",
            "async",
            "spawn",
            "cpu",
            "gpu",
            "sfa",
            "net",
            "perceive",
            "reason",
            "decide",
            "act",
            "loop",
            "gov",
            "latency",
            "timeout",
            "throughput",
            "proven",
            "verified",
            "inferred",
            "generated",
            "uncertain",
        ];
        let text =
            if tok.token_type == Ident || matches!(tok.token_type, KeywordAsync | KeywordSpawn) {
                self.advance();
                tok.value.clone()
            } else {
                let val = tok.value.clone();
                if valid.contains(&val.as_str()) {
                    self.advance();
                    val
                } else {
                    return Err(ParseError {
                        message: format!("Invalid annotation token: {:?}", tok.value),
                        line: tok.line,
                        column: tok.column,
                    });
                }
            };
        if valid.contains(&text.as_str()) {
            Ok(text)
        } else {
            Err(ParseError {
                message: format!("Invalid effect/capability type: {text}"),
                line: tok.line,
                column: tok.column,
            })
        }
    }

    /// 效应 token 集合：pure | io | async | spawn
    fn is_effect_token(s: &str) -> bool {
        matches!(s, "pure" | "io" | "async" | "spawn")
    }

    /// 能力 token 集合：cpu | gpu | sfa | net
    fn is_capability_token(s: &str) -> bool {
        matches!(s, "cpu" | "gpu" | "sfa" | "net")
    }

    /// 认知循环 token 集合：perceive | reason | decide | act | loop
    fn is_cognitive_loop_token(s: &str) -> bool {
        matches!(s, "perceive" | "reason" | "decide" | "act" | "loop")
    }

    /// 治理级别 token 集合：gov
    fn is_governance_token(s: &str) -> bool {
        s == "gov"
    }

    /// 时间通道 token 集合：latency | timeout | throughput
    fn is_time_channel_token(s: &str) -> bool {
        matches!(s, "latency" | "timeout" | "throughput")
    }

    /// 置信度 token 集合：proven | verified | inferred | generated | uncertain
    fn is_confidence_token(s: &str) -> bool {
        matches!(
            s,
            "proven" | "verified" | "inferred" | "generated" | "uncertain"
        )
    }

    /// 解析 0..n 个多通道注解 `@X` / `@X(...)` / `@ llm("...")`。
    /// 自动判定为效应/能力/置信度/认知循环/治理/时间通道；顺序无关。
    /// 返回 (effect, capability, `llm_prompt`, `cognitive_loop`, governance, latency, timeout, throughput, confidence)
    /// 特殊语法：
    ///   @ llm("...") — LLM 编译指令
    ///   @ gov(level) — 治理级别（带括号值）
    ///   @ latency(Nms) @ timeout(Ns|Nms) @ throughput(N/s) — 时间约束
    fn parse_channel_annotations(&mut self, preset_effect: Option<String>) -> AnnotationResult {
        let mut effect = preset_effect;
        let mut capability: Option<String> = None;
        let mut llm_prompt: Option<String> = None;
        let mut cognitive_loop: Option<String> = None;
        let mut governance: Option<String> = None;
        let mut latency: Option<String> = None;
        let mut timeout: Option<String> = None;
        let mut throughput: Option<String> = None;
        let mut confidence: Option<String> = None;
        loop {
            if !matches!(self.current().token_type, At) {
                break;
            }

            // @ llm("...")
            if self.peek_token_value(1) == Some("llm") {
                self.advance(); // consume @
                self.advance(); // consume llm
                self.expect(LeftParen, "'('")?;
                let prompt = self
                    .expect(StringLiteral, "LLM prompt string")?
                    .value
                    .clone();
                self.expect(RightParen, "')'")?;
                llm_prompt = Some(prompt);
                continue;
            }

            self.advance(); // consume @
            let tok = self.parse_effect_or_cap()?;

            if Self::is_effect_token(&tok) {
                effect = Some(tok);
            } else if Self::is_capability_token(&tok) {
                capability = Some(tok);
            } else if Self::is_cognitive_loop_token(&tok) {
                cognitive_loop = Some(tok);
            } else if Self::is_governance_token(&tok) {
                // @ gov(level)
                self.expect(LeftParen, "'('")?;
                let level = self.expect(Ident, "governance level")?.value.clone();
                match level.as_str() {
                    "prepare" | "suggest" | "approve" | "execute" => {
                        governance = Some(level);
                    }
                    _ => {
                        return Err(ParseError {
                            message: format!(
                                "Unknown governance level: {level}. Expected: prepare, suggest, approve, execute"
                            ),
                            line: self.current().line,
                            column: self.current().column,
                        });
                    }
                }
                self.expect(RightParen, "')'")?;
            } else if Self::is_confidence_token(&tok) {
                confidence = Some(tok);
            } else if Self::is_time_channel_token(&tok) {
                // @ latency(50ms) / @ timeout(5s) / @ throughput(100/s)
                self.expect(LeftParen, "'('")?;
                let num = self.expect(IntLiteral, "time value")?.value.clone();
                let mut unit = if self.current().token_type == Ident {
                    self.advance().value.clone()
                } else {
                    String::new()
                };
                // 处理 /s 后缀（如 throughput(100/s)）
                if self.current().token_type == Slash {
                    self.advance(); // consume /
                    let suffix = self.expect(Ident, "throughput unit")?.value.clone();
                    unit = format!("/{suffix}");
                }
                let val = format!("{num}{unit}");
                self.expect(RightParen, "')'")?;
                match tok.as_str() {
                    "latency" => latency = Some(val),
                    "timeout" => timeout = Some(val),
                    "throughput" => throughput = Some(val),
                    _ => {}
                }
            } else {
                return Err(ParseError {
                    message: format!("Unknown annotation token: {tok}"),
                    line: self.current().line,
                    column: self.current().column,
                });
            }
        }
        Ok((
            effect,
            capability,
            llm_prompt,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            confidence,
        ))
    }

    fn parse_fn(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "function name")?.value.clone();
        let params = self.parse_fn_params()?;
        let return_type = if self.match_token(Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let (
            effect,
            capability,
            llm_prompt,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            confidence,
        ) = self.parse_channel_annotations(None)?;
        let body = self.parse_block()?;
        Ok(Stmt::Fn {
            name,
            params,
            return_type,
            effect,
            capability,
            llm_prompt,
            confidence,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            body: Box::new(body),
            async_: false,
            pub_: false,
        })
    }

    fn parse_async_fn(&mut self) -> Result<Stmt, ParseError> {
        self.expect(KeywordFn, "'fn'")?;
        let name = self.expect(Ident, "function name")?.value.clone();
        let params = self.parse_fn_params()?;
        let return_type = if self.match_token(Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let (
            effect,
            capability,
            llm_prompt,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            confidence,
        ) = self.parse_channel_annotations(Some("async".to_string()))?;
        let body = self.parse_block()?;
        Ok(Stmt::Fn {
            name,
            params,
            return_type,
            effect,
            capability,
            llm_prompt,
            confidence,
            cognitive_loop,
            governance,
            latency,
            timeout,
            throughput,
            body: Box::new(body),
            async_: true,
            pub_: false,
        })
    }

    fn parse_fn_params(&mut self) -> Result<Vec<FnParam>, ParseError> {
        self.expect(LeftParen, "'('")?;
        let mut params = Vec::new();
        if !self.check(RightParen) {
            loop {
                let name = self.expect(Ident, "parameter name")?.value.clone();
                let type_ann = if self.match_token(Colon) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let default = if self.match_token(Equal) {
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                params.push(FnParam {
                    name,
                    type_annotation: type_ann,
                    default,
                });
                if !self.match_token(Comma) {
                    break;
                }
            }
        }
        self.expect(RightParen, "')'")?;
        Ok(params)
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let condition = Box::new(self.parse_expr_no_struct()?);
        let then_body = self.parse_block()?;
        let else_body = if self.match_token(KeywordElse) {
            if self.check(KeywordIf) {
                self.advance();
                vec![self.parse_if()?]
            } else {
                self.parse_block()?
            }
        } else {
            Vec::new()
        };
        Ok(Stmt::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let condition = Box::new(self.parse_expr_no_struct()?);
        let body = self.parse_block()?;
        Ok(Stmt::While { condition, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let target = self.expect(Ident, "loop variable")?.value.clone();
        self.expect(KeywordIn, "'in'")?;
        let iterable = Box::new(self.parse_expr_no_struct()?);
        let body = self.parse_block()?;
        Ok(Stmt::For {
            target,
            iterable,
            body,
        })
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        let target = Box::new(self.parse_expr_no_struct()?);
        self.expect(LeftBrace, "'{'")?;
        let mut arms = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            let pat = self.parse_pattern()?;
            let guard = if self.match_token(KeywordIf) {
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            self.expect(DoubleArrow, "'=>'")?;
            let body = vec![
                self.parse_statement()?
                    .unwrap_or(Stmt::Expr(Box::new(Expr::IntLiteral(0)))),
            ];
            self.match_token(Comma);
            arms.push(MatchArm {
                pattern: pat,
                guard,
                body,
            });
        }
        self.expect(RightBrace, "'}'")?;
        Ok(Stmt::Match { target, arms })
    }

    fn parse_struct(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "struct name")?.value.clone();
        let derives = Vec::new();
        self.expect(LeftBrace, "'{'")?;
        let mut fields = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            let fname = self.expect(Ident, "field name")?.value.clone();
            self.expect(Colon, "':'")?;
            let ftype = self.parse_type()?;
            fields.push(FieldDef {
                name: fname,
                type_annotation: ftype,
            });
            self.match_token(Comma);
        }
        self.expect(RightBrace, "'}'")?;
        Ok(Stmt::StructDef {
            name,
            derives,
            fields,
        })
    }

    fn parse_enum(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "enum name")?.value.clone();
        self.expect(LeftBrace, "'{'")?;
        let mut variants = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            let vname = self.expect(Ident, "variant name")?.value.clone();
            let fields = Vec::new();
            variants.push(EnumVariant {
                name: vname,
                fields,
            });
            self.match_token(Comma);
        }
        self.expect(RightBrace, "'}'")?;
        Ok(Stmt::EnumDef { name, variants })
    }

    fn parse_trait(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "trait name")?.value.clone();
        self.expect(LeftBrace, "'{'")?;
        let mut methods = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            self.expect(KeywordFn, "'fn'")?;
            let mname = self.expect(Ident, "method name")?.value.clone();
            let params = self.parse_fn_params()?;
            let return_type = if self.match_token(Arrow) {
                Some(self.parse_type()?)
            } else {
                None
            };
            methods.push(TraitMethod {
                name: mname,
                return_type,
                params,
            });
        }
        self.expect(RightBrace, "'}'")?;
        Ok(Stmt::TraitDef { name, methods })
    }

    fn parse_impl(&mut self) -> Result<Stmt, ParseError> {
        let trait_name = None;
        let type_name = self.expect(Ident, "type name")?.value.clone();
        self.expect(LeftBrace, "'{'")?;
        let mut methods = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            self.expect(KeywordFn, "'fn'")?;
            let mname = self.expect(Ident, "method name")?.value.clone();
            let _params = self.parse_fn_params()?;
            let return_type = if self.match_token(Arrow) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let _body = self.parse_block()?;
            methods.push(FnParam {
                name: mname,
                type_annotation: return_type,
                default: None,
            });
        }
        self.expect(RightBrace, "'}'")?;
        Ok(Stmt::ImplBlock {
            trait_name,
            type_name,
            methods,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        if self.check(RightBrace) || self.check(Semicolon) {
            Ok(Stmt::Return(None))
        } else {
            match self.parse_expression() {
                Ok(expr) => Ok(Stmt::Return(Some(Box::new(expr)))),
                Err(e) => {
                    self.skip_expr_to_sync();
                    self.record_error(
                        format!("Failed to parse return expression: {}", e.message),
                        e.line,
                        e.column,
                    );
                    Ok(Stmt::Return(None))
                }
            }
        }
    }

    fn parse_use(&mut self) -> Result<Stmt, ParseError> {
        let path = self.expect(Ident, "module path")?.value.clone();
        Ok(Stmt::Use(path))
    }

    fn parse_export(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "export name")?.value.clone();
        Ok(Stmt::Export(name))
    }

    fn parse_spawn(&mut self) -> Result<Stmt, ParseError> {
        self.expect(KeywordFn, "'fn'")?;
        let fn_stmt = self.parse_fn()?;
        Ok(Stmt::Spawn {
            fn_decl: Box::new(fn_stmt),
        })
    }

    fn parse_channel(&mut self) -> Result<Stmt, ParseError> {
        let send_name = self.expect(Ident, "channel send endpoint")?.value.clone();
        let recv_name = self.expect(Ident, "channel recv endpoint")?.value.clone();
        // 可选元素类型注解 `: T`（Phase 0 仅记录，运行时不强制）
        let elem_type = if self.match_token(Colon) {
            self.parse_type()?
        } else {
            crate::ast::TypeRef::new(crate::ast::BaseType::Unknown)
        };
        // 容量语法（`cap N`）留待后续；Phase 0 使用无界通道。
        Ok(Stmt::Channel {
            send_name,
            recv_name,
            elem_type,
            capacity: 0,
        })
    }

    fn parse_assert(&mut self) -> Result<Stmt, ParseError> {
        let condition = Box::new(self.parse_expression()?);
        let message = if self.match_token(Comma) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        Ok(Stmt::Assert { condition, message })
    }

    fn parse_try_catch(&mut self) -> Result<Stmt, ParseError> {
        let try_body = self.parse_block()?;
        self.expect(KeywordCatch, "'catch'")?;
        let (catch_param, catch_body) = if self.match_token(LeftParen) {
            let param = self.expect(Ident, "catch parameter")?.value.clone();
            self.expect(RightParen, "')'")?;
            let body = self.parse_block()?;
            (Some(param), body)
        } else {
            (None, self.parse_block()?)
        };
        Ok(Stmt::TryCatch {
            try_body,
            catch_param,
            catch_body,
        })
    }

    fn parse_const(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "constant name")?.value.clone();
        let type_ann = if self.match_token(Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.match_token(Equal) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        Ok(Stmt::Const {
            name,
            value,
            type_annotation: type_ann,
        })
    }

    fn parse_type_alias(&mut self) -> Result<Stmt, ParseError> {
        let name = self.expect(Ident, "type name")?.value.clone();
        let aliased_type = if self.match_token(Equal) {
            Some(self.parse_type()?)
        } else {
            None
        };
        Ok(Stmt::TypeAlias { name, aliased_type })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        match self.current().token_type {
            LeftBrace => {
                self.advance();
            }
            _ => {
                self.record_error(
                    "Expected '{' for block".into(),
                    self.current().line,
                    self.current().column,
                );
                return Ok(Vec::new());
            }
        }
        let mut stmts = Vec::new();
        while !self.check(RightBrace) && !self.check(Eof) {
            if let Some(s) = self.parse_statement()? {
                stmts.push(s);
            }
            // 统一消费语句末尾的可选分号。
            // 各关键字分支（let/return/if 等）本身不消费 ';'，
            // 若不在此处理，分号会被后续 parse_statement 误判为表达式错误。
            self.match_token(Semicolon);
        }
        if self.check(RightBrace) {
            self.advance();
        } else {
            self.record_error(
                "Expected '}' to close block".into(),
                self.current().line,
                self.current().column,
            );
        }
        Ok(stmts)
    }

    // ═══════════════════════════════
    //  Pattern 解析
    // ═══════════════════════════════

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.current().clone();

        // Wildcard
        if tok.token_type == Ident && tok.value == "_" {
            self.advance();
            return Ok(Pattern {
                kind: "wild".into(),
                name: String::new(),
                binding: None,
                inner: Vec::new(),
                fields: Vec::new(),
                value: None,
            });
        }

        // Ident pattern (constructor or binding)
        if tok.token_type == Ident {
            self.advance();
            let name = tok.value.clone();

            // Constructor with inner patterns: Some(v), Ok(v), Err(e)
            if self.match_token(LeftParen) {
                let inner_pat = self.parse_pattern()?;
                self.expect(RightParen, "')'")?;
                return Ok(Pattern {
                    kind: "ctor".into(),
                    name,
                    binding: Some(inner_pat.name.clone()),
                    inner: vec![inner_pat],
                    fields: Vec::new(),
                    value: None,
                });
            }

            // Struct pattern: Box { x, y }
            if self.match_token(LeftBrace) {
                let mut fields = Vec::new();
                while !self.check(RightBrace) && !self.check(Eof) {
                    let fname = self.expect(Ident, "field name")?.value.clone();
                    if self.match_token(Colon) {
                        let binding = self.expect(Ident, "binding")?.value.clone();
                        fields.push((fname, binding));
                    } else {
                        fields.push((fname.clone(), fname));
                    }
                    self.match_token(Comma);
                }
                self.expect(RightBrace, "'}'")?;
                return Ok(Pattern {
                    kind: "struct".into(),
                    name,
                    binding: None,
                    inner: Vec::new(),
                    fields,
                    value: None,
                });
            }

            // Bare constructor (enum variant): Red, Green, Blue, None
            return Ok(Pattern {
                kind: "ctor".into(),
                name,
                binding: None,
                inner: Vec::new(),
                fields: Vec::new(),
                value: None,
            });
        }

        // Literal pattern
        if matches!(
            tok.token_type,
            IntLiteral | FloatLiteral | StringLiteral | BoolLiteral | CharLiteral
        ) {
            self.advance();
            let lit_expr = match tok.token_type {
                IntLiteral => Expr::IntLiteral(tok.value.parse().unwrap_or(0)),
                FloatLiteral => Expr::FloatLiteral(tok.value.parse().unwrap_or(0.0)),
                StringLiteral => Expr::StringLiteral(tok.value.clone()),
                BoolLiteral => Expr::BoolLiteral(tok.value == "true"),
                CharLiteral => Expr::CharLiteral(tok.value.chars().next().unwrap_or('\0')),
                _ => unreachable!(),
            };
            return Ok(Pattern {
                kind: "lit".into(),
                name: String::new(),
                binding: None,
                inner: Vec::new(),
                fields: Vec::new(),
                value: Some(Box::new(lit_expr)),
            });
        }

        Err(ParseError {
            message: format!("Unexpected token in pattern: {:?}", tok.value),
            line: tok.line,
            column: tok.column,
        })
    }

    // ═══════════════════════════════
    //  类型解析
    // ═══════════════════════════════

    fn parse_type(&mut self) -> Result<TypeRef, ParseError> {
        let tok = self.expect(Ident, "type name")?;
        let base = match tok.value.as_str() {
            "int" => BaseType::Int,
            "float" => BaseType::Float,
            "string" => BaseType::String,
            "bool" => BaseType::Bool,
            "char" => BaseType::Char,
            "none" => BaseType::None,
            "array" => BaseType::Array,
            "option" => BaseType::Option,
            "result" => BaseType::Result,
            "func" => BaseType::Func,
            _ => BaseType::Unknown,
        };

        // Generic: Type<T> or Type<T, E>
        if self.match_token(Less) {
            let arg = self.parse_type()?;
            let err = if self.match_token(Comma) {
                Some(self.parse_type()?)
            } else {
                None
            };
            self.expect(Greater, "'>'")?;
            return Ok(TypeRef {
                base,
                generic_arg: Some(Box::new(arg)),
                result_err: err.map(Box::new),
            });
        }

        Ok(TypeRef::new(base))
    }

    // ═══════════════════════════════
    //  表达式解析
    // ═══════════════════════════════

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_binary(0)?;

        let mut ops = Vec::new();
        while self.match_token(Pipe) {
            let name = match self.current().token_type {
                Ident => {
                    let tok = self.advance();
                    tok.value
                }
                _ => {
                    self.record_error(
                        "Expected pipe function name".into(),
                        self.current().line,
                        self.current().column,
                    );
                    self.skip_expr_to_sync();
                    return Ok(left);
                }
            };
            ops.push((name.clone(), Expr::Ident(name)));
        }

        if ops.is_empty() {
            Ok(left)
        } else {
            Ok(Expr::Pipe {
                input: Box::new(left),
                ops,
            })
        }
    }

    fn op_precedence(tt: TokenType) -> i32 {
        match tt {
            Or => 1,
            And => 2,
            BitOr => 3,
            BitXor => 4,
            BitAnd => 5,
            Equal | DoubleEqual | NotEqual => 6,
            Less | Greater | LessEqual | GreaterEqual => 7,
            Shl | Shr => 8,
            Plus | Minus => 9,
            Star | Slash | Modulo => 10,
            _ => 0,
        }
    }

    fn parse_binary(&mut self, min_prec: i32) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;

        loop {
            let op_tt = self.current().token_type;
            let prec = Self::op_precedence(op_tt);
            if prec < min_prec {
                break;
            }
            if prec == 0 {
                break;
            }

            self.advance(); // consume operator
            let op = match op_tt {
                Plus => "+",
                Minus => "-",
                Star => "*",
                Slash => "/",
                Modulo => "%",
                Equal => "=",
                DoubleEqual => "==",
                NotEqual => "!=",
                Less => "<",
                Greater => ">",
                LessEqual => "<=",
                GreaterEqual => ">=",
                And => "&&",
                Or => "||",
                BitAnd => "&",
                BitOr => "|",
                BitXor => "^",
                Shl => "<<",
                Shr => ">>",
                _ => {
                    return Err(ParseError {
                        message: format!("Unknown binary operator: {:?}", self.current().value),
                        line: self.current().line,
                        column: self.current().column,
                    });
                }
            }
            .to_string();

            let right = self.parse_binary(prec + 1)?;
            left = Expr::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.current().clone();
        match tok.token_type {
            Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: "-".into(),
                    operand: Box::new(operand),
                })
            }
            Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: "!".into(),
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_primary(),
        }
    }

    /// 前瞻判定紧跟的 `{` 是否为结构体字面量体。
    ///
    /// 只接受两种确定形态，避免与块表达式抢词：
    /// - `{ }`            → 无字段结构体
    /// - `{ ident : ...`  → 首个字段是 `名字: 值`
    ///
    /// 调用方须先确认 `self.check(LeftBrace)`。
    fn looks_like_struct_literal(&self) -> bool {
        if self.peek(1).token_type == RightBrace {
            return true;
        }
        self.peek(1).token_type == Ident && self.peek(2).token_type == Colon
    }

    /// 解析结构体字面量体：`{ field: expr, ... }`（`name` 已被消费）。
    fn parse_struct_literal(&mut self, name: String) -> Result<Expr, ParseError> {
        self.expect(LeftBrace, "'{'")?;
        let mut fields: Vec<(String, Expr)> = Vec::new();

        while !self.check(RightBrace) && !self.check(Eof) {
            let field_tok = self.expect(Ident, "field name")?;
            self.expect(Colon, "':' after field name")?;
            // 字段值处于花括号内，结构体字面量在此重新允许（支持嵌套构造）
            let prev = self.no_struct_lit;
            self.no_struct_lit = false;
            let value = self.parse_expression();
            self.no_struct_lit = prev;
            fields.push((field_tok.value, value?));

            if !self.match_token(Comma) {
                break;
            }
        }

        self.expect(RightBrace, "'}' to close struct literal")?;
        Ok(Expr::StructLiteral { name, fields })
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.current().clone();

        // if/match as expressions (for let r = if true { 42 })
        if tok.token_type == KeywordIf {
            self.advance();
            let stmt = self.parse_if()?;
            return Ok(self.stmt_to_expr(stmt));
        }
        if tok.token_type == KeywordMatch {
            self.advance();
            let stmt = self.parse_match()?;
            return Ok(self.stmt_to_expr(stmt));
        }

        // Range expression: 0..10
        if tok.token_type == IntLiteral && self.peek(1).token_type == DoubleDot {
            let start = tok.value.parse::<i64>().unwrap_or(0);
            self.advance();
            self.advance();
            let end_tok = self.expect(IntLiteral, "range end")?;
            let end = end_tok.value.parse::<i64>().unwrap_or(0);
            return Ok(Expr::Range {
                start: Box::new(Expr::IntLiteral(start)),
                end: Box::new(Expr::IntLiteral(end)),
                inclusive: false,
            });
        }

        // Literals
        match tok.token_type {
            IntLiteral => {
                self.advance();
                return Ok(Expr::IntLiteral(tok.value.parse().unwrap_or(0)));
            }
            FloatLiteral => {
                self.advance();
                return Ok(Expr::FloatLiteral(tok.value.parse().unwrap_or(0.0)));
            }
            StringLiteral => {
                self.advance();
                return Ok(Expr::StringLiteral(tok.value.clone()));
            }
            BoolLiteral => {
                self.advance();
                return Ok(Expr::BoolLiteral(tok.value == "true"));
            }
            CharLiteral => {
                self.advance();
                return Ok(Expr::CharLiteral(tok.value.chars().next().unwrap_or('\0')));
            }
            _ => {}
        }

        // Option/Result literal
        if tok.token_type == Ident
            && (tok.value == "Some"
                || tok.value == "None"
                || tok.value == "Ok"
                || tok.value == "Err")
        {
            self.advance();
            let base = match tok.value.as_str() {
                "Some" => {
                    self.expect(LeftParen, "'('")?;
                    let val = self.parse_expression()?;
                    self.expect(RightParen, "')'")?;
                    Expr::OptionValue {
                        is_some: true,
                        value: Some(Box::new(val)),
                    }
                }
                "None" => Expr::OptionValue {
                    is_some: false,
                    value: None,
                },
                "Ok" => {
                    self.expect(LeftParen, "'('")?;
                    let val = self.parse_expression()?;
                    self.expect(RightParen, "')'")?;
                    Expr::ResultValue {
                        is_ok: true,
                        value: Some(Box::new(val)),
                        error: None,
                    }
                }
                "Err" => {
                    self.expect(LeftParen, "'('")?;
                    let val = self.parse_expression()?;
                    self.expect(RightParen, "')'")?;
                    Expr::ResultValue {
                        is_ok: false,
                        value: None,
                        error: Some(Box::new(val)),
                    }
                }
                _ => Expr::Ident(tok.value.clone()),
            };
            return self.parse_postfix(base);
        }

        // Parenthesized expression
        if tok.token_type == LeftParen {
            self.advance();
            let expr = self.parse_expression()?;
            self.expect(RightParen, "')'")?;
            return self.parse_postfix(expr);
        }

        // Array literal: [1, 2, 3]
        if tok.token_type == LeftBracket {
            self.advance();
            let mut elements = Vec::new();
            if !self.check(RightBracket) {
                elements.push(self.parse_expression()?);
                while self.match_token(Comma) {
                    elements.push(self.parse_expression()?);
                }
            }
            self.expect(RightBracket, "']'")?;
            return self.parse_postfix(Expr::Array(elements));
        }

        // Block as expression
        if tok.token_type == LeftBrace {
            let stmts = self.parse_block()?;
            let last = stmts
                .into_iter()
                .last()
                .unwrap_or(Stmt::Expr(Box::new(Expr::IntLiteral(0))));
            return self.parse_postfix(self.stmt_to_expr(last));
        }

        // Identifier / call / member access / index
        if tok.token_type == Ident {
            self.advance();

            // 结构体字面量：`Point { x: 1, y: 2 }`
            // 仅在非抑制上下文、且后续 token 形态确为 `{ }` 或 `{ ident : ...` 时才认，
            // 否则退回把 `{` 交给外层当块处理（保持 `if x {`、`fn f() {` 等语义不变）。
            if !self.no_struct_lit && self.check(LeftBrace) && self.looks_like_struct_literal() {
                return self.parse_struct_literal(tok.value.clone());
            }

            let mut obj = Expr::Ident(tok.value.clone());

            // 模块路径：mod::func / mod::sub::func
            // 解析为嵌套 MemberAccess（mod 的 func 成员），随后可继续 call/index 链
            while self.match_token(DoubleColon) {
                let member_tok = self.expect(Ident, "path segment")?;
                obj = Expr::MemberAccess {
                    object: Box::new(obj),
                    member: member_tok.value,
                };
            }

            return self.parse_postfix(obj);
        }

        Err(ParseError {
            message: format!("Unexpected token: {:?}", tok.value),
            line: tok.line,
            column: tok.column,
        })
    }

    /// Postfix 运算符链：`.` 成员访问 / `(` 调用 / `[` 索引 / `?` 错误传播。
    ///
    /// 统一应用于所有 primary 表达式之后（标识符、Option/Result 字面量、括号、数组、块），
    /// 使 `Some(42)?`、`(f())?`、`arr[0]?` 等组合均可解析；`?` 仅在 Option/Result 上早退。
    fn parse_postfix(&mut self, mut obj: Expr) -> Result<Expr, ParseError> {
        loop {
            // Member access: obj.field
            if self.match_token(Dot) {
                match self.current().token_type {
                    Ident => {
                        let member = self.advance().value;
                        obj = Expr::MemberAccess {
                            object: Box::new(obj),
                            member,
                        };
                    }
                    _ => {
                        self.record_error(
                            "Expected field name after '.'".into(),
                            self.current().line,
                            self.current().column,
                        );
                        break;
                    }
                }
            }
            // Call: obj(args)
            else if self.match_token(LeftParen) {
                let mut args = Vec::new();
                if !self.check(RightParen) {
                    args.push(self.parse_expression()?);
                    while self.match_token(Comma) {
                        args.push(self.parse_expression()?);
                    }
                }
                match self.current().token_type {
                    RightParen => {
                        self.advance();
                        obj = Expr::Call {
                            func: Box::new(obj),
                            args,
                        };
                    }
                    _ => {
                        self.record_error(
                            "Expected ')'".into(),
                            self.current().line,
                            self.current().column,
                        );
                        self.skip_expr_to_sync();
                        break;
                    }
                }
            }
            // Index: obj[index]
            else if self.match_token(LeftBracket) {
                let idx = self.parse_expression()?;
                match self.current().token_type {
                    RightBracket => {
                        self.advance();
                        obj = Expr::Index {
                            array: Box::new(obj),
                            index: Box::new(idx),
                        };
                    }
                    _ => {
                        self.record_error(
                            "Expected ']'".into(),
                            self.current().line,
                            self.current().column,
                        );
                        self.skip_expr_to_sync();
                        break;
                    }
                }
            }
            // Try (错误传播)：expr?  —— 仅对 Option/Result 早退
            else if self.match_token(QuestionMark) {
                obj = Expr::Try(Box::new(obj));
            } else {
                break;
            }
        }
        Ok(obj)
    }

    fn stmt_to_expr(&self, stmt: Stmt) -> Expr {
        match stmt {
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond_expr = *condition;
                // Convert last statement of each block to expression
                let then_expr = stmts_to_expr(then_body);
                let else_expr = stmts_to_expr(else_body);
                Expr::IfExpr(
                    Box::new(cond_expr),
                    Box::new(then_expr),
                    Box::new(else_expr),
                )
            }
            Stmt::Match { target, arms } => Expr::MatchExpr(Box::new(*target), arms),
            other => stmt_to_expr_single(other),
        }
    }
}

fn stmts_to_expr(stmts: Vec<Stmt>) -> Expr {
    let mut iter = stmts.into_iter();
    let last = iter.next_back();
    match last {
        Some(s) => stmt_to_expr_single(s),
        None => Expr::IntLiteral(0),
    }
}

fn stmt_to_expr_single(stmt: Stmt) -> Expr {
    match stmt {
        Stmt::Expr(e) => *e,
        Stmt::Return(Some(e)) => *e,
        Stmt::Return(None) => Expr::IntLiteral(0),
        Stmt::Let { value, .. } => value.map_or(Expr::IntLiteral(0), |v| *v),
        _ => Expr::IntLiteral(0),
    }
}

// ── AST 打印 ──

#[must_use]
pub fn ast_to_string(node: &Program) -> String {
    let mut lines = Vec::new();
    lines.push("Program(".to_string());
    for s in &node.statements {
        lines.push(format!("  {}", stmt_to_string(s, 1)));
    }
    lines.push(")".to_string());
    lines.join("\n")
}

fn stmt_to_string(stmt: &Stmt, indent: usize) -> String {
    let p = "  ".repeat(indent);
    match stmt {
        Stmt::Let {
            name,
            value,
            type_annotation,
            mutable,
        } => {
            let mut s = format!("{}{}Let({}", p, if *mutable { "mut " } else { "" }, name);
            if let Some(t) = type_annotation {
                write!(s, ": {t}").unwrap();
            }
            if let Some(v) = value {
                write!(s, " = {}", expr_to_string(v, 0)).unwrap();
            }
            s.push(')');
            s
        }
        Stmt::Fn {
            name,
            params,
            return_type,
            ..
        } => {
            let params_str: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            let ret = return_type
                .as_ref()
                .map(|t| format!(" -> {t}"))
                .unwrap_or_default();
            format!("{p}Fn({name}({params_str:?}){ret})")
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
        } => {
            let then_s = stmts_to_string(then_body, indent + 1);
            let else_s = if else_body.is_empty() {
                String::new()
            } else {
                format!(" else {}", stmts_to_string(else_body, indent + 1))
            };
            format!(
                "{p}If({} => {}{else_s})",
                expr_to_string(condition, 0),
                then_s
            )
        }
        Stmt::Return(v) => match v {
            Some(e) => format!("{p}Return({})", expr_to_string(e, 0)),
            None => format!("{p}Return(None)"),
        },
        _ => format!("{p}{}", stmt_name(stmt)),
    }
}

fn stmt_name(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Let { .. } => "Let",
        Stmt::Const { .. } => "Const",
        Stmt::Fn { .. } => "Fn",
        Stmt::Return(_) => "Return",
        Stmt::Break => "Break",
        Stmt::Continue => "Continue",
        Stmt::If { .. } => "If",
        Stmt::While { .. } => "While",
        Stmt::For { .. } => "For",
        Stmt::Match { .. } => "Match",
        Stmt::StructDef { .. } => "StructDef",
        Stmt::EnumDef { .. } => "EnumDef",
        Stmt::TraitDef { .. } => "TraitDef",
        Stmt::ImplBlock { .. } => "ImplBlock",
        Stmt::Spawn { .. } => "Spawn",
        Stmt::Channel { .. } => "Channel",
        Stmt::TryCatch { .. } => "TryCatch",
        Stmt::Assert { .. } => "Assert",
        Stmt::Use(_) => "Use",
        Stmt::Export(_) => "Export",
        Stmt::TypeAlias { .. } => "TypeAlias",
        Stmt::Expr(_) => "Expr",
        Stmt::Llm { .. } => "Llm",
    }
}

fn stmts_to_string(stmts: &[Stmt], indent: usize) -> String {
    stmts
        .iter()
        .map(|s| stmt_to_string(s, indent))
        .collect::<Vec<_>>()
        .join("\n")
}

fn expr_to_string(expr: &Expr, _indent: usize) -> String {
    match expr {
        Expr::IntLiteral(v) => format!("Int({v})"),
        Expr::FloatLiteral(v) => format!("Float({v})"),
        Expr::StringLiteral(v) => format!("Str({v:?})"),
        Expr::BoolLiteral(v) => format!("Bool({v})"),
        Expr::CharLiteral(v) => format!("Char({v:?})"),
        Expr::Ident(v) => format!("Ident({v})"),
        Expr::StructLiteral { name, fields } => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", expr_to_string(v, 0)))
                .collect();
            format!("StructLit({name} {{ {} }})", parts.join(", "))
        }
        Expr::BinaryOp { left, op, right } => format!(
            "Bin({}, {}, {})",
            expr_to_string(left, 0),
            op,
            expr_to_string(right, 0)
        ),
        Expr::UnaryOp { op, operand } => format!("Unary({}, {})", op, expr_to_string(operand, 0)),
        Expr::Call { func, args } => {
            let args_str: Vec<String> = args.iter().map(|a| expr_to_string(a, 0)).collect();
            format!("Call({}({}))", expr_to_string(func, 0), args_str.join(", "))
        }
        Expr::Pipe { input, ops } => format!(
            "Pipe({}, {:?})",
            expr_to_string(input, 0),
            ops.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
        ),
        Expr::Range { start, end, .. } => format!(
            "Range({}..{})",
            expr_to_string(start, 0),
            expr_to_string(end, 0)
        ),
        Expr::Array(elems) => {
            let elems_str: Vec<String> = elems.iter().map(|e| expr_to_string(e, 0)).collect();
            format!("[{}]", elems_str.join(", "))
        }
        Expr::OptionValue { is_some, value } => {
            if *is_some {
                format!(
                    "Some({})",
                    value
                        .as_ref()
                        .map(|v| expr_to_string(v, 0))
                        .unwrap_or_default()
                )
            } else {
                "None".into()
            }
        }
        Expr::ResultValue {
            is_ok,
            value,
            error,
        } => {
            if *is_ok {
                format!(
                    "Ok({})",
                    value
                        .as_ref()
                        .map(|v| expr_to_string(v, 0))
                        .unwrap_or_default()
                )
            } else {
                format!(
                    "Err({})",
                    error
                        .as_ref()
                        .map(|v| expr_to_string(v, 0))
                        .unwrap_or_default()
                )
            }
        }
        Expr::MemberAccess { object, member } => {
            format!("{}.{}", expr_to_string(object, 0), member)
        }
        Expr::Index { array, index } => {
            format!("{}[{}]", expr_to_string(array, 0), expr_to_string(index, 0))
        }
        Expr::IfExpr(c, t, e) => format!(
            "If({}, {}, {})",
            expr_to_string(c, 0),
            expr_to_string(t, 0),
            expr_to_string(e, 0)
        ),
        Expr::MatchExpr(target, _) => format!("Match({})", expr_to_string(target, 0)),
        Expr::Try(inner) => format!("Try({})", expr_to_string(inner, 0)),
    }
}

/// Expression variants for if/match that were converted from statements
impl Expr {
    // Placeholder variants for if/match expressions
    #[must_use]
    pub fn dummy() -> Self {
        Expr::IntLiteral(0)
    }
}

// Can't add new variants to an enum from outside,
// so we define these as top-level in ast.rs instead.
// For now, if/match as expressions work through the
// stmt_to_expr conversion returning current variants.
