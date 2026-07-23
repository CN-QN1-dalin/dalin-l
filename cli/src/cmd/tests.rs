use dalin_compiler::{lexer, parser, ty, token};
use dalin_runtime::interpreter;

pub fn run() -> Result<(), String> {
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
                        println!("  [lexer] {}: ok", $name);
                    } else {
                        failed += 1;
                        println!("  [lexer] {}: FAILED", $name);
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  [lexer] {}: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_parse {
        ($name:expr, $src:expr) => {
            let mut lex = lexer::Lexer::new($src);
            match lex.tokenize() {
                Ok(tokens) => {
                    let mut p = parser::Parser::new(tokens);
                    match p.parse() {
                        Ok(prog) => {
                            if !prog.is_empty() {
                                passed += 1;
                                println!("  [parser] {}: ok", $name);
                            } else {
                                failed += 1;
                                println!("  [parser] {}: empty program", $name);
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            println!("  [parser] {}: {}", $name, e);
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  [parser] {}: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_infer {
        ($name:expr, $src:expr, $expected:expr) => {
            let mut lex = lexer::Lexer::new($src);
            match lex.tokenize() {
                Ok(tokens) => {
                    let mut p = parser::Parser::new(tokens);
                    match p.parse() {
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
                                println!("  [infer] {}: ok", $name);
                            } else {
                                failed += 1;
                                println!("  [infer] {}: FAILED types={:?}", $name, types);
                            }
                        }
                        Err(e) => {
                            failed += 1;
                            println!("  [infer] {}: parser error: {}", $name, e);
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("  [infer] {}: lexer error: {}", $name, e);
                }
            }
        };
    }

    macro_rules! test_run {
        ($name:expr, $src:expr) => {
            match interpreter::run_source($src) {
                Ok(_) => {
                    passed += 1;
                    println!("  [run] {}: ok", $name);
                }
                Err(e) => {
                    failed += 1;
                    println!("  [run] {}: {}", $name, e);
                }
            }
        };
    }

    // Lexer Tests
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
    test_lexer!("布尔字面量", "true false", |toks: &[token::Token]| {
        toks.iter().filter(|t| t.token_type == token::TokenType::BoolLiteral).count() == 2
    });

    // Parser Tests
    println!("\n--- Parser Tests ---");
    test_parse!("let 语句", "let x = 42");
    test_parse!("函数定义", "fn add(a, b) { return a + b }");
    test_parse!("if-else", "if x > 0 { println(x) } else { println(-x) }");
    test_parse!("for 循环", "for i in 0..10 { println(i) }");
    test_parse!("管道操作", "data |> filter |> map");
    test_parse!("数组字面量", "let arr = [1, 2, 3]");
    test_parse!("中文函数", "fn 计算(a) { return a + 1 }");

    // Type Inference Tests
    println!("\n--- Type Inference Tests ---");
    test_infer!("int 字面量", "let x = 42", [("x", "int")]);
    test_infer!("string 字面量", "let x = \"hello\"", [("x", "string")]);
    test_infer!("bool 字面量", "let x = true", [("x", "bool")]);
    test_infer!("数组类型", "let arr = [1, 2, 3]", [("arr", "array<int>")]);
    test_infer!("Option<int>", "let opt = Some(42)", [("opt", "option<int>")]);

    // Runtime Tests
    println!("\n--- Runtime Tests ---");
    test_run!("Hello World", "println(\"Hello, Dalin!\")");
    test_run!("算术运算", "println(1 + 2)");
    test_run!("函数调用", "fn add(a, b) { return a + b }\nprintln(add(3, 4))");
    test_run!("递归阶乘", "fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\nprintln(fact(5))");
    test_run!("for 循环", "let nums = [1, 2, 3]\nlet mut sum = 0\nfor i in nums { sum = sum + i }\nprintln(sum)");
    test_run!("管道操作", "fn inc(x) { return x + 1 }\nlet r = 1 |> inc |> inc\nprintln(r)");
    test_run!("中文变量", "let 名字 = \"大林\"\nprintln(名字)");

    // Summary
    println!("\n============================================================");
    println!("  Tests: {} / {} passed | {} failed", passed, passed + failed, failed);
    println!("============================================================");

    if failed > 0 {
        Err(format!("{} tests failed", failed))
    } else {
        Ok(())
    }
}
