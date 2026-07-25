# Dalin L 3.0 — 语言规范

> **版本**: 3.0.0-dev  
> **最后更新**: 2026-07-25  
> **状态**: 草案

---

## 目录

1. [词法结构](#1-词法结构)
2. [语法结构](#2-语法结构)
3. [类型系统](#3-类型系统)
4. [运行时模型](#4-运行时模型)
5. [标准库](#5-标准库)
6. [附录](#6-附录)

---

## 1. 词法结构

### 1.1 标识符

Dalin L 支持 **中文标识符全链路**。标识符由 Unicode 字母、数字、下划线组成，不能以数字开头。

```dal
let 名字 = "大林"
let user_name = "alice"
fn 阶乘(n) { return n * 阶乘(n - 1) }
```

### 1.2 关键字

```text
fn, let, return, if, else, while, match, struct, enum,
trait, impl, mod, pub, import, as, true, false, none,
try, catch, spawn, async, await, send, recv
```

### 1.3 字面量

| 类型 | 示例 | 说明 |
|------|------|------|
| 整数 | `42`, `-1`, `0xFF` | i64 范围 |
| 浮点 | `3.14`, `-0.5`, `1e10` | f64 精度 |
| 字符串 | `"hello"`, `"abc\n"` | 双引号，支持转义 |
| 布尔 | `true`, `false` | 布尔字面量 |
| 字符 | `'a'`, `'好'` | Unicode 字符 |
| 数组 | `[1, 2, 3]` | 同构数组 |
| 空值 | `none` | null/unit 值 |

### 1.4 注释

```dal
// 单行注释
/* 块注释 */
```

### 1.5 操作符

```text
算术: +, -, *, /, %
比较: ==, !=, <, >, <=, >=
逻辑: &&, ||, !
赋值: =
管道: |>
成员: .
```

---

## 2. 语法结构

### 2.1 程序结构

一个 Dalin L 程序由一系列顶层声明组成：

```dal
// 函数定义
fn main() @ pure @ cpu {
    return 42
}

// 带类型标注的函数
fn add(a: int, b: int) -> int @ pure @ cpu {
    return a + b
}

// 结构体定义
struct Point {
    x: int,
    y: int
}

// 枚举（未标记）
enum Color {
    Red,
    Green,
    Blue
}

// Trait 定义
trait Display {
    fn to_string(self) -> string
}

// Trait 实现
impl Display for int {
    fn to_string(self) -> string {
        return str(self)
    }
}
```

### 2.2 函数定义

```dal
// 匿名函数
fn (x) -> x * x

// 命名函数
fn greet(name) @ pure @ cpu {
    return "Hello, " + name
}

// 带 effect/capability 标注的函数
fn read_file(path) @ io @ filesystem {
    return file_read(path)
}

// 异步函数（spawn 协程）
async fn process(data) @ spawn @ cpu {
    return heavy_computation(data)
}
```

### 2.3 变量绑定

```dal
let x = 42
let name: string = "Alice"
let nums = [1, 2, 3]
```

### 2.4 控制流

**If/Else 表达式**：
```dal
let result = if x > 0 {
    "positive"
} else if x < 0 {
    "negative"
} else {
    "zero"
}
```

**While 循环**：
```dal
let i = 0
while i < 10 {
    print(i)
    let i = i + 1
}
```

**Match 表达式**：
```dal
match x {
    1 => "one",
    2 => "two",
    _ => "other"
}

// 带守卫的模式
match x {
    v if v > 0 => "positive",
    v if v < 0 => "negative",
    _ => "zero"
}

// 解构元组
match pair {
    (0, 0) => "origin",
    (x, 0) => "x-axis: " + str(x),
    (0, y) => "y-axis: " + str(y),
    (x, y) => str(x) + ", " + str(y)
}

// Enum 变体匹配
match opt {
    Some(v) => "value: " + str(v),
    None => "no value"
}
```

### 2.5 结构体

```dal
struct Point { x: int, y: int }

// 构造
let p = Point(3, 4)

// 字段访问
let x = p.x
```

### 2.6 枚举

```dal
enum Option<T> {
    Some(T),
    None
}

enum Result<T, E> {
    Ok(T),
    Err(E)
}

let opt = Some(42)
let res = Ok("success")
```

### 2.7 错误处理 (Try/Catch)

```dal
try {
    risky_operation()
} catch (e) {
    print("Error: " + e)
}

// 多 catch 分支
try {
    operation()
} catch (err) {
    fallback()
}
```

### 2.8 字符串插值

```dal
let name = "world"
let greeting = "Hello, " + name + "!"  // "Hello, world!"
let sum_str = "sum: " + str(42)         // "sum: 42"
```

---

## 3. 类型系统

### 3.1 基础类型

| 类型 | 描述 | 值域 |
|------|------|------|
| `int` | 64 位整数 | -2^63 ~ 2^63-1 |
| `float` | 64 位浮点 | IEEE 754 double |
| `string` | UTF-8 字符串 | 变长 |
| `bool` | 布尔值 | `true` / `false` |
| `char` | Unicode 字符 | 单个码点 |
| `none` | 空类型 | 仅 `none` |

### 3.2 复合类型

```dal
// 数组
[1, 2, 3] : list<int>

// 结构体
Point(x, y) : struct { x: int, y: int }

// 枚举
Some(42) : Option<int>

// Option/Result
Result(true, 42, none) : Result<int, string>
```

### 3.3 七通道类型系统

Dalin L 3.0 的七通道系统在传统类型之外扩展了运行时维度：

| 通道 | 标签 | 说明 |
|------|------|------|
| Effect | `@pure`, `@io`, `@spawn` | 副作用分类 |
| Capability | `@cpu`, `@memory`, `@network` | 资源能力 |
| Confidence | `@high`, `@medium`, `@low` | 结果置信度 |
| Governance | 级别枚举 | 治理合规 |
| Latency | 毫秒/秒 | 延迟约束 |
| Cognitive Loop | 阶段枚举 | 认知循环 |
| QN | 向量 | QN1 认知维度 |

### 3.4 HM 类型推断

Dalin L 使用 Hindley-Milner 类型推断：

```dal
// 自动推断类型
fn id(x) { return x }          // id: ∀T. T → T
fn compose(f, g, x) { return f(g(x)) }  // 多态推断

// 显式标注
fn add(a: int, b: int) -> int {
    return a + b
}
```

### 3.5 泛型与 Trait

```dal
trait Add {
    fn add(self, other: Self) -> Self
}

impl Add for int {
    fn add(self, other: int) -> int { return self + other }
}

fn generic_add<T: Add>(a: T, b: T) -> T {
    return a.add(b)
}
```

---

## 4. 运行时模型

### 4.1 值表示

运行时值使用 `Value` 枚举：

```rust
enum Value {
    Int(i64),                          // 整数
    Float(f64),                        // 浮点
    String(String),                    // 字符串
    Bool(bool),                        // 布尔
    Char(char),                        // 字符
    None,                              // 空值
    Array(Vec<Value>),                 // 数组
    Option(bool, Option<Box<Value>>),  // Option
    Result(bool, Box<Value>, Box<Value>), // Result
    Function(FnValue),                 // 函数引用
    Struct(HashMap<String, Value>),    // 结构体
    EnumVariant(String, String),       // 枚举变体
    Task(String),                      // 任务句柄
    ChannelSender(...),                // 通道发送端
    ChannelReceiver(...),              // 通道接收端
}
```

### 4.2 环境模型

词法作用域，支持嵌套：

```dal
let outer = 1
{
    let inner = 2
    // inner 可见，outer 可见
}
// inner 不可见，outer 可见
```

### 4.3 协程与并发

```dal
async fn heavy_compute(x) @ spawn @ cpu {
    return x * x
}

fn main() @ pure @ cpu {
    let task = spawn heavy_compute(42)
    let result = await task
    return result
}
```

### 4.4 Profiler

内置采样式 profiler，支持函数级调用追踪：

```bash
dalib profile my_program.dal
```

输出示例：
```
=== Dalin L 3.0 Profiler Report ===
Total time: 150ms

Function              Count     Total (ms)    Max (ms)
─────────────────────────────────────────────────────────
main                       1        150.0       150.0
parse_tokens               5         12.3         4.1
```

### 4.5 字节码缓存

Dalin L 支持增量编译，通过 `.dalin_cache/` 目录缓存已编译的字节码：

```bash
dalib run my_program.dal  # 自动检测并缓存
```

---

## 5. 标准库

### 5.1 核心模块（stdlib/）

| 文件 | 内容 |
|------|------|
| `core_types.dal` | 类型工具函数 |
| `math.dal` | 数学运算 |
| `strings.dal` | 字符串操作 |
| `collections.dal` | 集合类型 |
| `json.dal` | JSON 解析 |
| `time.dal` | 时间操作 |
| `logging.dal` | 日志系统 |
| `testing.dal` | 测试辅助 |
| `crypto.dal` | 加密原语 |
| `networking.dal` | 网络操作 |
| `encoding.dal` | 编码工具 |
| `errors.dal` | 错误类型 |
| ... | 共 33 个模块 |

### 5.2 内置函数

| 函数 | 签名 | 说明 |
|------|------|------|
| `print` | `print(...)` | 打印到 stdout |
| `println` | `println(...)` | 打印并换行 |
| `len` | `len(x) -> int` | 数组/字符串长度 |
| `push` | `push(arr, val) -> arr` | 数组追加 |
| `int` | `int(x) -> int` | 转换为整数 |
| `float` | `float(x) -> float` | 转换为浮点 |
| `str` | `str(x) -> string` | 转换为字符串 |
| `abs` | `abs(x) -> int` | 绝对值 |

---

## 6. 附录

### 6.1 CLI 命令参考

```bash
dalib run        # 编译并运行
dalib build      # 编译为字节码
dalib test       # 运行测试
dalib lint       # 代码检查
dalib fmt        # 格式化代码
dalib profile    # 性能分析
dalib repl       # 交互式 REPL
dalib evolve     # 自进化工具
dalib lang       # 语言信息
```

### 6.2 与 Rust 的对应关系

| Dalin L | Rust | 说明 |
|---------|------|------|
| `fn` | `fn` | 函数定义 |
| `let` | `let` | 变量绑定 |
| `struct` | `struct` | 结构体 |
| `enum` | `enum` | 枚举 |
| `match` | `match` | 模式匹配 |
| `if` | `if` | 条件表达式 |
| `while` | `while` | 循环 |
| `none` | `()` | unit 类型 |
| `Option` | `Option` | 可选值 |
| `Result` | `Result` | 结果类型 |
| `trait` | `trait` | 接口定义 |
| `impl` | `impl` | 接口实现 |
| `try/catch` | `panic/catch` | 错误处理 |