use criterion::{Criterion, criterion_group, criterion_main};
use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use dalin_compiler::ty2::SevenChannelInferencer;

use std::fs;

fn load_stdlib_sources() -> Vec<String> {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stdlib");
    let mut sources = Vec::new();
    if let Ok(dir) = base.read_dir() {
        for entry in dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "dal") {
                if let Ok(src) = fs::read_to_string(&path) {
                    sources.push(src);
                }
            }
        }
    }
    sources
}

fn tokenize(combined: &str) -> Vec<dalin_compiler::token::Token> {
    let mut lexer = Lexer::new(combined);
    lexer.tokenize().expect("tokenize stdlib should succeed")
}

fn bench_lexer(c: &mut Criterion) {
    let sources = load_stdlib_sources();
    let combined: String = sources.join("\n");

    c.bench_function("lexer_standard_library", |b| {
        b.iter(|| {
            let tokens = tokenize(&combined);
            assert!(!tokens.is_empty());
        });
    });
}

fn bench_parse(c: &mut Criterion) {
    let sources = load_stdlib_sources();
    let combined: String = sources.join("\n");

    c.bench_function("parse_standard_library", |b| {
        b.iter_with_setup(
            || (tokenize(&combined), combined.len()),
            |(tokens, len)| {
                let mut parser = Parser::new(tokens);
                let (prog, errors) = parser.parse().expect("parse stdlib sources should succeed");
                assert!(
                    !prog.statements.is_empty(),
                    "stdlib should contain statements"
                );
                assert!(errors.is_empty(), "stdlib should parse without errors");
                // Prevent bench from being optimized away
                assert!(len > 0, "source should not be empty");
            },
        );
    });
}

fn bench_type_check(c: &mut Criterion) {
    let sources = load_stdlib_sources();
    let combined: String = sources.join("\n");

    c.bench_function("type_check_standard_library", |b| {
        b.iter_with_setup(
            || {
                let tokens = tokenize(&combined);
                let mut parser = Parser::new(tokens);
                parser.parse().expect("parse stdlib sources should succeed").0
            },
            |prog| {
                let mut inf = SevenChannelInferencer::new();
                inf.infer_program(&prog);
            },
        );
    });
}

criterion_group!(benches, bench_lexer, bench_parse, bench_type_check);
criterion_main!(benches);
