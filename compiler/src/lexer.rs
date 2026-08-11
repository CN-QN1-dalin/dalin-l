/// Dalin L — 词法分析器
use crate::token::{
    Token, TokenType,
    TokenType::{
        And, Arrow, At, Attribute, BitAnd, BitOr, BitXor, BoolLiteral, CharLiteral, Colon, Comma,
        Dollar, Dot, DoubleArrow, DoubleColon, DoubleDot, DoubleEqual, Eof, Equal, FloatLiteral,
        Greater, GreaterEqual, Ident, IntLiteral, KeywordAssert, KeywordAsync, KeywordBreak,
        KeywordCatch, KeywordChannel, KeywordConst, KeywordContinue, KeywordElse, KeywordEnum,
        KeywordError, KeywordExport, KeywordFn, KeywordFor, KeywordIf, KeywordImpl, KeywordIn,
        KeywordLet, KeywordMatch, KeywordMod, KeywordMut, KeywordOk, KeywordPub, KeywordReturn,
        KeywordSpawn, KeywordStruct, KeywordTrait, KeywordTry, KeywordType, KeywordUse,
        KeywordWhile, LeftBrace, LeftBracket, LeftParen, Less, LessEqual, Minus, MinusEqual,
        Modulo, Not, NotEqual, Or, Pipe, Plus, PlusEqual, QuestionMark, RightBrace, RightBracket,
        RightParen, Semicolon, Shl, Shr, Slash, SlashEqual, Star, StarEqual, StringLiteral,
    },
};
use std::collections::HashMap;

fn is_chinese_char(ch: char) -> bool {
    let cp = ch as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x20000..=0x2A6DF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_' || is_chinese_char(ch)
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || is_chinese_char(ch)
}

fn build_keywords() -> HashMap<&'static str, TokenType> {
    let mut m = HashMap::new();
    m.insert("let", KeywordLet);
    m.insert("fn", KeywordFn);
    m.insert("return", KeywordReturn);
    m.insert("if", KeywordIf);
    m.insert("else", KeywordElse);
    m.insert("match", KeywordMatch);
    m.insert("for", KeywordFor);
    m.insert("in", KeywordIn);
    m.insert("while", KeywordWhile);
    m.insert("break", KeywordBreak);
    m.insert("continue", KeywordContinue);
    m.insert("spawn", KeywordSpawn);
    m.insert("async", KeywordAsync);
    m.insert("try", KeywordTry);
    m.insert("catch", KeywordCatch);
    m.insert("use", KeywordUse);
    m.insert("trait", KeywordTrait);
    m.insert("assert", KeywordAssert);
    m.insert("channel", KeywordChannel);
    m.insert("mut", KeywordMut);
    m.insert("ok", KeywordOk);
    m.insert("error", KeywordError);
    m.insert("export", KeywordExport);
    m.insert("pub", KeywordPub);
    m.insert("impl", KeywordImpl);
    m.insert("struct", KeywordStruct);
    m.insert("enum", KeywordEnum);
    m.insert("type", KeywordType);
    m.insert("const", KeywordConst);
    m.insert("mod", KeywordMod);
    m.insert("true", BoolLiteral);
    m.insert("false", BoolLiteral);
    m
}

#[derive(Debug)]
pub struct LexerError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for LexerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}:{}] {}", self.line, self.column, self.message)
    }
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    keywords: HashMap<&'static str, TokenType>,
}

