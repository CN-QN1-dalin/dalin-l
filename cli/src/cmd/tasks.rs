/// Legacy run_tasks_demo() — TaskSpec demo
use dalin_compiler::{lexer, parser, task_spec, ty2};

pub fn run() -> Result<(), String> {
    println!("============================================================");
    println!("  Dalin L 3.0 — TaskSpec（控制面编译期契约）");
    println!("============================================================");

    let src = r#"
        fn pure_add(a, b) -> int @ pure @ cpu { return a + b }
        fn fetch(url) @ async @ net { return "data" }
        fn encode(x) -> string @ sfa { return x }
        fn fan_out() @ spawn @ cpu { return 0 }
    "#;

    let mut lex = lexer::Lexer::new(src);
    match lex.tokenize() {
        Ok(tokens) => {
            let mut parser = parser::Parser::new(tokens);
            match parser.parse() {
                Ok(prog) => {
                    let mut infer = ty2::SevenChannelInferencer::new();
                    infer.infer_program(&prog);
                    print!("{}", infer.print_report());

                    let specs = task_spec::from_program(&prog);
                    println!("=== TaskSpec ===");
                    for s in &specs {
                        println!("  {} : effect={:?} capability={:?}", s.fn_id, s.effect, s.capability);
                    }
                    if let Some(parent) = specs.iter().find(|s| s.fn_id == "fan_out") {
                        let child = parent.spawn_child("worker", ty2::Effect::Io, ty2::Capability::Cpu);
                        println!("\n  spawn: {} -> parent={:?}", child.fn_id, child.parent_task);
                    }
                }
                Err(e) => println!("  Parse error: {}", e),
            }
        }
        Err(e) => println!("  Lex error: {}", e),
    }

    Ok(())
}
