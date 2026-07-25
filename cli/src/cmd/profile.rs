use dalin_runtime::profiler::Profiler;
use std::path::Path;

/// dalib profile <file.dal>
pub fn run_profile(input: &str, verbose: bool) -> Result<(), String> {
    if !Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    println!("=== Dalin L 3.0 Profiler ===");
    println!("Profile target: {}\n", input);

    let mut profiler = Profiler::new();

    // Profile: tokenize
    let src =
        std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;

    profiler.start_call("tokenize");
    let mut lex = dalin_compiler::lexer::Lexer::new(&src);
    let tokens = lex.tokenize().map_err(|e| format!("Lexer error: {}", e))?;
    profiler.end_call("tokenize");

    if verbose {
        println!("  ✓ Tokenized {} tokens", tokens.len());
    }

    // Profile: parse
    profiler.start_call("parse");
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let prog = parser.parse().map_err(|e| format!("Parse error: {}", e))?;
    profiler.end_call("parse");

    if verbose {
        println!("  ✓ Parsed {} statements", prog.statements.len());
    }

    // Profile: type check
    profiler.start_call("type_check");
    {
        let mut inf = dalin_compiler::ty2::SevenChannelInferencer::new();
        inf.infer_program(&prog);
        let has_errors = inf.effect.errors.len()
            + inf.cognitive_loop.errors.len()
            + inf.governance.errors.len()
            + inf.time_constraint.errors.len();
        if has_errors > 0 && verbose {
            println!("  ⚠ Type checker produced warnings/errors");
        }
    }
    profiler.end_call("type_check");

    if verbose {
        println!("  ✓ Type checking completed");
    }

    // Profile: execute
    profiler.start_call("run");
    match dalin_runtime::interpreter::run_source(&src) {
        Ok(results) => {
            if verbose {
                println!("  ✓ Executed {} expressions", results.len());
            }
        }
        Err(e) => {
            profiler.end_call("run");
            return Err(format!("Runtime error: {}", e));
        }
    }
    profiler.end_call("run");

    // Print report
    println!();
    println!("{}", profiler.report());

    Ok(())
}