impl Lexer {
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            keywords: build_keywords(),
        }
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.current()?;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.pos += 1;
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        loop {
            let ch = match self.current() {
                Some(c) => c,
                None => return,
            };
            match ch {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.advance();
                }
                '/' if self.peek(1) == Some('/') => self.skip_line_comment(),
                '/' if self.peek(1) == Some('*') => self.skip_block_comment(),
                _ => break,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.advance(); // /
        self.advance(); // /
        while let Some(ch) = self.current() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        self.advance(); // /
        self.advance(); // *
        while let Some(ch) = self.current() {
            if ch == '*' && self.peek(1) == Some('/') {
                self.advance();
                self.advance();
                return;
            }
            self.advance();
        }
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(ch) = self.current() {
            if is_ident_continue(ch) {
                self.advance();
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn read_number(&mut self) -> Result<(TokenType, String), LexerError> {
        let start = self.pos;

        // 十六进制字面量: 0x1F / 0xFF / 0x10
        if self.current() == Some('0') && (self.peek(1) == Some('x') || self.peek(1) == Some('X')) {
            self.advance(); // 0
            self.advance(); // x
            let hex_start = self.pos;
            while let Some(ch) = self.current() {
                if ch.is_ascii_hexdigit() {
                    self.advance();
                } else {
                    break;
                }
            }
            let hex_text: String = self.chars[hex_start..self.pos].iter().collect();
            if !hex_text.is_empty() {
                // 归一化为十进制表示，供下游 parse::<i64>() 直接使用
                if let Ok(v) = i64::from_str_radix(&hex_text, 16) {
                    return Ok((IntLiteral, v.to_string()));
                }
                // 非空但超出 i64 范围：清晰报错，而非误当标识符产生误导性 "Undefined variable"
                return Err(LexerError {
                    message: format!("十六进制字面量超出 i64 范围: 0x{}", hex_text),
                    line: self.line,
                    column: self.column,
                });
            }
            // 空十六进制字面量 (0x 后无数字)：报错而非误当标识符
            return Err(LexerError {
                message: "空十六进制字面量: 0x 后缺少十六进制数字".into(),
                line: self.line,
                column: self.column,
            });
        }

        let mut has_dot = false;

        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                self.advance();
            } else if ch == '.' && !has_dot {
                if self.peek(1) == Some('.') {
                    break;
                }
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        // 科学计数法: 1e12 / 1.5e-3 / 2E+10 / 3.0E8
        let mut has_exp = false;
        if matches!(self.current(), Some('e' | 'E')) {
            let sign_at = self.peek(1);
            let has_sign = sign_at == Some('+') || sign_at == Some('-');
            let digit_peek = if has_sign { self.peek(2) } else { sign_at };
            if digit_peek.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                self.advance(); // 消费 e/E
                if has_sign {
                    self.advance(); // 消费 +/- 号
                }
                while let Some(c) = self.current() {
                    if c.is_ascii_digit() {
                        self.advance();
                    } else {
                        break;
                    }
                }
                has_exp = true;
            }
        }

        let text: String = self.chars[start..self.pos].iter().collect();
        if has_dot || has_exp {
            // 带小数点或科学计数法均归为浮点字面量
            if text.parse::<f64>().is_ok() {
                return Ok((FloatLiteral, text));
            }
            // 带小数点/指数但 f64 解析失败（极罕见格式）：清晰报错
            return Err(LexerError {
                message: format!("无效浮点字面量: {}", text),
                line: self.line,
                column: self.column,
            });
        }
        if text.parse::<i64>().is_ok() {
            return Ok((IntLiteral, text));
        }
        // 纯整数但超出 i64 范围：清晰报错，避免误当标识符导致误导性的 "Undefined variable"
        if text.chars().all(|c| c.is_ascii_digit()) {
            return Err(LexerError {
                message: format!("整数字面量超出 i64 范围 (最大 {}): {}", i64::MAX, text),
                line: self.line,
                column: self.column,
            });
        }
        // 兜底（理论不可达：read_number 仅消费 digit/./e）：保留原 Ident 行为
        Ok((Ident, text))
    }

    fn read_string(&mut self, quote: char) -> Result<String, LexerError> {
        self.advance(); // skip opening quote
        let mut parts = String::new();

        loop {
            match self.current() {
                None => {
                    return Err(LexerError {
                        message: "Unterminated string".into(),
                        line: self.line,
                        column: self.column,
                    });
                }
                Some('\\') => {
                    self.advance();
                    let esc = self.current().unwrap_or('\\');
                    let replacement = match esc {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\'' => '\'',
                        '\\' => '\\',
                        _ => esc,
                    };
                    parts.push(replacement);
                    self.advance();
                }
                Some(ch) if ch == quote => {
                    self.advance(); // skip closing quote
                    return Ok(parts);
                }
                Some(ch) => {
                    parts.push(ch);
                    self.advance();
                }
            }
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexerError> {
        self.skip_whitespace();

        let line = self.line;
        let col = self.column;

        let ch = match self.current() {
            Some(c) => c,
            None => return Ok(Token::new(Eof, String::new(), line, col)),
        };

        // Identifiers & keywords
        if is_ident_start(ch) {
            let ident = self.read_ident();
            let tt = self.keywords.get(ident.as_str()).copied().unwrap_or(Ident);
            return Ok(Token::new(tt, ident, line, col));
        }

        // Numbers
        if ch.is_ascii_digit() {
            let (tt, val) = self.read_number()?;
            return Ok(Token::new(tt, val, line, col));
        }

        // Strings
        if ch == '"' || ch == '\'' {
            let s = self.read_string(ch)?;
            let tt = if ch == '"' {
                StringLiteral
            } else {
                CharLiteral
            };
            return Ok(Token::new(tt, s, line, col));
        }

        // Attribute #[...]
        if ch == '#' && self.peek(1) == Some('[') {
            self.advance(); // #
            self.advance(); // [
            let mut attr = String::from("#[");
            while let Some(c) = self.current() {
                if c == ']' {
                    attr.push(']');
                    self.advance();
                    break;
                }
                attr.push(c);
                self.advance();
            }
            return Ok(Token::new(Attribute, attr, line, col));
        }

        // Multi-char operators
        let two_char: String = [ch, self.peek(1).unwrap_or('\0')].iter().collect();
        let double_map: HashMap<&str, TokenType> = [
            ("->", Arrow),
            ("=>", DoubleArrow),
            ("|>", Pipe),
            ("<|", Pipe),
            ("==", DoubleEqual),
            ("!=", NotEqual),
            ("<=", LessEqual),
            (">=", GreaterEqual),
            ("&&", And),
            ("||", Or),
            ("<<", Shl),
            (">>", Shr),
            ("..", DoubleDot),
            ("::", DoubleColon),
            ("+=", PlusEqual),
            ("-=", MinusEqual),
            ("*=", StarEqual),
            ("/=", SlashEqual),
        ]
        .iter()
        .copied()
        .collect();

        if let Some(&tt) = double_map.get(two_char.as_str()) {
            self.advance();
            self.advance();
            return Ok(Token::new(tt, two_char, line, col));
        }

        // Single-char operators
        let single_map: HashMap<char, TokenType> = [
            ('+', Plus),
            ('-', Minus),
            ('*', Star),
            ('/', Slash),
            ('%', Modulo),
            ('=', Equal),
            ('<', Less),
            ('>', Greater),
            ('!', Not),
            ('&', BitAnd),
            ('|', BitOr),
            ('^', BitXor),
            ('?', QuestionMark),
            ('@', At),
            ('$', Dollar),
            (',', Comma),
            (';', Semicolon),
            (':', Colon),
            ('(', LeftParen),
            (')', RightParen),
            ('[', LeftBracket),
            (']', RightBracket),
            ('{', LeftBrace),
            ('}', RightBrace),
            ('.', Dot),
        ]
        .iter()
        .copied()
        .collect();

        if let Some(&tt) = single_map.get(&ch) {
            self.advance();
            return Ok(Token::new(tt, ch.to_string(), line, col));
        }

        // Unknown
        self.advance();
        Ok(Token::new(Ident, format!("?{ch}?"), line, col))
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.token_type == Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_let_and_ident() {
        let mut lex = Lexer::new("let x = 42");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[0].token_type, KeywordLet);
        assert_eq!(toks[1].value, "x");
        assert_eq!(toks[2].token_type, Equal);
        assert_eq!(toks[3].token_type, IntLiteral);
        assert_eq!(toks[3].value, "42");
    }

    #[test]
    fn test_chinese_identifier() {
        let mut lex = Lexer::new("let 名字 = 42");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[1].value, "名字");
        assert_eq!(toks[3].token_type, IntLiteral);
        assert_eq!(toks[3].value, "42");
    }

    #[test]
    fn test_bool_literals() {
        let mut lex = Lexer::new("true false");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[0].token_type, BoolLiteral);
        assert_eq!(toks[0].value, "true");
        assert_eq!(toks[1].token_type, BoolLiteral);
        assert_eq!(toks[1].value, "false");
    }

    #[test]
    fn test_operators() {
        let mut lex = Lexer::new("a + b - c * d / e");
        let toks = lex.tokenize().unwrap();
        let ops: Vec<&str> = toks
            .iter()
            .filter(|t| matches!(t.token_type, Plus | Minus | Star | Slash))
            .map(|t| t.value.as_str())
            .collect();
        assert_eq!(ops, vec!["+", "-", "*", "/"]);
    }

    #[test]
    fn test_pipe_operator() {
        let mut lex = Lexer::new("x |> f |> g");
        let toks = lex.tokenize().unwrap();
        let pipes = toks.iter().filter(|t| t.token_type == Pipe).count();
        assert_eq!(pipes, 2);
    }

    #[test]
    fn test_range_expr() {
        let mut lex = Lexer::new("0..10");
        let toks = lex.tokenize().unwrap();
        let ddot = toks.iter().find(|t| t.token_type == DoubleDot);
        assert!(ddot.is_some());
    }

    #[test]
    fn test_string_escape() {
        let mut lex = Lexer::new("\"hello\\nworld\"");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[0].token_type, StringLiteral);
        assert_eq!(toks[0].value, "hello\nworld");
    }

    #[test]
    fn test_empty_source() {
        let mut lex = Lexer::new("");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].token_type, Eof);
    }

    #[test]
    fn test_attribute_macro() {
        let mut lex = Lexer::new("#[derive(Clone)]");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[0].token_type, Attribute);
        assert_eq!(toks[0].value, "#[derive(Clone)]");
    }

    #[test]
    fn test_all_keywords() {
        let mut lex = Lexer::new(
            "let fn return if else match for in while spawn async channel try catch use trait assert mut const type struct enum impl pub export ok error",
        );
        let toks = lex.tokenize().unwrap();
        let kw_count = toks
            .iter()
            .filter(|t| {
                matches!(
                    t.token_type,
                    KeywordLet
                        | KeywordFn
                        | KeywordReturn
                        | KeywordIf
                        | KeywordElse
                        | KeywordMatch
                        | KeywordFor
                        | KeywordIn
                        | KeywordWhile
                        | KeywordSpawn
                        | KeywordAsync
                        | KeywordChannel
                        | KeywordTry
                        | KeywordCatch
                        | KeywordUse
                        | KeywordTrait
                        | KeywordAssert
                        | KeywordMut
                        | KeywordConst
                        | KeywordType
                        | KeywordStruct
                        | KeywordEnum
                        | KeywordImpl
                        | KeywordPub
                        | KeywordExport
                        | KeywordOk
                        | KeywordError
                )
            })
            .count();
        assert_eq!(kw_count, 27);
    }

    #[test]
    fn test_number_overflow_is_error_not_ident() {
        // 路线 #8 修复：超 i64 范围整数/空 hex 必须返回清晰错误，不得误当标识符
        let overflow_ints = [
            "let x = 99999999999999999999", // 22 个 9，远超 i64::MAX
            "let x = 9223372036854775808",  // i64::MAX + 1
            "let x = 12345678901234567890", // 20 位 > i64::MAX
        ];
        for src in overflow_ints {
            let mut lex = Lexer::new(src);
            let res = lex.tokenize();
            assert!(res.is_err(), "超大整数应报错而非误当标识符: {}", src);
            let msg = format!("{:?}", res.unwrap_err());
            assert!(
                msg.contains("i64 范围"),
                "错误信息应指出 i64 范围溢出: {} -> {}",
                src,
                msg
            );
        }

        // 空十六进制字面量必须报错
        for src in ["let x = 0x", "let x = 0xG"] {
            let mut lex = Lexer::new(src);
            let res = lex.tokenize();
            assert!(res.is_err(), "空/非法 hex 应报错: {}", src);
            let msg = format!("{:?}", res.unwrap_err());
            assert!(
                msg.contains("十六进制") || msg.contains("hex"),
                "错误信息应指出 hex 问题: {} -> {}",
                src,
                msg
            );
        }

        // 合法字面量仍正确切分
        let mut lex = Lexer::new("let n = 42");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, IntLiteral);
        assert_eq!(toks[3].value, "42");

        let mut lex = Lexer::new("let z = 2E+10");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, FloatLiteral);
        assert_eq!(toks[3].value, "2E+10");
    }

    #[test]
    fn test_scientific_notation() {
        // 整数指数形式: 1e12 应识别为浮点字面量
        let mut lex = Lexer::new("let x = 1e12");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, FloatLiteral);
        assert_eq!(toks[3].value, "1e12");

        // 小数 + 负指数: 1.5e-3
        let mut lex = Lexer::new("let y = 1.5e-3");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, FloatLiteral);
        assert_eq!(toks[3].value, "1.5e-3");

        // 大写 E + 正指数: 2E+10
        let mut lex = Lexer::new("let z = 2E+10");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, FloatLiteral);
        assert_eq!(toks[3].value, "2E+10");

        // 普通整数不应被误判为浮点
        let mut lex = Lexer::new("let n = 42");
        let toks = lex.tokenize().unwrap();
        assert_eq!(toks[3].token_type, IntLiteral);
        assert_eq!(toks[3].value, "42");
    }
}
