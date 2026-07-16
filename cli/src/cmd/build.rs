use dalin_compiler::{lexer, parser, ty};
use dalin_dlvm::BytecodeCompiler;
use crate::util;

pub fn run(input: &str, output: &str, verbose: bool) -> Result<(), String> {
    let banner = util::banner("BUILD");
    println!("{}", banner);

    let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;
    if verbose { println!("\n  [src] {} bytes", src.len()); }

    // Lexer
    {
        util::section("Lexer");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{}", e))?;
        if verbose {
            let total = tokens.iter().filter(|t| t.token_type != dalin_compiler::token::TokenType::Eof).count();
            for tok in tokens.iter().filter(|t| t.token_type != dalin_compiler::token::TokenType::Eof).take(20) {
                println!("  {}", tok);
            }
            if total > 20 { println!("  ... and {} more", total - 20); }
        }
        println!("  ✅ {} tokens", tokens.len());
    }

    // Parser
    {
        util::section("Parser");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{}", e))?;
        let prog = parser::Parser::new(tokens).parse().map_err(|e| format!("{}", e))?;
        println!("  ✅ {} statements", prog.statements.len());
    }

    // Type Inference
    {
        util::section("Type Inference");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{}", e))?;
        let prog = parser::Parser::new(tokens).parse().map_err(|e| format!("{}", e))?;
        let mut infer = ty::TypeInferencer::new();
        infer.infer_program(&prog);
        let report = infer.print_report();
        if !report.trim().is_empty() { println!("\n{}\n{}", "  ", report.trim_end()); }
        else { println!("  (no inference data)"); }
        println!("  ✅ Type inference complete");
    }

    // Bytecode Compilation
    {
        util::section("Bytecode Compiler");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{}", e))?;
        let prog = parser::Parser::new(tokens).parse().map_err(|e| format!("{}", e))?;
        let funcs = BytecodeCompiler::new().compile(&prog);
        println!("  ✅ Compiled {} functions", funcs.len());
    }

    // Output
    {
        util::section("Output");
        let bytes = format!("DANL-VM-bytecode-0.1\n{}\n{}", input, src.len());
        let data = bytes.as_bytes().to_vec();
        std::fs::write(output, &data).map_err(|e| format!("Cannot write '{}': {}", output, e))?;
        println!("  ✅ Wrote {} bytes → {}", data.len(), output);
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   BUILD COMPLETE ✓                ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
