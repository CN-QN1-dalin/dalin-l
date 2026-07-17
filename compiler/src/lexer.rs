/// Dalin L — 词法分析器
use crate::token::{Token, TokenType, TokenType::*};
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
                ' ' | '\t' | '\r' => { self.advance(); }
                '\n' => { self.advance(); }
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
            if ch == '\n' { break; }
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

    fn read_number(&mut self) -> (TokenType, String) {
        let start = self.pos;
        let mut has_dot = false;

        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                self.advance();
            } else if ch == '.' && !has_dot {
                if self.peek(1) == Some('.') { break; }
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        let text: String = self.chars[start..self.pos].iter().collect();
        if has_dot {
            // Try parsing as float
            if text.parse::<f64>().is_ok() {
                return (FloatLiteral, text);
            }
        }
        if text.parse::<i64>().is_ok() {
            return (IntLiteral, text);
        }
        (Ident, text)
    }

    fn read_string(&mut self, quote: char) -> Result<String, LexerError> {
        self.advance(); // skip opening quote
        let mut parts = String::new();

        loop {
            match self.current() {
                None => return Err(LexerError {
                    message: "Unterminated string".into(),
                    line: self.line,
                    column: self.column,
                }),
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
            let (tt, val) = self.read_number();
            return Ok(Token::new(tt, val, line, col));
        }

        // Strings
        if ch == '"' || ch == '\'' {
            let s = self.read_string(ch)?;
            let tt = if ch == '"' { StringLiteral } else { CharLiteral };
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
            ("->", Arrow), ("=>", DoubleArrow), ("|>", Pipe), ("<|", Pipe),
            ("==", DoubleEqual), ("!=", NotEqual), ("<=", LessEqual), (">=", GreaterEqual),
            ("&&", And), ("||", Or), ("..", DoubleDot), ("::", DoubleColon),
            ("+=", PlusEqual), ("-=", MinusEqual), ("*=", StarEqual), ("/=", SlashEqual),
        ].iter().cloned().collect();

        if let Some(&tt) = double_map.get(two_char.as_str()) {
            self.advance();
            self.advance();
            return Ok(Token::new(tt, two_char, line, col));
        }

        // Single-char operators
        let single_map: HashMap<char, TokenType> = [
            ('+', Plus), ('-', Minus), ('*', Star), ('/', Slash), ('%', Modulo),
            ('=', Equal), ('<', Less), ('>', Greater), ('!', Not),
            ('?', QuestionMark), ('@', At), ('$', Dollar),
            (',', Comma), (';', Semicolon), (':', Colon),
            ('(', LeftParen), (')', RightParen),
            ('[', LeftBracket), (']', RightBracket),
            ('{', LeftBrace), ('}', RightBrace),
            ('.', Dot),
        ].iter().cloned().collect();

        if let Some(&tt) = single_map.get(&ch) {
            self.advance();
            return Ok(Token::new(tt, ch.to_string(), line, col));
        }

        // Unknown
        self.advance();
        Ok(Token::new(Ident, format!("?{}?", ch), line, col))
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        const MAX_TOKENS: usize = 100_000;
        let mut tokens = Vec::new();
        loop {
            if tokens.len() >= MAX_TOKENS {
                return Err(LexerError {
                    message: format!("Too many tokens (max {})", MAX_TOKENS),
                    line: self.line,
                    column: self.column,
                });
            }
            let tok = self.next_token()?;
            let is_eof = tok.token_type == Eof;
            tokens.push(tok);
            if is_eof { break; }
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
        let ops: Vec<&str> = toks.iter()
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
        let mut lex = Lexer::new("let fn return if else match for in while spawn async channel try catch use trait assert mut const type struct enum impl pub export ok error");
        let toks = lex.tokenize().unwrap();
        let kw_count = toks.iter()
            .filter(|t| matches!(t.token_type,
                KeywordLet | KeywordFn | KeywordReturn | KeywordIf | KeywordElse |
                KeywordMatch | KeywordFor | KeywordIn | KeywordWhile | KeywordSpawn |
                KeywordAsync | KeywordChannel | KeywordTry | KeywordCatch | KeywordUse |
                KeywordTrait | KeywordAssert | KeywordMut | KeywordConst | KeywordType |
                KeywordStruct | KeywordEnum | KeywordImpl | KeywordPub | KeywordExport |
                KeywordOk | KeywordError
            ))
            .count();
        assert_eq!(kw_count, 27);
    }
}