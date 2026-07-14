# Dalin L — Agent-Native Programming Language

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-1.95+-orange.svg)

**Dalin L** 是一门面向 AI Agent 的编程语言，支持中文标识符、HM 类型推断、模式匹配与管道操作。

> Python 原型 → Rust 移植，从设计验证到可运行实现。

---

## 特性

- **中文标识符全链路支持** — 变量名、函数名、参数名全线中文化
- **HM 类型推断** — Robinson Unification + 多态函数调用
- **模式匹配** — wild / ident / lit / ctor / struct 五种模式 + 守卫 + 嵌套
- **管道操作** — `data |> filter |> map` 链式语法
- **if/match 表达式** — `let r = if true { 42 }`
- **Option / Result** — `Some(v) / None / Ok(v) / Err(e)`
- **递归 + 闭包** — 函数是第一公民
- **零依赖** — 纯 Rust 标准库实现

## 快速开始

```bash
git clone https://github.com/CN-QN1-dalin/dalin-l.git
cd dalin-l
cargo run          # 运行演示
cargo run --repl   # 启动交互式 REPL
cargo run --test   # 运行测试
```

## 示例

```rust
let 名字 = "大林"
fn greet(n) {
    return "你好, " + n + "!"
}
println(greet(名字))       // → 你好, 大林!

// 递归阶乘
fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
println(fact(5))            // → 120

// 模式匹配
let opt = Some(100)
match opt {
    Some(v) => println("got", v),
    None => println("empty"),
}                            // → got 100

// 管道操作
fn double(x) { return x * 2 }
let r = 1 |> double |> double
println(r)                   // → 4

// 范围 + for 循环
let mut sum = 0
for i in 0..5 { sum = sum + i }
println(sum)                 // → 10
```

## 架构

```
Source ─→ Lexer ─→ Tokens ─→ Parser ─→ AST ─→ TypeInferencer ─→ Type Report
                                          ↘ Interpreter ─→ Runtime Output
```

| 模块 | 职责 |
|------|------|
| `token.rs` | 65+ Token 类型定义 |
| `ast.rs` | 30+ AST 节点 (表达式/语句/模式) |
| `lexer.rs` | 词法分析器 (中文标识符 / 转义 / 注释) |
| `parser.rs` | 递归下降语法分析器 (错误恢复 / 运算符优先级) |
| `ty.rs` | HM 类型推断引擎 (Robinson Unification) |
| `env.rs` | 运行时作用域环境 |
| `interpreter.rs` | 树遍历解释器 |

## 测试

```bash
cargo run --test
# 42/42 passed, 0 failed
```

## 许可证

[MIT](LICENSE)

## 作者

**贾大林** ([@CN-QN1-dalin](https://github.com/CN-QN1-dalin))