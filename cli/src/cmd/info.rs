use crate::util;

pub fn run(json: bool) -> Result<(), String> {
    let banner = util::banner("INFO");
    println!("{}", banner);

    println!("\n  +-----------------------------------------+");
    println!("  |  Dalin L Compiler Info                  |");
    println!("  +-----------------------------------------+");
    println!("  |  Version:        v0.1.0                 |");
    println!("  |  Edition:        2026                   |");
    println!("  |  Target:         aarch64-apple-darwin   |");
    println!("  +-----------------------------------------+");

    println!("\n  Supported Phases:");
    for phase in &["A", "B", "C", "D", "E", "F", "G", "H"] {
        println!("    [OK] Phase {} - Completed", phase);
    }

    println!("\n  Features:");
    println!("    * Three-Channel Architecture (Parser->Semantics->Runtime)");
    println!("    * Seven-Channel Type Inference");
    println!("    * QN1 Backend + Time-Aware Runtime");
    println!("    * Self-Healing Runtime Engine");
    println!("    * Module/Package System + Stdlib (22 modules)");
    println!("    * Agent-Native Concurrency (spawn + channel)");
    println!("    * DLVM Bytecode VM");

    if json {
        println!(
            "\n  {{ \"version\": \"0.1.0\", \"phases\": [\"A\",\"B\",\"C\",\"D\",\"E\",\"F\",\"G\",\"H\"] }}"
        );
    }

    println!("\n  ============================================");
    println!("  |   INFO COMPLETE OK                        |");
    println!("  ============================================");
    Ok(())
}
