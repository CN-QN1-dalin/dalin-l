use std::io::{self, Write};
use crate::util;
use dalin_compiler::{lexer, parser, ty};
use dalin_runtime::interpreter;

pub fn run() -> Result<(), String> {
    println!("============================================================");
    println!("  Dalin L 2.0 — REPL Interactive Mode");
    println!("============================================================");
    println!("\nType 'exit' to quit, 'help' for help\n");

    loop {
        print!("dal> ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() || line.trim().is_empty() {
            continue;
        }

        let line = line.trim().to_string();
        match line.as_str() {
            "exit" | "quit" => { println!("再见！"); break; }
            "help" => {
                println!("Dalin L 2.0 — Agent-Native Programming Language");
                println!("  Syntax: let fn return if else match for in while");
            }
            _ => {
                let mut lex = lexer::Lexer::new(&line);
                match lex.tokenize() {
                    Ok(tokens) => {
                        let mut p = parser::Parser::new(tokens);
                        match p.parse() {
                            Ok(prog) => {
                                let mut infer = ty::TypeInferencer::new();
                                infer.infer_program(&prog);
                                let report = infer.print_report();
                                if !report.trim().is_empty() { println!("  {}", report.trim_end()); }
                                
                                if let Err(e) = interpreter::run_source(&line) {
                                    println!("  ❌ Runtime: {}", e);
                                }
                            }
                            Err(e) => println!("  ❌ Parse: {}", e),
                        }
                    }
                    Err(e) => println!("  ❌ Lex: {}", e),
                }
            }
        }
    }
    Ok(())
}
