/// Dalin L — Rust 移植入口

mod token;
mod ast;
mod lexer;
mod parser;
mod ty;
mod env;
mod interpreter;

use std::io::{self, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--repl" {
        run_repl();
    } else if args.len() > 1 && args[1] == "--test" {
        run_tests();
    } else {
        run_demo();
    }
}

fn run_demo() {
    let demo = r#"
    let x = 42
    let name = "大林"
    fn greet(n) {
        return "你好, " + n + "!"
    }
    println(greet(name))
    let nums = [1, 2, 3, 4, 5]
    let mut sum = 0
    for i in nums {
        sum = sum + i
    }
    println("sum =", sum)
    fn fact(n) {
        if n <= 1 { return 1 }
        return n * fact(n - 1)
    }
    println("5! =", fact(5))
    let opt = Some(100)
    match opt {
        Some(v) => println("got", v),
        None => println("empty"),
    }
    "#;

    println!("{}", "=".repeat(60));
    println!("  Dalin L — Rust Port v0.1.0");
    println!("{}", "=".repeat(60));

    // Step 1: Lexer
    println!("\n--- 1. Lexer ---");
    let mut lex = lexer::Lexer::new(demo);
    match lex.tokenize() {
        Ok(tokens) => {
            for tok in &tokens {
                if tok.token_type != token::TokenType::Eof {
                    println!("  {}", tok);
                }
            }
            println!("  ✅ Lexer OK ({} tokens)", tokens.len());

            // Step 2: Parser
            println!("\n--- 2. Parser ---");
            let mut parser = parser::Parser::new(tokens);
            match parser.parse() {
                Ok(prog) => {
                    println!("  ✅ Parser OK ({} statements)", prog.statements.len());
                    println!("\n  AST:\n{}", parser::ast_to_string(&prog));

                    // Step 3: Type Inference
                    println!("\n--- 3. Type Inference ---");
                    let mut infer = ty::TypeInferencer::new();
                    let _ = infer.infer_program(&prog);
                    print!("{}", infer.print_report());

                    // Step 4: Interpreter
                    println!("\n--- 4. Interpreter ---");
                    let mut interp = interpreter::Interpreter::new();
                    match interp.interpret(&prog) {
                        Ok(results) => {
                            println!("  ✅ Execution OK");
                            println!("\nResults: {:?}", results);
                        }
                        Err(e) => println!("  ❌ Runtime error: {}", e),
                    }
                }
                Err(e) => println!("  ❌ Parser error: {}", e),
            }
        }
        Err(e) => println!("  ❌ Lexer error: {}", e),
    }
}

fn run_repl() {
    println!();
    println!("{}", "=".repeat(60));
    println!("  Dalin L — Rust Port v0.1.0");
    println!("  Interactive REPL (minimal)");
    println!("{}", "=".repeat(60));
    println!();
    println!("Type 'exit' to quit, 'help' for help");

    loop {
        print!("dalín> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() || line.trim().is_empty() {
            continue;
        }

        let line = line.trim().to_string();
        match line.as_str() {
            "exit" | "quit" => {
                println!("👋 再见！");
                break;
            }
            "help" => {
                println!("Dalin L — Agent-Native Programming Language");
                println!("  let fn return if else match for in while");
                println!("  -> => |> :: ..");
                println!("  中文标识符支持");
            }
            _ => {
                let mut lex = lexer::Lexer::new(&line);
                match lex.tokenize() {
                    Ok(tokens) => {
                        let mut parser = parser::Parser::new(tokens);
                        match parser.parse() {
                            Ok(prog) => {
                                let mut infer = ty::TypeInferencer::new();
                                let _ = infer.infer_program(&prog);
                                println!("{}", infer.print_report());

                                let mut interp = interpreter::Interpreter::new();
                                match interp.interpret(&prog) {
                                    Ok(_) => {}
                                    Err(e) => println!("  ❌ {}", e),
                                }
                            }
                            Err(e) => println!("  ❌ {}", e),
                        }
                    }
                    Err(e) => println!("  ❌ {}", e),
                }
            }
        }
    }
}

