/// Dalin L — AST 节点定义

use std::fmt;

// ═══════════════════════════════
//  类型表示
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum BaseType {
    Int,
    Float,
    String,
    Bool,
    Char,
    None,
    Array,
    Option,
    Result,
    Func,
    Unknown,
}

impl fmt::Display for BaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => write!(f, "int"),
            Self::Float => write!(f, "float"),
            Self::String => write!(f, "string"),
            Self::Bool => write!(f, "bool"),
            Self::Char => write!(f, "char"),
            Self::None => write!(f, "none"),
            Self::Array => write!(f, "array"),
            Self::Option => write!(f, "option"),
            Self::Result => write!(f, "result"),
            Self::Func => write!(f, "func"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub base: BaseType,
    pub generic_arg: Option<Box<TypeRef>>,
    pub result_err: Option<Box<TypeRef>>,
}

impl TypeRef {
    pub fn new(base: BaseType) -> Self {
        Self { base, generic_arg: None, result_err: None }
    }

    pub fn generic(base: BaseType, arg: TypeRef) -> Self {
        Self { base, generic_arg: Some(Box::new(arg)), result_err: None }
    }

    pub fn result(ok: TypeRef, err: TypeRef) -> Self {
        Self { base: BaseType::Result, generic_arg: Some(Box::new(ok)), result_err: Some(Box::new(err)) }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(arg) = &self.generic_arg {
            write!(f, "{}<{}>", self.base, arg)?;
            if let Some(err) = &self.result_err {
                write!(f, ", {}>", err)?;
            }
            Ok(())
        } else if let Some(err) = &self.result_err {
            write!(f, "result<{}, {}>", self.base, err)
        } else {
            write!(f, "{}", self.base)
        }
    }
}

// ═══════════════════════════════
//  表达式节点
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub enum Expr {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    CharLiteral(char),
    Ident(String),
    BinaryOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
    UnaryOp {
        op: String,
        operand: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    MemberAccess {
        object: Box<Expr>,
        member: String,
    },
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Pipe {
        input: Box<Expr>,
        ops: Vec<(String, Expr)>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Array(Vec<Expr>),
    OptionValue {
        is_some: bool,
        value: Option<Box<Expr>>,
    },
    ResultValue {
        is_ok: bool,
        value: Option<Box<Expr>>,
        error: Option<Box<Expr>>,
    },
    /// if/match 作为表达式（从语句转换而来）
    IfExpr(Box<Expr>, Box<Expr>, Box<Expr>),    // (condition, then, else)
    MatchExpr(Box<Expr>, Vec<MatchArm>),         // (target, arms)
}

// ═══════════════════════════════
//  Pattern 节点
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: String,     // 'wild' | 'ident' | 'lit' | 'ctor' | 'struct'
    pub name: String,
    pub binding: Option<String>,
    pub inner: Vec<Pattern>,
    pub fields: Vec<(String, String)>,
    pub value: Option<Box<Expr>>,
}

// ═══════════════════════════════
//  MatchArm
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
}

// ═══════════════════════════════
//  语句节点
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        value: Option<Box<Expr>>,
        type_annotation: Option<TypeRef>,
        mutable: bool,
    },
    Const {
        name: String,
        value: Option<Box<Expr>>,
        type_annotation: Option<TypeRef>,
    },
    Fn {
        name: String,
        params: Vec<FnParam>,
        return_type: Option<TypeRef>,
        body: Vec<Stmt>,
        async_: bool,
        pub_: bool,
    },
    Return(Option<Box<Expr>>),
    If {
        condition: Box<Expr>,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    While {
        condition: Box<Expr>,
        body: Vec<Stmt>,
    },
    For {
        target: String,
        iterable: Box<Expr>,
        body: Vec<Stmt>,
    },
    Match {
        target: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    StructDef {
        name: String,
        derives: Vec<String>,
        fields: Vec<FieldDef>,
    },
    EnumDef {
        name: String,
        variants: Vec<EnumVariant>,
    },
    TraitDef {
        name: String,
        methods: Vec<TraitMethod>,
    },
    ImplBlock {
        trait_name: Option<String>,
        type_name: String,
        methods: Vec<FnParam>,
    },
    Spawn {
        fn_decl: Box<Stmt>,
    },
    Channel {
        send_name: String,
        recv_name: String,
        elem_type: TypeRef,
        capacity: usize,
    },
    TryCatch {
        try_body: Vec<Stmt>,
        catch_param: Option<String>,
        catch_body: Vec<Stmt>,
    },
    Assert {
        condition: Box<Expr>,
        message: Option<Box<Expr>>,
    },
    Use(String),
    Export(String),
    TypeAlias {
        name: String,
        aliased_type: Option<TypeRef>,
    },
    Expr(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct FnParam {
    pub name: String,
    pub type_annotation: Option<TypeRef>,
    pub default: Option<Box<Expr>>,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_annotation: TypeRef,
}

#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name: String,
    pub return_type: Option<TypeRef>,
    pub params: Vec<FnParam>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<TypeRef>,
}

// ═══════════════════════════════
//  Program
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

impl Program {
    pub fn new() -> Self {
        Self { statements: Vec::new() }
    }

    pub fn add(&mut self, stmt: Stmt) {
        self.statements.push(stmt);
    }

    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}
