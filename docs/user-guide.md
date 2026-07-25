# Dalin L 3.0 — 用户指南

> **版本**: 3.0.0-dev  
> **最后更新**: 2026-07-25

---

## 目录

1. [快速开始](#1-快速开始)
2. [语言基础](#2-语言基础)
3. [进阶主题](#3-进阶主题)
4. [最佳实践](#4-最佳实践)
5. [与主流语言对比](#5-与主流语言对比)
6. [故障排除](#6-故障排除)

---

## 1. 快速开始

### 1.1 安装

要求：Rust 1.95+（必须支持 edition 2024）

```bash
git clone https://github.com/CN-QN1-dalin/dalin-l.git
cd dalin-l

# 构建（默认）
cargo build --release

# 构建（含 Native Codegen，需要 LLVM）
cargo build --release --features llvm
```

### 1.2 第一个程序

创建 `hello.dal`：

```dal
fn main() @ pure @ cpu {
    println("Hello, Dalin L!")
    return 0
}
```

运行：

```bash
dalib run --input hello.dal
```

输出：

```
Hello, Dalin L!
```

### 1.3 Hello World 详解

| 部分 | 说明 |
|------|------|
| `fn main()` | 程序入口函数 |
| `@ pure @ cpu` | 效应标注：纯函数 + CPU 能力 |
| `println(...)` | 内置函数，打印并换行 |
| `return 0` | 返回值 |

---

## 2. 语言基础

### 2.1 变量与常量

```dal
// 变量绑定
let x = 42
let name = "Alice"
let pi = 3.14159
let is_ready = true

// 可变变量（通过 let 重新绑定）
let counter = 0
let counter = counter + 1
```

### 2.2 函数

```dal
// 简单函数
fn greet(name) @ pure @ cpu {
    return "Hello, " + name
}

// 多参数函数
fn add(a, b, c) @ pure @ cpu {
    return a + b + c
}

// 递归函数
fn factorial(n) @ pure @ cpu {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}
```

### 2.3 条件判断

```dal
// if/else 表达式
let status = if score >= 60 {
    "pass"
} else {
    "fail"
}

// 链式 if/else
let grade = if score >= 90 {
    "A"
} else if score >= 80 {
    "B"
} else if score >= 70 {
    "C"
} else {
    "D"
}
```

### 2.4 循环

```dal
// while 循环
let i = 0
while i < 10 {
    println(i)
    let i = i + 1
}

// 计算 1 到 100 的和
let sum = 0
let i = 1
while i <= 100 {
    let sum = sum + i
    let i = i + 1
}
```

### 2.5 数组

```dal
// 创建数组
let nums = [1, 2, 3, 4, 5]

// 长度
let count = len(nums)  // 5

// 索引访问
let first = nums[0]   // 1
let last = nums[4]    // 5

// 追加
let nums = push(nums, 6)  // [1, 2, 3, 4, 5, 6]
```

### 2.6 模式匹配

```dal
// 基础匹配
match x {
    1 => "one",
    2 => "two",
    _ => "other"
}

// 带守卫
match n {
    v if v > 0 => "positive",
    v if v < 0 => "negative",
    _ => "zero"
}

// 解构元组
match pair {
    (0, 0) => "origin",
    (x, 0) => "x-axis: " + str(x),
    (_, _) => "somewhere"
}

// Option 匹配
match opt {
    Some(v) => "value: " + str(v),
    None => "no value"
}
```

### 2.7 结构体

```dal
// 定义
struct Point {
    x: int,
    y: int
}

// 构造
let p = Point(3, 4)

// 访问
let distance = p.x * p.x + p.y * p.y
```

### 2.8 错误处理

```dal
// try/catch
try {
    let result = risky_operation()
    println("Success: " + str(result))
} catch (err) {
    println("Error: " + err)
}
```

---

## 3. 进阶主题

### 3.1 模块与导入

```dal
// 导入标准库
import "math"
import "json"

// 导入用户模块
mod utils
mod helpers

// 使用导入的模块
let data = json.parse(raw)
let avg = math.average(nums)
```

### 3.2 七通道类型标注

```dal
// 纯函数 + CPU 能力
fn compute(x) @ pure @ cpu { return x * x }

// IO 操作 + 网络能力
fn fetch(url) @ io @ network { return http_get(url) }

// 协程 + 内存能力
async fn process(data) @ spawn @ memory { return analyze(data) }

// 高置信度 + 认知循环
fn verify(x) @ high @ cognitive { return double_check(x) }
```

### 3.3 协程与并发

```dal
// 异步函数
async fn compute(x) @ spawn @ cpu {
    return heavy_calculation(x)
}

// 启动协程
let task = spawn compute(42)

// 等待结果
let result = await task

// 通道通信
let (sender, receiver) = channel()
send(sender, "hello")
let msg = recv(receiver)
```

### 3.4 自进化协议

Dalin L 3.0 内置自进化闭环：

```bash
# 运行诊断
dalib evolve stats

# 查看进化提议
dalib evolve review

# 应用进化（需审批）
dalib evolve apply
```

### 3.5 性能分析

```bash
# 使用内置 profiler
dalib profile my_program.dal

# 输出示例
# === Dalin L 3.0 Profiler Report ===
# Total time: 45.2ms
#
# Function              Count     Total (ms)    Max (ms)
# ─────────────────────────────────────────────────────────
# main                       1         45.2        45.2
# process_data              10         28.1         5.3
# validate                  15         12.4         1.2
```

### 3.6 增量编译

Dalin L 自动缓存已编译的字节码：

```bash
# 第一次编译（完整编译）
dalib run --input main.dal

# 第二次运行（如果文件未修改，跳过编译直接运行）
dalib run --input main.dal
```

缓存目录：`.dalin_cache/`（自动 gitignored）

---

## 4. 最佳实践

### 4.1 命名规范

| 元素 | 规范 | 示例 |
|------|------|------|
| 变量 | snake_case | `user_name`, `count` |
| 函数 | snake_case | `get_data`, `compute` |
| 常量 | UPPER_SNAKE | `MAX_SIZE`, `PI` |
| 结构体 | PascalCase | `UserProfile`, `Point` |
| 枚举 | PascalCase | `Option`, `Result` |
| 文件 | snake_case | `my_module.dal` |

### 4.2 错误处理策略

```dal
// 优先使用 try/catch 处理可能的错误
try {
    let data = fetch_data()
    process(data)
} catch (err) {
    log_error(err)
    fallback()
}

// 使用 Option/Result 模式
fn safe_divide(a, b) @ pure @ cpu {
    if b == 0 {
        return None
    }
    return Some(a / b)
}

match safe_divide(10, 2) {
    Some(v) => println("Result: " + str(v)),
    None => println("Division by zero")
}
```

### 4.3 测试策略

```dal
// 使用 testing 模块
import "testing"

fn test_add() @ pure @ cpu {
    assert(add(2, 3) == 5)
    assert(add(-1, 1) == 0)
    assert(add(0, 0) == 0)
}

fn test_factorial() @ pure @ cpu {
    assert(factorial(0) == 1)
    assert(factorial(5) == 120)
}
```

### 4.4 Profiler 使用建议

1. **先跑基准测试**：`cargo bench` 建立基线
2. **定位热点**：`dalib profile` 找出耗时函数
3. **优化后验证**：再次运行 `cargo bench` 确认改进
4. **设置回归门**：benchmark 结果比基线差 10% 以上视为回归

---

## 5. 与主流语言对比

### 5.1 与 Rust 的异同

| 特性 | Dalin L | Rust |
|------|---------|------|
| 所有权 | 无（GC/引用计数） | 编译时所有权 |
| 生命周期 | 自动 | 显式标注 |
| 类型推断 | HM 类型推断 | HM + 局部推断 |
| 中文标识符 | ✅ 全链路支持 | ❌ 不支持 |
| 七通道系统 | ✅ 内置 | ❌ 无 |
| 协程 | M:N 协程 | async/await |
| 编译模式 | 解释器 + 字节码 | 编译到 native |
| 学习曲线 | 低 | 高 |

### 5.2 与 Python 的异同

| 特性 | Dalin L | Python |
|------|---------|--------|
| 类型系统 | 静态 + 七通道 | 动态 |
| 性能 | 编译型，更快 | 解释型，较慢 |
| 并发 | 原生协程 | GIL 限制 |
| 中文标识符 | ✅ 原生支持 | ✅ 支持 |
| 包管理 | Cryo 包管理器 | pip |
| IDE 支持 | LSP + VS Code 扩展 | 广泛 |

### 5.3 与 Go 的异同

| 特性 | Dalin L | Go |
|------|---------|-----|
| 并发模型 | M:N 协程 + 通道 | goroutine + channel |
| 类型系统 | HM 推断 + 七通道 | 静态 + 接口 |
| 泛型 | Trait 约束 | 无泛型 |
| 编译速度 | 快（增量缓存） | 非常快 |
| 内存安全 | 运行时检查 | 编译时检查 |
| 模式匹配 | ✅ 支持 | ❌ 不支持 |

---

## 6. 故障排除

### 6.1 常见错误

| 错误信息 | 可能原因 | 解决方案 |
|----------|----------|----------|
| `Unexpected token` | 语法错误 | 检查括号匹配、操作符位置 |
| `Undefined variable` | 变量未声明 | 检查变量名拼写、作用域 |
| `Function not found` | 函数未定义 | 检查函数名、导入路径 |
| `Type mismatch` | 类型不兼容 | 检查操作数类型 |
| `Division by zero` | 除零 | 添加除数检查 |

### 6.2 调试技巧

1. **使用 `print` 调试**：
   ```dal
   fn debug_add(a, b) @ pure @ cpu {
       print("a = " + str(a))
       print("b = " + str(b))
       let result = a + b
       print("result = " + str(result))
       return result
   }
   ```

2. **使用 `assert` 验证**：
   ```dal
   assert(x > 0, "x must be positive")
   ```

3. **使用 Profiler 定位性能瓶颈**：
   ```bash
   dalib profile --input my_program.dal
   ```

### 6.3 获取帮助

- 查看 `docs/` 目录下的完整文档
- 运行 `dalib help` 查看 CLI 使用帮助
- 运行 `dalib lang` 查看语言信息