fn run_tests() {
    println!("Running Dalin L tests...");
    let mut passed = 0u32;
    let mut failed = 0u32;

    macro_rules! test_lexer {
        ($name:expr, $src:expr, $check:expr) => {
            let mut lex = lexer::Lexer::new($src);
            match lex.tokenize() {
                Ok(tokens) => {
                    if $check(&tokens) {
                        passed += 1;
                        println!("  ✅ [lexer] {}", $name);
                    } else {
                        failed += 1;
                        println!("  ❌ [lexer] {}", $name);
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  ❌ [lexer] {}: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_parse {
        ($name:expr, $src:expr) => {
            let mut lex = lexer::Lexer::new($src);
            match lex.tokenize() {
                Ok(tokens) => {
                    let mut parser = parser::Parser::new(tokens);
                    match parser.parse() {
                        Ok(prog) => {
                            if !prog.is_empty() {
                                passed += 1;
                                println!("  ✅ [parser] {}", $name);
                            } else {
                                failed += 1;
                                println!("  ❌ [parser] {}: empty program", $name);
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            println!("  ❌ [parser] {}: {}", $name, e);
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  ❌ [parser] {}: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_infer {
        ($name:expr, $src:expr, $expected:expr) => {
            let mut lex = lexer::Lexer::new($src);
            match lex.tokenize() {
                Ok(tokens) => {
                    let mut parser = parser::Parser::new(tokens);
                    match parser.parse() {
                        Ok(prog) => {
                            let mut infer = ty::TypeInferencer::new();
                            let types = infer.infer_program(&prog);
                            let mut ok = true;
                            for (k, v) in $expected {
                                if let Some(actual) = types.get(k) {
                                    if format!("{}", actual) != v {
                                        ok = false;
                                    }
                                } else {
                                    ok = false;
                                }
                            }
                            if ok {
                                passed += 1;
                                println!("  ✅ [infer] {}", $name);
                            } else {
                                failed += 1;
                                println!("  ❌ [infer] {}: types={:?}", $name, types);
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            println!("  ❌ [infer] {}: parser error: {}", $name, e);
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  ❌ [infer] {}: lexer error: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_run {
        ($name:expr, $src:expr) => {
            match interpreter::run_source($src) {
                Ok(_) => {
                    passed += 1;
                    println!("  ✅ [run] {}", $name);
                }
                Err(e) => {
                    failed += 1;
                    println!("  ❌ [run] {}: {}", $name, e);
                }
            }
        };
    }

    // ── Lexer Tests ──
    println!("\n--- Lexer Tests ---");
    test_lexer!("let x = 42", "let x = 42", |toks: &[token::Token]| {
        toks.len() >= 4 && toks[0].token_type == token::TokenType::KeywordLet
    });
    test_lexer!("中文标识符", "let 名字 = 42", |toks: &[token::Token]| {
        toks.len() >= 4 && toks[1].value == "名字"
    });
    test_lexer!("管道操作符", "x |> f", |toks: &[token::Token]| {
        toks.iter().any(|t| t.token_type == token::TokenType::Pipe)
    });
    test_lexer!("范围表达式", "0..10", |toks: &[token::Token]| {
        toks.iter().any(|t| t.token_type == token::TokenType::DoubleDot)
    });
    test_lexer!("属性宏", "#[derive(Clone)]", |toks: &[token::Token]| {
        toks[0].token_type == token::TokenType::Attribute
    });
    test_lexer!("布尔字面量", "true false", |toks: &[token::Token]| {
        toks.iter().filter(|t| t.token_type == token::TokenType::BoolLiteral).count() == 2
    });
    test_lexer!("空源码", "", |toks: &[token::Token]| toks.len() == 1 && toks[0].token_type == token::TokenType::Eof);

    // ── Parser Tests ──
    println!("\n--- Parser Tests ---");
    test_parse!("let 语句", "let x = 42");
    test_parse!("函数定义", "fn add(a, b) { return a + b }");
    test_parse!("if-else", "if x > 0 { println(x) } else { println(-x) }");
    test_parse!("for 循环", "for i in 0..10 { println(i) }");
    test_parse!("while 循环", "while true { println(1) }");
    test_parse!("match 表达式", "match x { Some(v) => v, None => 0 }");
    test_parse!("struct 定义", "struct Point { x: int, y: int }");
    test_parse!("enum 定义", "enum Color { Red, Green, Blue }");
    test_parse!("管道操作", "data |> filter |> map");
    test_parse!("数组字面量", "let arr = [1, 2, 3]");
    test_parse!("Some/None", "let opt = Some(42)");
    test_parse!("if 表达式", "let r = if true { 42 }");
    test_parse!("match 表达式 let", "let r = match 1 { 1 => 1, _ => 0 }");
    test_parse!("中文函数", "fn 计算(a) { return a + 1 }");
    test_parse!("嵌套 match", "match x { Some(Some(v)) => v, _ => 0 }");
    test_parse!("闭包函数", "fn foo() { fn bar() { return 1 } }");

    // ── Type Inference Tests ──
    println!("\n--- Type Inference Tests ---");
    test_infer!("int 字面量", "let x = 42", vec![("x", "int")]);
    test_infer!("string 字面量", "let x = \"hello\"", vec![("x", "string")]);
    test_infer!("bool 字面量", "let x = true", vec![("x", "bool")]);
    test_infer!("float 字面量", "let x = 3.14", vec![("x", "float")]);
    test_infer!("数组类型", "let arr = [1, 2, 3]", vec![("arr", "array<int>")]);
    test_infer!("Option<int>", "let opt = Some(42)", vec![("opt", "option<int>")]);
    test_infer!("中文变量", "let 名字 = \"大林\"", vec![("名字", "string")]);
    test_infer!("类型注解", "let x: int = 42", vec![("x", "int")]);

    // ── Runtime Tests ──
    println!("\n--- Runtime Tests ---");
    test_run!("Hello World", "println(\"Hello, Dalin!\")");
    test_run!("算术运算", "println(1 + 2)");
    test_run!("函数调用", "fn add(a, b) { return a + b }\nprintln(add(3, 4))");
    test_run!("字符串拼接", "let s = \"Hello, \" + \"World\"\nprintln(s)");
    test_run!("递归阶乘", "fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\nprintln(fact(5))");
    test_run!("for 循环", "let nums = [1, 2, 3]\nlet mut sum = 0\nfor i in nums { sum = sum + i }\nprintln(sum)");
    test_run!("match Some", "let opt = Some(42)\nmatch opt { Some(v) => println(v), None => println(\"empty\") }");
    test_run!("match None", "let opt = None\nmatch opt { Some(v) => println(v), None => println(\"empty\") }");
    test_run!("数组索引", "let arr = [10, 20, 30]\nprintln(arr[1])");
    test_run!("管道操作", "fn inc(x) { return x + 1 }\nlet r = 1 |> inc |> inc\nprintln(r)");
    test_run!("中文变量", "let 名字 = \"大林\"\nprintln(名字)");

    // ── Summary ──
    println!("\n{}", "=".repeat(60));
    println!("  Tests: ✅{} / {} passed | ❌{} failed", passed, passed + failed, failed);
    println!("{}", "=".repeat(60));
}