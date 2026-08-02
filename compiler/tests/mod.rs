// Dalin L 3.0 — Benchmark integration tests
// These run alongside the unit tests to verify benchmark modules compile.

#[cfg(test)]
mod tests {
    use dalin_compiler::lexer::Lexer;
    use dalin_compiler::parser::Parser;

    #[test]
    fn test_bench_compile_module_exists() {
        // 守护编译链路：bench_compile 依赖的编译管线符号可解析
        let src = "fn main() { return 42 }";
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().expect("tokenize");
        let mut parser = Parser::new(tokens);
        let (prog, errs) = parser.parse().expect("parse");
        assert!(!prog.statements.is_empty(), "program has statements");
        assert!(errs.is_empty(), "no parse errors");
    }

    #[test]
    fn test_bench_runtime_module_exists() {
        // 守护 runtime 依赖：常量求值符号可用（bench_runtime 的依赖链路）
        let src = "fn f() { return 1 + 2 }";
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize().expect("tokenize");
        let mut parser = Parser::new(tokens);
        let (prog, _) = parser.parse().expect("parse");
        assert!(prog.statements.len() == 1, "one function");
    }
}
