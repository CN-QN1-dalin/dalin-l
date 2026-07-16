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
        effect: Option<String>,         // Dalin L 2.0: pure | io | async | spawn
        capability: Option<String>,     // Dalin L 2.0: cpu | gpu | sfa | net
        llm_prompt: Option<String>,     // @llm("...") 编译指令
        /// 置信度 @ proven | verified | inferred | generated | uncertain
        confidence: Option<String>,
        /// Phase C: 认知循环阶段 @ perceive | reason | decide | act | loop
        cognitive_loop: Option<String>,
        /// Phase C: 治理级别 @ gov(prepare) | gov(suggest) | gov(approve) | gov(execute)
        governance: Option<String>,
        /// Phase D: 延迟约束 @ latency(50ms)
        latency: Option<String>,
        /// Phase D: 超时约束 @ timeout(5s)
        timeout: Option<String>,
        /// Phase D: 吞吐量约束 @ throughput(100/s)
        throughput: Option<String>,
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
    /// @llm 编译指令：自然语言描述，编译器在编译时生成代码
    /// prompt: LLM 提示词描述要生成的行为
    /// target: 可选的目标函数名（若在 fn 外独立使用）
    Llm {
        prompt: String,
        target: Option<String>,
        effect: Option<String>,
        capability: Option<String>,
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
//  模块/包系统节点 (Phase H)
// ═══════════════════════════════

/// 属性宏: #[derive(Debug, Clone)]
#[derive(Debug, Clone)]
pub struct AttrDerive {
    pub name: String,         // "derive"
    pub traits: Vec<String>,   // ["Debug", "Clone", ...]
}

/// 模块声明: mod foo; 或 mod foo { ... }
#[derive(Debug, Clone)]
pub enum ModuleDecl {
    /// mod foo; — 从同目录下的 foo.dalin 加载
    External(String),
    /// mod foo { items... } — 内联模块
    Inline(String, Vec<Stmt>),
}

/// use 路径中的通配符 / 重命名
#[derive(Debug, Clone)]
pub enum UseTree {
    /// use foo::bar;
    Path(Vec<String>),
    /// use foo::*;
    Glob,
    /// use foo::{a, b, c};
    Group(Vec<(String, Option<String>)>),
}

/// 包版本字符串 (SemVer)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

/// dalin.toml 包描述
#[derive(Debug, Clone)]
pub struct PackageManifest {
    pub name: String,
    pub version: SemVer,
    pub edition: String,
    pub deps: Vec<(String, String)>,    // (name, version_req)
    pub stdlib_modules: Vec<String>,     // 标准库模块列表
}

/// 编译时宏声明: macro_rules! foo { ... }
#[derive(Debug, Clone)]
pub enum MacroDecl {
    Declarative {
        name: String,
        rules: Vec<MacroRule>,
    },
    Derive {
        name: String,
        target_type: String,
        body: Vec<Stmt>,
    },
}

/// macro_rules! 单条规则
#[derive(Debug, Clone)]
pub struct MacroRule {
    pub pattern: Vec<String>,   // token 模式
    pub expansion: Vec<String>, // 展开模板
}

// ═══════════════════════════════
//  Program
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
    /// Phase H: 顶层模块声明
    pub modules: Vec<ModuleDecl>,
    /// Phase H: use/import 列表
    pub uses: Vec<UseTree>,
    /// Phase H: 包描述符 (main crate)
    pub package_manifest: Option<Box<PackageManifest>>,
    /// Phase H: 宏声明 (在展开前收集)
    pub macros: Vec<MacroDecl>,
    /// Phase H: #[derive(...)] 属性
    pub derive_attrs: Vec<AttrDerive>,
}

impl Program {
    pub fn new() -> Self {
        Self {
            statements: Vec::new(),
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        }
    }

    pub fn add(&mut self, stmt: Stmt) {
        self.statements.push(stmt);
    }

    pub fn is_empty(&self) -> bool {
        self.statements.is_empty() && self.modules.is_empty()
    }
}
