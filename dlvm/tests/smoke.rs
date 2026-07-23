//! Dalin L DLVM — smoke + match/guard bytecode tests
use dalin_compiler::lexer::Lexer;
use dalin_compiler::parser::Parser;
use dalin_dlvm::{BytecodeCompiler, Value, Vm};

fn run_src(src: &str) -> Value {
    let mut lex = Lexer::new(src);
    let tokens = lex.tokenize().expect("lex failed");
    let mut parser = Parser::new(tokens);
    let prog = parser.parse().expect("parse failed");
    let mut compiler = BytecodeCompiler::new();
    let funcs = compiler.compile(&prog);
    let mut vm = Vm::new(funcs);
    vm.run().expect("vm run failed")
}

#[test]
fn dlvm_compiles() {
    // Smoke test: crate compiles without errors
}

#[test]
fn match_guard_bytecode() {
    // guard `x if x > 10` must be evaluated in the bytecode path
    let src = r#"
        fn classify(n) {
            match n {
                1 => "one"
                2 => "two"
                x if x > 10 => "big"
                _ => "other"
            }
        }
        classify(15)
    "#;
    assert_eq!(run_src(src), Value::Str("big".into()));
}

#[test]
fn match_guard_false_falls_through() {
    // 5 matches neither `1` nor guard `x > 10` → must fall through to `_`
    let src = r#"
        fn classify(n) {
            match n {
                1 => "one"
                x if x > 10 => "big"
                _ => "other"
            }
        }
        classify(5)
    "#;
    assert_eq!(run_src(src), Value::Str("other".into()));
}

#[test]
fn match_multi_arm_fallthrough() {
    // fix: pattern mismatch must fall through to the NEXT arm, not skip all arms
    let src = r#"
        fn pick(n) {
            match n {
                1 => "a"
                2 => "b"
                3 => "c"
                _ => "z"
            }
        }
        pick(2)
    "#;
    assert_eq!(run_src(src), Value::Str("b".into()));
}
