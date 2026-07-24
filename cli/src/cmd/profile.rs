use dalin_compiler::ty2::parse_governance;
use dalin_compiler::{lexer, parser};
use dalin_runtime::cognitive::{ConfidenceGate, ConfidenceLevel, GovernanceChecker};
use dalin_runtime::interpreter::Interpreter;

pub fn run(input: &str, gov: &str, confidence: &str) -> Result<(), String> {
    let src = std::fs::read_to_string(input)
        .map_err(|e| format!("Cannot read '{}': {}", input, e))?;

    // Lex
    let mut lex = lexer::Lexer::new(&src);
    let tokens = lex.tokenize().map_err(|e| format!("Lex error: {}", e))?;

    // Parse
    let mut p = parser::Parser::new(tokens);
    let prog = p.parse();
    for err in p.recovered() {
        eprintln!("  ⚠ Parse warning: {}", err);
    }

    println!("╔════════════════════════════════════════════╗");
    println!("║   Dalin L Performance Profiler            ║");
    println!("╚════════════════════════════════════════════╝");
    println!("Source:      {}", input);
    println!("Statements:  {}", prog.statements.len());
    println!("Governance:  {}", gov);
    println!("Confidence:  {}", confidence);
    println!();

    // Parse governance level
    let governance_level = parse_governance(gov);
    let confidence_level = ConfidenceLevel::from_annotation(Some(confidence));

    // Interpret with profiling and cognitive runtime
    let mut interp = Interpreter::new();
    interp.governance_checker = GovernanceChecker::new(governance_level);
    interp.confidence_gate = ConfidenceGate::new(confidence_level);
    interp.enable_profiling();

    match interp.interpret(&prog) {
        Ok(_) => {
            println!("✅ Execution completed successfully\n");
        }
        Err(e) => {
            eprintln!("  ❌ Runtime error: {}", e);
        }
    }

    // Print cognitive report
    println!("── Cognitive Report ──");
    println!("Phases:\n{}", interp.cognitive_machine.report());
    println!("Governance:\n{}", interp.governance_checker.report());
    println!("Confidence:\n{}", format_confidence_log(&interp.confidence_gate));
    println!("Timing:\n{}", interp.time_monitor.report());

    // Print profiling report
    println!("{}", interp.profile_report());

    Ok(())
}

fn format_confidence_log(gate: &dalin_runtime::cognitive::ConfidenceGate) -> String {
    let mut out = String::new();
    for (name, level, ok) in &gate.gate_log {
        out.push_str(&format!("  {} {}: {:?}\n", if *ok { "✅" } else { "❌" }, name, level));
    }
    out
}
