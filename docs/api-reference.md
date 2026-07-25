# Dalin L 3.0 — API 参考

> **版本**: 3.0.0-dev  
> **最后更新**: 2026-07-25

---

## 目录

1. [编译器 API](#1-编译器-api)
2. [运行时 API](#2-运行时-api)
3. [DLVM 字节码格式](#3-dlvm-字节码格式)
4. [CLI 命令参考](#4-cli-命令参考)
5. [LSP 协议](#5-lsp-协议)

---

## 1. 编译器 API

### 1.1 词法分析器 (`dalin_compiler::lexer::Lexer`)

```rust
/// 将源码字符串转换为 Token 流
pub struct Lexer {
    source: String,
    position: usize,
}

impl Lexer {
    /// 创建新的词法分析器
    pub fn new(source: &str) -> Self;

    /// 逐 token 扫描（迭代器接口）
    pub fn next_token(&mut self) -> Result<Token, LexerError>;

    /// 收集所有 token
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError>;
}
```

**Token 类型**：
```rust
pub enum Token {
    // 字面量
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),

    // 标识符与关键字
    Ident(String),
    KeyFn, KeyLet, KeyReturn, KeyIf, KeyElse, KeyWhile,
    KeyMatch, KeyStruct, KeyEnum, KeyTrait, KeyImpl,
    KeyTrue, KeyFalse, KeyNone, KeyTry, KeyCatch,
    KeySpawn, KeyAsync, KeyAwait, KeySend, KeyRecv,

    // 符号
    Plus, Minus, Star, Slash, Percent,
    Eq, EqEq, Bang, BangEq,
    Lt, Gt, LtEq, GtEq,
    AndAnd, OrOr,
    Arrow, FatArrow,
    Dot, Comma, Colon, Semicolon,
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Pipe, At, Hash,

    // 特殊
    Annotation(String),  // @pure, @cpu 等
    Eof,
}
```

### 1.2 语法分析器 (`dalin_compiler::parser::Parser`)

```rust
/// 将 Token 流解析为 AST Program
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    /// 创建解析器
    pub fn new(tokens: Vec<Token>) -> Self;

    /// 解析完整程序
    pub fn parse(&mut self) -> Result<Program, ParseError>;

    /// 解析表达式
    pub fn parse_expr(&mut self) -> Result<Expr, ParseError>;

    /// 解析语句
    pub fn parse_stmt(&mut self) -> Result<Stmt, ParseError>;

    /// 错误恢复：跳过错误 token 到下一个同步点
    pub fn synchronize(&mut self);
}
```

**AST 节点**：
```rust
pub struct Program {
    pub functions: Vec<FnDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub traits: Vec<TraitDef>,
    pub impls: Vec<ImplDef>,
}

pub enum Expr {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    CharLiteral(char),
    NoneLiteral,
    Ident(String),
    ArrayLiteral(Vec<Expr>),
    StructLiteral(String, Vec<(String, Expr)>),
    BinaryOp { left: Box<Expr>, op: String, right: Box<Expr> },
    UnaryOp { op: String, right: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    If { cond: Box<Expr>, then: Box<Expr>, else_: Option<Box<Expr>> },
    Match { target: Box<Expr>, arms: Vec<MatchArm> },
    FieldAccess { obj: Box<Expr>, field: String },
    Index { arr: Box<Expr>, idx: Box<Expr> },
    Lambda { params: Vec<FnParam>, body: Vec<Stmt> },
    OptionValue { is_some: bool, value: Option<Box<Expr>> },
    ResultValue { is_ok: bool, value: Option<Box<Expr>>, error: Option<Box<Expr>> },
    InterpolatedString(Vec<Expr>),
    Pipe { left: Box<Expr>, right: Box<Expr> },
}

pub enum Stmt {
    Let { name: String, type_annotation: Option<TypeRef>, value: Expr },
    Return(Option<Expr>),
    Expr(Expr),
    If { cond: Expr, then: Vec<Stmt>, else_: Vec<Stmt> },
    While { cond: Expr, body: Vec<Stmt> },
    Match { target: Expr, arms: Vec<MatchArm> },
    Assign { name: String, value: Expr },
    TryCatch { try_block: Vec<Stmt>, catch_var: Option<String>, catch_block: Vec<Stmt> },
    FnDef(FnDef),
    StructDef(StructDef),
    EnumDef(EnumDef),
    TraitDef(TraitDef),
    ImplDef(ImplDef),
}
```

### 1.3 类型检查器 (`dalin_compiler::ty2`)

```rust
/// 七通道类型检查系统
pub mod tier1;  // 基础类型推断
pub mod tier2;  // 效应/能力检查
pub mod tier3;  // 置信度/认知循环检查

pub fn infer_type(program: &Program) -> TypeResult;
pub fn check_effects(program: &Program, context: &EffectContext) -> Result<(), EffectError>;
pub fn check_capabilities(program: &Program, context: &CapContext) -> Result<(), CapError>;
```

### 1.4 字节码缓存 (`dalin_compiler::cache`)

```rust
/// 增量编译缓存
pub struct BytecodeCache {
    cache_dir: PathBuf,
}

impl BytecodeCache {
    pub fn new(cache_dir: Option<PathBuf>) -> Self;

    /// 计算文件哈希
    pub fn hash_file(path: &Path) -> Result<u64, CacheError>;

    /// 检查缓存是否存在
    pub fn get_cached(path: &Path) -> Option<Vec<u8>>;

    /// 写入缓存
    pub fn set_cached(path: &Path, bytecode: &[u8]) -> Result<(), CacheError>;

    /// 清理过期缓存
    pub fn clean(&self) -> Result<usize, CacheError>;
}
```

---

## 2. 运行时 API

### 2.1 解释器 (`dalin_runtime::interpreter::Interpreter`)

```rust
/// Dalin L 运行时解释器
pub struct Interpreter {
    globals: Environment,
    functions: HashMap<String, FnValue>,
    structs: HashMap<String, Vec<(String, Option<TypeRef>)>>,
    enums: HashMap<String, Vec<String>>,
    profiler: Option<Profiler>,
}

impl Interpreter {
    /// 创建新解释器
    pub fn new() -> Self;

    /// 执行已编译的程序
    pub fn interpret(&mut self, prog: &Program) -> Result<Vec<Value>, RuntimeError>;

    /// 执行表达式
    pub fn eval_expr(&mut self, expr: &Expr, env: &mut Environment) -> Result<Value, RuntimeError>;

    /// 执行语句
    pub fn eval_stmt(&mut self, stmt: &Stmt, env: &mut Environment) -> Result<(), RuntimeError>;

    /// 调用函数
    pub fn call_fn(&mut self, name: &str, args: &[Value]) -> Result<Value, RuntimeError>;
}

/// 便捷入口
pub fn run_source(source: &str) -> Result<Vec<Value>, RuntimeError>;
pub fn run_source_with_tree(source: &str) -> Result<String, RuntimeError>;
```

### 2.2 环境 (`dalin_runtime::env::Environment`)

```rust
/// 词法作用域环境
pub struct Environment {
    parent: Option<Box<Environment>>,
    variables: HashMap<String, Value>,
}

impl Environment {
    /// 创建根环境
    pub fn new() -> Self;

    /// 创建子作用域
    pub fn child(&self) -> Self;

    /// 定义变量
    pub fn define(&mut self, name: &str, value: Value);

    /// 查找变量（沿作用域链向上）
    pub fn lookup(&self, name: &str) -> Option<Value>;

    /// 赋值变量（沿作用域链向上）
    pub fn assign(&mut self, name: &str, value: Value) -> bool;
}

/// 运行时值
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
    None,
    Array(Vec<Value>),
    Option(bool, Option<Box<Value>>),           // (is_some, value)
    Result(bool, Option<Box<Value>>, Option<Box<Value>>), // (is_ok, value, error)
    Function(FnValue),
    Struct(HashMap<String, Value>),
    EnumVariant(String, String),                 // (enum_name, variant_name)
    Task(String),
    ChannelSender(Arc<Sender<Value>>),
    ChannelReceiver(String),
}

pub struct FnValue {
    pub name: String,
    pub params: Vec<FnParam>,
    pub body: Vec<Stmt>,
    pub closure: Environment,
    pub return_type: Option<TypeRef>,
    pub effect: Option<String>,
    pub capability: Option<String>,
}
```

### 2.3 Profiler (`dalin_runtime::profiler::Profiler`)

```rust
/// 采样式函数级 Profiler
pub struct Profiler {
    calls: HashMap<String, CallStats>,
    stack: Vec<(String, Instant)>,
}

pub struct CallStats {
    pub count: u64,
    pub total_ms: f64,
    pub max_ms: f64,
}

impl Profiler {
    pub fn new() -> Self;
    pub fn start_call(&mut self, name: &str);
    pub fn end_call(&mut self, name: &str);
    pub fn report(&self) -> String;
    pub fn reset(&mut self);
}
```

### 2.4 错误类型

```rust
#[derive(Debug, Clone)]
pub struct RuntimeError(pub String);

// 常见错误场景
// - "Undefined variable: ..."
// - "Unexpected token: ..."
// - "Division by zero"
// - "Function not found: ..."
// - "Type mismatch: ..."
```

---

## 3. DLVM 字节码格式

### 3.1 Opcode 列表

| Opcode | 操作码 | 说明 |
|--------|--------|------|
| `Nop` | 0x00 | 空操作 |
| `PushInt(i64)` | 0x01 | 压入整数 |
| `PushFloat(f64)` | 0x02 | 压入浮点 |
| `PushString(String)` | 0x03 | 压入字符串 |
| `PushBool(bool)` | 0x04 | 压入布尔 |
| `PushNone` | 0x05 | 压入空值 |
| `PushArray(usize)` | 0x06 | 构建数组 |
| `LoadLocal(usize)` | 0x10 | 加载局部变量 |
| `StoreLocal(usize)` | 0x11 | 存储局部变量 |
| `LoadGlobal(usize)` | 0x12 | 加载全局变量 |
| `StoreGlobal(usize)` | 0x13 | 存储全局变量 |
| `Add` | 0x20 | 整数加法 |
| `Sub` | 0x21 | 整数减法 |
| `Mul` | 0x22 | 整数乘法 |
| `Div` | 0x23 | 整数除法 |
| `FAdd` | 0x24 | 浮点加法 |
| `FSub` | 0x25 | 浮点减法 |
| `FMul` | 0x26 | 浮点乘法 |
| `FDiv` | 0x27 | 浮点除法 |
| `Eq` | 0x30 | 相等比较 |
| `Neq` | 0x31 | 不等比较 |
| `Lt` | 0x32 | 小于 |
| `Gt` | 0x33 | 大于 |
| `Lte` | 0x34 | 小于等于 |
| `Gte` | 0x35 | 大于等于 |
| `Jmp(usize)` | 0x40 | 无条件跳转 |
| `JmpIf(usize)` | 0x41 | 条件跳转 |
| `Call(String)` | 0x50 | 函数调用 |
| `Return` | 0x51 | 函数返回 |
| `Spawn` | 0x60 | 协程启动 |
| `Await` | 0x61 | 协程等待 |
| `Send` | 0x62 | 通道发送 |
| `Recv` | 0x63 | 通道接收 |
| `Builtin(usize)` | 0xF0 | 内置函数调用 |

### 3.2 字节码布局

```
[magic: 4 bytes] [version: 2 bytes] [num_functions: 4 bytes]
[function_table: var]
  for each function:
    [name_len: 2 bytes] [name: name_len bytes] [num_ops: 4 bytes] [ops: num_ops * 4 bytes]
[data_section: var]
  [string_table]
  [constant_pool]
```

---

## 4. CLI 命令参考

### `dalib run`

编译并运行 Dalin L 源文件：

```bash
dalib run --input main.dal
dalib run --watch  # 监视模式，自动重编译
```

### `dalib build`

编译为字节码或 native 二进制：

```bash
dalib build --output out.dalc     # 字节码输出
dalib build --native --output app  # (需要 LLVM) native 二进制
```

### `dalib profile`

性能分析：

```bash
dalib profile my_program.dal
```

### `dalib test`

运行测试：

```bash
dalib test                    # 运行所有测试
dalib test --test "fib"       # 运行匹配名称的测试
```

### `dalib fmt`

格式化代码：

```bash
dalib fmt input.dal
dalib fmt --check *.dal       # 仅检查格式
```

### `dalib repl`

启动交互式 REPL：

```bash
dalib repl
> let x = 42
> x + 1
43
```

### `dalib evolve`

自进化工具：

```bash
dalib evolve stats            # 查看进化统计
dalib evolve review           # 审查进化提议
```

---

## 5. LSP 协议

### 5.1 支持的功能

| 功能 | 说明 |
|------|------|
| 语法高亮 | TextMate 语法定义 |
| 自动补全 | 关键字、标识符、标准库函数 |
| 跳转定义 | 函数/变量/类型定义跳转 |
| 悬停信息 | 类型签名、文档注释 |
| 错误诊断 | 实时语法/类型错误标记 |
| 重命名符号 | 跨文件符号重命名 |
| 文档符号 | 文件内符号列表 |
| 工作区符号 | 工作区符号搜索 |

### 5.2 启动

```bash
dalin-ls                    # 启动 LSP server
dalin-ls --port 8080        # 指定端口
dalin-ls --stdio            # 通过 stdio 通信
```

### 5.3 VS Code 扩展

VS Code 扩展 (dalin-l-vscode) 提供：
- 语法高亮 (`.dal` 文件)
- 集成 LSP 支持
- 代码片段与自动补全
- 调试适配器 (DAP) 集成