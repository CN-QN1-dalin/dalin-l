// 临时诊断：检查 stdlib 所有 .dal 文件是否能零错误解析
use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;

#[test]
fn stdlib_all_files_parse_clean() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stdlib");
    let mut total = 0;
    let mut errors = 0;
    let mut failures: Vec<(String, usize, String)> = Vec::new();
    for entry in std::fs::read_dir(&base).unwrap().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "dal") {
            total += 1;
            let src = std::fs::read_to_string(&path).unwrap();
            let mut lex = Lexer::new(&src);
            let tokens = match lex.tokenize() {
                Ok(t) => t,
                Err(e) => {
                    errors += 1;
                    failures.push((path.display().to_string(), 0, format!("LEX: {e}")));
                    continue;
                }
            };
            let mut parser = Parser::new(tokens);
            match parser.parse() {
                Ok((_, errs)) => {
                    if !errs.is_empty() {
                        errors += 1;
                        let first = &errs[0];
                        failures.push((
                            path.display().to_string(),
                            errs.len(),
                            format!("L{}C{}: {}", first.line, first.column, first.message),
                        ));
                    }
                }
                Err(e) => {
                    errors += 1;
                    failures.push((path.display().to_string(), 1, format!("FATAL: {e}")));
                }
            }
        }
    }
    println!("== {} 文件, {} 有错误 ==", total, errors);
    for (f, n, msg) in &failures {
        println!("❌ {} ({} err): {}", f, n, msg);
    }
    assert!(errors == 0, "{} stdlib files have parse errors", errors);
}
