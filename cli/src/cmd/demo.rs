use dalin_compiler::{lexer, parser, ty};
use dalin_runtime::interpreter;

pub fn run() -> Result<(), String> {
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

    println!("============================================================");
    println!("  Dalin L — Rust Port v0.1.0");
    println!("============================================================");

    // Step 1: Lexer
    println!("\n--- 1. Lexer ---");
    let mut lex = lexer::Lexer::new(demo);
    match lex.tokenize() {
        Ok(tokens) => {
            for tok in &tokens {
                if tok.token_type != dalin_compiler::token::TokenType::Eof {
                    println!("  {}", tok);
                }
            }
            println!("  ✅ Lexer OK ({} tokens)", tokens.len());

            // Step 2: Parser
            println!("\n--- 2. Parser ---");
            let mut p = parser::Parser::new(tokens);
            match p.parse() {
                Ok(prog) => {
                    println!("  ✅ Parser OK ({} statements)", prog.statements.len());

                    // Step 3: Type Inference
                    println!("\n--- 3. Type Inference ---");
                    let mut infer = ty::TypeInferencer::new();
                    infer.infer_program(&prog);
                    let report = infer.print_report();
                    if !report.trim().is_empty() {
                        print!("{}", report);
                    }

                    // Step 4: Interpreter
                    println!("\n--- 4. Interpreter ---");
                    match interpreter::run_source(demo) {
                        Ok(_) => println!("  ✅ Execution OK"),
                        Err(e) => println!("  ❌ Runtime error: {}", e),
                    }
                }
                Err(e) => println!("  ❌ Parser error: {}", e),
            }
        }
        Err(e) => println!("  ❌ Lexer error: {}", e),
    }

    Ok(())
}
