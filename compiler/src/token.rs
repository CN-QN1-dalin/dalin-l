/// Dalin L — Token 类型定义
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    // ── 核心关键字 (17) ──
    KeywordLet,
    KeywordFn,
    KeywordReturn,
    KeywordIf,
    KeywordElse,
    KeywordMatch,
    KeywordFor,
    KeywordIn,
    KeywordWhile,
    KeywordSpawn,
    KeywordAsync,
    KeywordTry,
    KeywordCatch,
    KeywordUse,
    KeywordTrait,
    KeywordAssert,
    KeywordChannel,

    // ── 扩展关键字 (12) ──
    KeywordMut,
    KeywordOk,
    KeywordError,
    KeywordExport,
    KeywordPub,
    KeywordImpl,
    KeywordStruct,
    KeywordEnum,
    KeywordType,
    KeywordConst,
    KeywordMod,

    // ── 关键字/字面量 ──
    KeywordNull, // null — 空值关键字

    // ── 类型检查/转换 (Step 2: is/as) ──
    KeywordIs, // is — 类型检查 operator
    KeywordAs, // as — 类型转换 operator

    // ── 字面量 ──
    Ident,
    IntLiteral,
    FloatLiteral,
    StringLiteral,    // 纯字符串字面量 "hello world"
    InterpolateToken, // 含 $ident 的插值字符串 "hello $name!"
    CharLiteral,
    BoolLiteral,

    // ── 运算符 ──
    Plus,
    Minus,
    Star,
    Slash,
    Modulo,
    Equal,
    DoubleEqual,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    Not,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    Arrow,              // ->
    DoubleArrow,        // =>
    Pipe,               // |> or <|
    QuestionMark,       // ?
    DoubleQuestionMark, // ??
    ColonQuestion,      // ?:
    At,                 // @
    Dollar,             // $

    // ── 分隔符 ──
    Comma,
    Semicolon,
    Colon,
    DoubleColon, // ::
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Dot,
    DoubleDot, // ..

    // ── 特殊 ──
    Eof,
    Newline,
    Attribute, // #[...]

    // ── 注释 ──
    Comment,
}

impl TokenType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::KeywordLet => "KEYWORD_LET",
            Self::KeywordFn => "KEYWORD_FN",
            Self::KeywordReturn => "KEYWORD_RETURN",
            Self::KeywordIf => "KEYWORD_IF",
            Self::KeywordElse => "KEYWORD_ELSE",
            Self::KeywordMatch => "KEYWORD_MATCH",
            Self::KeywordFor => "KEYWORD_FOR",
            Self::KeywordIn => "KEYWORD_IN",
            Self::KeywordWhile => "KEYWORD_WHILE",
            Self::KeywordSpawn => "KEYWORD_SPAWN",
            Self::KeywordAsync => "KEYWORD_ASYNC",
            Self::KeywordTry => "KEYWORD_TRY",
            Self::KeywordCatch => "KEYWORD_CATCH",
            Self::KeywordUse => "KEYWORD_USE",
            Self::KeywordTrait => "KEYWORD_TRAIT",
            Self::KeywordAssert => "KEYWORD_ASSERT",
            Self::KeywordChannel => "KEYWORD_CHANNEL",
            Self::KeywordMut => "KEYWORD_MUT",
            Self::KeywordOk => "KEYWORD_OK",
            Self::KeywordError => "KEYWORD_ERROR",
            Self::KeywordExport => "KEYWORD_EXPORT",
            Self::KeywordPub => "KEYWORD_PUB",
            Self::KeywordImpl => "KEYWORD_IMPL",
            Self::KeywordStruct => "KEYWORD_STRUCT",
            Self::KeywordEnum => "KEYWORD_ENUM",
            Self::KeywordType => "KEYWORD_TYPE",
            Self::KeywordConst => "KEYWORD_CONST",
            Self::KeywordMod => "KEYWORD_MOD",
            Self::KeywordIs => "KEYWORD_IS",
            Self::KeywordAs => "KEYWORD_AS",
            Self::KeywordNull => "KEYWORD_NULL",
            Self::Ident => "IDENT",
            Self::IntLiteral => "INT_LITERAL",
            Self::FloatLiteral => "FLOAT_LITERAL",
            Self::StringLiteral => "STRING_LITERAL",
            Self::InterpolateToken => "INTERPOLATE_TOKEN",
            Self::CharLiteral => "CHAR_LITERAL",
            Self::BoolLiteral => "BOOL_LITERAL",
            Self::Plus => "PLUS",
            Self::Minus => "MINUS",
            Self::Star => "STAR",
            Self::Slash => "SLASH",
            Self::Modulo => "MODULO",
            Self::Equal => "EQUAL",
            Self::DoubleEqual => "DOUBLE_EQUAL",
            Self::NotEqual => "NOT_EQUAL",
            Self::Less => "LESS",
            Self::Greater => "GREATER",
            Self::LessEqual => "LESS_EQUAL",
            Self::GreaterEqual => "GREATER_EQUAL",
            Self::And => "AND",
            Self::Or => "OR",
            Self::Not => "NOT",
            Self::PlusEqual => "PLUS_EQUAL",
            Self::MinusEqual => "MINUS_EQUAL",
            Self::StarEqual => "STAR_EQUAL",
            Self::SlashEqual => "SLASH_EQUAL",
            Self::Arrow => "ARROW",
            Self::DoubleArrow => "DOUBLE_ARROW",
            Self::Pipe => "PIPE",
            Self::QuestionMark => "QUESTION_MARK",
            Self::DoubleQuestionMark => "DOUBLE_QUESTION_MARK",
            Self::ColonQuestion => "COLON_QUESTION",
            Self::At => "AT",
            Self::Dollar => "DOLLAR",
            Self::Comma => "COMMA",
            Self::Semicolon => "SEMICOLON",
            Self::Colon => "COLON",
            Self::DoubleColon => "DOUBLE_COLON",
            Self::LeftParen => "LEFT_PAREN",
            Self::RightParen => "RIGHT_PAREN",
            Self::LeftBracket => "LEFT_BRACKET",
            Self::RightBracket => "RIGHT_BRACKET",
            Self::LeftBrace => "LEFT_BRACE",
            Self::RightBrace => "RIGHT_BRACE",
            Self::Dot => "DOT",
            Self::DoubleDot => "DOUBLE_DOT",
            Self::Eof => "EOF",
            Self::Newline => "NEWLINE",
            Self::Attribute => "ATTRIBUTE",
            Self::Comment => "COMMENT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(token_type: TokenType, value: String, line: usize, column: usize) -> Self {
        Self {
            token_type,
            value,
            line,
            column,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} {:25} {:?}",
            self.line,
            self.column,
            self.token_type.name(),
            self.value
        )
    }
}
