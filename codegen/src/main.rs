/// Dalin L 3.0 — Codegen CLI
///
/// Usage: dalin-codegen input.dal [-o output.wat]
use std::fs;
use std::path::Path;
use dalin_codegen::WasmCodegen;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.dal> [-o output.wat]", args[0]);
        std::process::exit(1);
    }

    let input = &args[1];
    let output = if args.len() > 2 && args[2] == "-o" {
        args[3].clone()
    } else {
        let stem = Path::new(input).file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out");
        format!("{}.wat", stem)
    };

    let src = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input, e);
            std::process::exit(1);
        }
    };

    let mut cg = WasmCodegen::new();
    match cg.compile_source(&src) {
        Ok(wat) => {
            fs::write(&output, &wat)
                .unwrap_or_else(|e| {
                    eprintln!("Error writing '{}': {}", output, e);
                    std::process::exit(1);
                });
            println!("✅ Generated: {}", output);
        }
        Err(e) => {
            eprintln!("❌ Codegen error: {}", e);
            std::process::exit(1);
        }
    }
}
