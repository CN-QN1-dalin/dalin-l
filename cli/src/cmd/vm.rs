/// Legacy run_demo_with_vm() — DLVM bytecode VM demo
use dalin_compiler::{lexer, parser};
use dalin_dlvm::BytecodeCompiler;
use dalin_runtime::interpreter;

pub fn run(mode: &str) -> Result<(), String> {
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
    println!("  Dalin L — DLVM (mode={})", mode);
    println!("============================================================");

    let mut lex = lexer::Lexer::new(demo);
    match lex.tokenize() {
        Ok(tokens) => {
            let mut parser = parser::Parser::new(tokens);
            let prog = parser.parse();
            // Report recovered parse errors
            for err in parser.recovered() {
                println!("  ⚠ Parser warning: {}", err);
            }
            if mode == "bytecode" {
                let mut compiler = BytecodeCompiler::new();
                let funcs = compiler.compile(&prog);
                let mut vm = dalin_dlvm::Vm::new(funcs);
                match vm.run() {
                    Ok(val) => println!("\n  ✅ DLVM OK: {}", val),
                    Err(e) => println!("\n  ❌ DLVM error: {}", e),
                }
            } else {
                let mut interp = interpreter::Interpreter::new();
                match interp.interpret(&prog) {
                    Ok(_) => println!("\n  ✅ Interpreter OK"),
                    Err(e) => println!("\n  ❌ Runtime error: {}", e),
                }
            }
        }
        Err(e) => println!("  ❌ Lexer error: {}", e),
    }

    Ok(())
}
