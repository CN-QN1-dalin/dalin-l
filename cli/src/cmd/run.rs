use crate::util;
use dalin_compiler::ty2::parse_governance;
use dalin_compiler::{lexer, parser};
use dalin_runtime::cognitive::{ConfidenceGate, ConfidenceLevel, GovernanceChecker};
use dalin_runtime::interpreter::Interpreter;
use std::thread;
use std::time::Duration;

pub fn run(input: &str, watch: bool, verbose: bool, gov: &str, confidence: &str) -> Result<(), String> {
    let banner = util::banner("RUN");
    println!("{}", banner);
    if verbose {
        println!("  governance={} confidence={}", gov, confidence);
    }

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    let governance_level = parse_governance(gov);
    let confidence_level = ConfidenceLevel::from_annotation(Some(confidence));

    let mut compiled_ok = false;

    loop {
        if compiled_ok && watch {
            println!("\n  [watch] Waiting for changes...");
            thread::sleep(Duration::from_secs(1));
        } else if watch {
            compiled_ok = true;
        }

        let src = std::fs::read_to_string(input)
            .map_err(|e| format!("Cannot read '{}': {}", input, e))?;

        let mut lex = lexer::Lexer::new(&src);
        match lex.tokenize() {
            Ok(tokens) => {
                let mut p = parser::Parser::new(tokens);
                let prog = p.parse();
                for err in p.recovered() {
                    eprintln!("  ⚠ Parse warning: {}", err);
                }
                let _ = util::ok("compile", &format!("{} statements", prog.statements.len()));

                let mut interp = Interpreter::new();
                interp.governance_checker = GovernanceChecker::new(governance_level.clone());
                interp.confidence_gate = ConfidenceGate::new(confidence_level.clone());

                match interp.interpret(&prog) {
                    Ok(_) => {
                        if verbose {
                            println!("\n  Runtime execution completed.");
                            println!("\n── Cognitive Report ──");
                            println!("Phases:\n{}", interp.cognitive_machine.report());
                            println!("Governance:\n{}", interp.governance_checker.report());
                            let fmt_conf = format!("{}", &interp.confidence_gate);
                            if !fmt_conf.is_empty() {
                                println!("Confidence:\n{}", fmt_conf);
                            }
                            println!("Timing:\n{}", interp.time_monitor.report());
                        }
                    }
                    Err(e) => {
                        println!("\n  ❌ Runtime error: {}", e);
                        if !watch {
                            return Err(format!("{}", e));
                        }
                    }
                }
            }
            Err(e) => {
                if !watch {
                    return Err(format!("{}", e));
                }
            }
        }

        if !watch {
            break;
        }
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   RUN COMPLETE                    ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
