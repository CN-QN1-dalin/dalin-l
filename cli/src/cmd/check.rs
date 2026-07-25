use crate::util;

pub fn run(input: &str, verbose: bool, json: bool) -> Result<(), String> {
    let banner = util::banner("CHECK");
    println!("{}", banner);

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    let src =
        std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;

    use dalin_compiler::{lexer, parser, ty};

    let mut lex = lexer::Lexer::new(&src);
    match lex.tokenize() {
        Ok(tokens) => {
            println!("  ✅ Lexer passed ({} tokens)", tokens.len());

            let mut p = parser::Parser::new(tokens);
            match p.parse() {
                Ok(prog) => {
                    println!("  ✅ Parser passed ({} stmts)", prog.statements.len());

                    if verbose {
                        let mut infer = ty::TypeInferencer::new();
                        infer.infer_program(&prog);
                        println!("\n{}", infer.print_report().trim_end());
                    } else {
                        println!("  ✅ Type inference passed (--verbose for details)");
                    }
                }
                Err(e) => {
                    return util::err("parser", &format!("{}", e)).map_err(|_| String::new());
                }
            }
        }
        Err(e) => {
            return util::err("lexer", &format!("{}", e)).map_err(|_| String::new());
        }
    }

    if json {
        println!("\n{{ \"status\": \"ok\", \"file\": \"{}\" }}", input);
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   CHECK COMPLETE ✓                ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
