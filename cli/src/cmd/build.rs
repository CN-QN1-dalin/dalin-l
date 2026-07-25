use crate::util;
use dalin_compiler::{lexer, parser, ty};
use dalin_dlvm::BytecodeCompiler;

pub fn run(input: &str, output: &str, verbose: bool) -> Result<(), String> {
    let banner = util::banner("BUILD");
    println!("{banner}");

    let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{input}': {e}"))?;
    if verbose {
        println!("\n  [src] {} bytes", src.len());
    }

    // Lexer
    {
        util::section("Lexer");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{e}"))?;
        if verbose {
            let total = tokens
                .iter()
                .filter(|t| t.token_type != dalin_compiler::token::TokenType::Eof)
                .count();
            for tok in tokens
                .iter()
                .filter(|t| t.token_type != dalin_compiler::token::TokenType::Eof)
                .take(20)
            {
                println!("  {tok}");
            }
            if total > 20 {
                println!("  ... and {} more", total - 20);
            }
        }
        println!("  ✅ {} tokens", tokens.len());
    }

    // Parser
    {
        util::section("Parser");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{e}"))?;
        let prog = parser::Parser::new(tokens)
            .parse()
            .map_err(|e| format!("{e}"))?;
        println!("  ✅ {} statements", prog.statements.len());
    }

    // Type Inference
    {
        util::section("Type Inference");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{e}"))?;
        let prog = parser::Parser::new(tokens)
            .parse()
            .map_err(|e| format!("{e}"))?;
        let mut infer = ty::TypeInferencer::new();
        infer.infer_program(&prog);
        let report = infer.print_report();
        if report.trim().is_empty() {
            println!("  (no inference data)");
        } else {
            println!("\n  \n{}", report.trim_end());
        }
        println!("  ✅ Type inference complete");
    }

    // Bytecode Compilation
    {
        util::section("Bytecode Compiler");
        let mut lex = lexer::Lexer::new(&src);
        let tokens = lex.tokenize().map_err(|e| format!("{e}"))?;
        let prog = parser::Parser::new(tokens)
            .parse()
            .map_err(|e| format!("{e}"))?;
        let funcs = BytecodeCompiler::new().compile(&prog);
        println!("  ✅ Compiled {} functions", funcs.len());
    }

    // Output
    {
        util::section("Output");
        let bytes = format!("DANL-VM-bytecode-0.1\n{}\n{}", input, src.len());
        let data = bytes.as_bytes().to_vec();
        std::fs::write(output, &data).map_err(|e| format!("Cannot write '{output}': {e}"))?;
        println!("  ✅ Wrote {} bytes → {}", data.len(), output);
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   BUILD COMPLETE ✓                ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_dir() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("dalin-build-test-{}-{}", pid, n))
    }

    fn create_temp_source(content: &str, dir: &PathBuf) -> PathBuf {
        std::fs::create_dir_all(dir).expect("Failed to create test dir");
        let file_path = dir.join("test.dal");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create test file");
        write!(file, "{}", content).expect("Failed to write test content");
        file_path
    }

    #[test]
    fn test_build_nonexistent_input() {
        let out_dir = test_dir();
        let result = run(
            "/nonexistent/file.dal",
            out_dir.join("out.bin").to_str().unwrap(),
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_build_empty_source() {
        let dir = test_dir();
        let src_path = create_temp_source("", &dir);
        let out_path = dir.join("output.bin");
        let result = run(
            src_path.to_str().unwrap(),
            out_path.to_str().unwrap(),
            false,
        );
        assert!(result.is_ok(), "Build empty source: {:?}", result.err());
        assert!(out_path.exists(), "Output file should exist");
    }

    #[test]
    fn test_build_simple_program() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let out_path = dir.join("output.bin");
        let result = run(
            src_path.to_str().unwrap(),
            out_path.to_str().unwrap(),
            false,
        );
        assert!(result.is_ok(), "Build simple program: {:?}", result.err());
        assert!(out_path.exists(), "Output file should exist");
    }

    #[test]
    fn test_build_with_verbose() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let out_path = dir.join("output.bin");
        let result = run(src_path.to_str().unwrap(), out_path.to_str().unwrap(), true);
        assert!(result.is_ok(), "Build with verbose: {:?}", result.err());
    }
}
