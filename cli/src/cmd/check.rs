use crate::util;

pub fn run(input: &str, verbose: bool, json: bool) -> Result<(), String> {
    let banner = util::banner("CHECK");
    println!("{}", banner);

    if !std::path::Path::new(input).exists() {
        return Err(format!("Source file '{}' does not exist", input));
    }

    let src =
        std::fs::read_to_string(input).map_err(|e| format!("Cannot read '{}': {}", input, e))?;

    let (report, errors) = dalin_compiler::compile_check(&src);

    if verbose {
        println!("{}", report.trim_end());
    }

    if errors.is_empty() {
        println!("  ✅ 七通道检查通过（无错误）");
    } else {
        println!("  ⚠️  发现 {} 个错误:", errors.len());
        for err in &errors {
            println!("  ❌ {}", err.to_string().trim_end());
        }
    }

    if json {
        let status = if errors.is_empty() { "ok" } else { "error" };
        println!("\n{{ \"status\": \"{}\", \"file\": \"{}\", \"errors\": {} }}", status, input, errors.len());
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   CHECK COMPLETE ✓                ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
