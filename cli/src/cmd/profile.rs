use dalin_runtime::profiler::Profiler;
use std::path::Path;

/// dalib profile <file.dal>
pub fn run_profile(input: &str, verbose: bool) -> Result<(), String> {
    if !Path::new(input).exists() {
        return Err(format!("Source file '{input}' does not exist"));
    }

    println!("=== Dalin L 3.0 Profiler ===");
    println!("Profile target: {input}\n");

    let mut profiler = Profiler::new();

    // Profile: tokenize
    let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{input}': {e}"))?;

    profiler.start_call("tokenize");
    let mut lex = dalin_compiler::lexer::Lexer::new(&src);
    let tokens = lex.tokenize().map_err(|e| format!("Lexer error: {e}"))?;
    profiler.end_call("tokenize");

    if verbose {
        println!("  ✓ Tokenized {} tokens", tokens.len());
    }

    // Profile: parse
    profiler.start_call("parse");
    let mut parser = dalin_compiler::parser::Parser::new(tokens);
    let (prog, _errs) = parser.parse().map_err(|e| format!("Parse error: {e}"))?;
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
            return Err(format!("Runtime error: {e}"));
        }
    }
    profiler.end_call("run");

    // Print report
    println!();
    println!("{}", profiler.report());

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
        std::env::temp_dir().join(format!("dalin-profile-test-{}-{}", pid, n))
    }

    fn create_temp_source(content: &str, dir: &PathBuf) -> PathBuf {
        std::fs::create_dir_all(dir).expect("Failed to create test dir");
        let file_path = dir.join("test.dal");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create test file");
        write!(file, "{}", content).expect("Failed to write test content");
        file_path
    }

    #[test]
    fn test_profile_nonexistent_file() {
        let result = run_profile("/nonexistent/file.dal", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_profile_empty_source() {
        let dir = test_dir();
        let src_path = create_temp_source("", &dir);
        let result = run_profile(src_path.to_str().unwrap(), false);
        assert!(result.is_ok(), "Profile empty source: {:?}", result.err());
    }

    #[test]
    fn test_profile_simple_expr() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let result = run_profile(src_path.to_str().unwrap(), false);
        assert!(result.is_ok(), "Profile simple expr: {:?}", result.err());
    }

    #[test]
    fn test_profile_with_verbose() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let result = run_profile(src_path.to_str().unwrap(), true);
        assert!(result.is_ok(), "Profile with verbose: {:?}", result.err());
    }
}
