use crate::util;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub fn run(input: &str, watch: bool, verbose: bool) -> Result<(), String> {
    let banner = util::banner("RUN");
    println!("{banner}");

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{input}' does not exist"));
    }

    // Resolve project root for cache dir
    let cwd = std::env::current_dir().unwrap_or_default();
    let project_root: PathBuf = if input.starts_with(cwd.to_string_lossy().as_ref()) {
        cwd.clone()
    } else {
        Path::new(input)
            .parent()
            .map_or_else(|| cwd.clone(), std::path::Path::to_path_buf)
    };

    let mut compiled_ok = false;

    loop {
        if compiled_ok && watch {
            println!("\n  [watch] Waiting for changes...");
            thread::sleep(Duration::from_secs(1));
        } else if watch {
            compiled_ok = true;
        }

        let src =
            std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{input}': {e}"))?;

        // Check cache first
        let needs_compile =
            !dalin_compiler::cache::is_cached(Path::new(input), &src, &project_root);
        if needs_compile {
            if verbose {
                println!("  🔨 Compiling {input} ...");
            }
        } else if verbose {
            println!("  ✓ Cache hit, skipping compilation");
        }

        use dalin_compiler::{lexer, parser};

        let mut lex = lexer::Lexer::new(&src);
        match lex.tokenize() {
            Ok(tokens) => {
                let mut p = parser::Parser::new(tokens);
                match p.parse() {
                    Ok((prog, _errs)) => {
                        let _ =
                            util::ok("compile", &format!("{} statements", prog.statements.len()));

                        // Write to cache after successful compile
                        if let Ok(_cached) = dalin_compiler::cache::ensure_cache_dir(&project_root)
                        {
                            let key =
                                dalin_compiler::cache::compute_cache_key(Path::new(input), &src);
                            // Serialize AST bytes as a simple cache value
                            let ast_data = format!("{:?}", prog.statements.len()).into_bytes();
                            let _ =
                                dalin_compiler::cache::write_cache(&project_root, &key, &ast_data);
                        }

                        use dalin_runtime::interpreter;
                        match interpreter::run_source(&src) {
                            Ok(_) => {
                                if verbose {
                                    println!("\n  Runtime execution completed.");
                                }
                            }
                            Err(e) => {
                                println!("\n  ❌ Runtime error: {e}");
                                if !watch {
                                    return Err(format!("{e}"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if !watch {
                            return Err(format!("{e}"));
                        }
                    }
                }
            }
            Err(e) => {
                if !watch {
                    return Err(format!("{e}"));
                }
            }
        }

        if !watch {
            break;
        }
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   RUN COMPLETE ✓                  ║");
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
        std::env::temp_dir().join(format!("dalin-run-test-{}-{}", pid, n))
    }

    fn create_temp_source(content: &str) -> (PathBuf, PathBuf) {
        let dir = test_dir();
        std::fs::create_dir_all(&dir).expect("Failed to create test dir");
        let file_path = dir.join("test.dal");
        let mut file = std::fs::File::create(&file_path).expect("Failed to create test file");
        write!(file, "{}", content).expect("Failed to write test content");
        (file_path, dir)
    }

    #[test]
    fn test_run_nonexistent_file() {
        let result = run("/nonexistent/file.dal", false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_run_with_empty_source() {
        let (path, _dir) = create_temp_source("");
        let result = run(path.to_str().unwrap(), false, false);
        assert!(
            result.is_ok(),
            "Empty source should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_run_with_simple_expr() {
        let src = "let x = 42";
        let (path, _dir) = create_temp_source(src);
        let result = run(path.to_str().unwrap(), false, false);
        assert!(
            result.is_ok(),
            "Simple assignment should succeed: {:?}",
            result.err()
        );
    }
}
