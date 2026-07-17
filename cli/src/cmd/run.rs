use std::thread;
use std::time::Duration;
use crate::util;

pub fn run(input: &str, watch: bool, verbose: bool) -> Result<(), String> {
    let banner = util::banner("RUN");
    println!("{}", banner);

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    let mut compiled_ok = false;

    loop {
        if compiled_ok && watch {
            println!("\n  [watch] Waiting for changes...");
            thread::sleep(Duration::from_secs(1));
        } else if watch {
            compiled_ok = true;
        }

        use dalin_compiler::{lexer, parser};

        let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;
        
        let mut lex = lexer::Lexer::new(&src);
        match lex.tokenize() {
            Ok(tokens) => {
                let mut p = parser::Parser::new(tokens);
                match p.parse() {
                    Ok(prog) => {
                        let _ = util::ok("compile", &format!("{} statements", prog.statements.len()));
                        
                        use dalin_runtime::interpreter;
                        match interpreter::run_source(&src) {
                            Ok(_) => { if verbose { println!("\n  Runtime execution completed."); } }
                            Err(e) => {
                                println!("\n  ❌ Runtime error: {}", e);
                                if !watch { return Err(format!("{}", e)); }
                            }
                        }
                    }
                    Err(e) => {
                        if !watch { return Err(format!("{}", e)); }
                    }
                }
            }
            Err(e) => {
                if !watch { return Err(format!("{}", e)); }
            }
        }

        if !watch { break; }
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   RUN COMPLETE ✓                  ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
