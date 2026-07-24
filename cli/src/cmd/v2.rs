/// Legacy run_v2_demo() — seven-channel type system demo
use dalin_compiler::{lexer, parser, ty2};

pub fn run() -> Result<(), String> {
    println!("============================================================");
    println!("  Dalin L 3.0 — 七通道类型系统");
    println!("============================================================");

    println!("\n--- 效应格 (Effect Lattice) ---");
    use ty2::Effect;
    println!("  pure <= io?    {}", Effect::Pure.leq(&Effect::Io));
    println!("  pure <= async? {}", Effect::Pure.leq(&Effect::Async));
    println!("  pure <= spawn? {}", Effect::Pure.leq(&Effect::Spawn));
    println!("  io <= async?   {}", Effect::Io.leq(&Effect::Async));

    println!("\n--- 能力格 (Capability Lattice) ---");
    use ty2::Capability;
    println!("  cpu <= gpu? {}", Capability::Cpu.leq(&Capability::Gpu));
    println!("  cpu <= sfa? {}", Capability::Cpu.leq(&Capability::Sfa));
    println!("  cpu <= net? {}", Capability::Cpu.leq(&Capability::Net));

    // 七通道推断演示
    println!("\n--- 七通道类型推断 ---");
    let src = r#"
        fn pure_add(a, b) -> int @ pure @ cpu {
            return a + b
        }
        async fn fetch(url) {
            return "data"
        }
        let x = 42
    "#;

    let mut lex = lexer::Lexer::new(src);
    match lex.tokenize() {
        Ok(tokens) => {
            let mut parser = parser::Parser::new(tokens);
            let prog = parser.parse();
            if !parser.recovered().is_empty() {
                for err in parser.recovered() {
                    eprintln!("  ⚠ Parse warning: {}", err);
                }
            }
            let mut infer = ty2::SevenChannelInferencer::new();
            infer.infer_program(&prog);
            print!("{}", infer.print_report());
        }
        Err(e) => println!("  Lex error: {}", e),
    }

    Ok(())
}
