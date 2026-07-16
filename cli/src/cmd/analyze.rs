use crate::util;
use dalin_compiler::{lexer, parser, ty};

pub fn run(input: &str, verbose: bool, _json: bool) -> Result<(), String> {
    let banner = util::banner("ANALYZE");
    println!("{}", banner);

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;

    let mut lex = lexer::Lexer::new(&src);
    let tokens = lex.tokenize().map_err(|e| format!("Lexer: {}", e))?;
    let token_count = tokens.len();

    let prog = parser::Parser::new(tokens).parse().map_err(|e| format!("Parse: {}", e))?;

    let mut infer = ty::TypeInferencer::new();
    infer.infer_program(&prog);
    let report = infer.print_report();

    let func_count = prog.statements.iter().filter(|s| matches!(s, dalin_compiler::ast::Stmt::Fn { .. })).count();

    println!("\n  ┌─────────────────────────────────┐");
    println!("  │  Source Analysis Report         │");
    println!("  ├─────────────────────────────────┤");
    println!("  │  File:           {}          │", input);
    println!("  │  Lines:          {:<11}│", src.lines().count());
    println!("  │  Tokens:         {:<11}│", token_count);
    println!("  │  Statements:     {:<11}│", prog.statements.len());
    println!("  │  Functions:      {:<11}│", func_count);
    println!("  └─────────────────────────────────┘");

    if verbose && !report.trim().is_empty() { println!("\n  Type Inference:\n{}", report.trim_end()); }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   ANALYSIS COMPLETE ✓             ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
