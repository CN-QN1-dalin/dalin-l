use crate::util;

pub fn run(input: &str, verbose: bool, json: bool, quality: bool) -> Result<(), String> {
    let banner = util::banner("CHECK");
    println!("{banner}");

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{input}' does not exist"));
    }

    let src = std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{input}': {e}"))?;

    use dalin_compiler::{lexer, parser, ty};

    let mut lex = lexer::Lexer::new(&src);
    match lex.tokenize() {
        Ok(tokens) => {
            println!("  ✅ Lexer passed ({} tokens)", tokens.len());

            let mut p = parser::Parser::new(tokens);
            match p.parse() {
                Ok((prog, _errs)) => {
                    println!("  ✅ Parser passed ({} stmts)", prog.statements.len());

                    if verbose {
                        let mut infer = ty::TypeInferencer::new();
                        infer.infer_program(&prog);
                        println!("\n{}", infer.print_report().trim_end());
                    } else {
                        println!("  ✅ Type inference passed (--verbose for details)");
                    }

                    // Run quality engine after check passes
                    if quality {
                        let analyzer = dalin_compiler::quality::QualityAnalyzer::new(None);
                        let report = analyzer.analyze(&prog, Some(input));

                        if json {
                            println!("\n{}\n", report.format_json());
                        } else {
                            println!("\n{}", report.format_text("warn"));
                        }
                    }
                }
                Err(e) => {
                    return util::err("parser", &format!("{e}")).map_err(|_| String::new());
                }
            }
        }
        Err(e) => {
            return util::err("lexer", &format!("{e}")).map_err(|_| String::new());
        }
    }

    if json && !quality {
        println!("\n{{ \"status\": \"ok\", \"file\": \"{input}\" }}");
    }

    if !quality {
        println!("\n  ╔═══════════════════════════════════╗");
        println!("  ║   CHECK COMPLETE ✓                ║");
        println!("  ╚═══════════════════════════════════╝");
    }
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
        std::env::temp_dir().join(format!("dalin-check-test-{}-{}", pid, n))
    }

    fn create_temp_source(content: &str, dir: &PathBuf) -> PathBuf {
        std::fs::create_dir_all(dir).expect("Failed to create test dir");
        let file_path = dir.join("test.dal");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create test file");
        write!(file, "{}", content).expect("Failed to write test content");
        file_path
    }

    #[test]
    fn test_check_nonexistent_file() {
        let result = run("/nonexistent/file.dal", false, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_check_empty_source() {
        let dir = test_dir();
        let src_path = create_temp_source("", &dir);
        let result = run(src_path.to_str().unwrap(), false, false, false);
        assert!(result.is_ok(), "Check empty source: {:?}", result.err());
    }

    #[test]
    fn test_check_simple_expr() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let result = run(src_path.to_str().unwrap(), false, false, false);
        assert!(result.is_ok(), "Check simple expr: {:?}", result.err());
    }

    #[test]
    fn test_check_with_verbose() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let result = run(src_path.to_str().unwrap(), true, false, false);
        assert!(result.is_ok(), "Check with verbose: {:?}", result.err());
    }

    #[test]
    fn test_check_with_json_output() {
        let dir = test_dir();
        let src = "let x = 42";
        let src_path = create_temp_source(src, &dir);
        let result = run(src_path.to_str().unwrap(), false, true, false);
        assert!(result.is_ok(), "Check with JSON: {:?}", result.err());
    }
}
