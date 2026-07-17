use crate::util;

pub fn run(name: &str, lib_only: bool, git_init: bool) -> Result<(), String> {
    let banner = util::banner("INIT");
    println!("{}", banner);

    let out_dir = std::path::Path::new(name);
    if out_dir.exists() { return Err(format!("Directory '{}' already exists", name)); }

    std::fs::create_dir_all(out_dir.join("src")).map_err(|e| format!("Cannot create src/: {}", e))?;
    std::fs::create_dir_all(out_dir.join("tests")).map_err(|e| format!("Cannot create tests/: {}", e))?;

    let toml_content = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nstdlib = \"latest\"\n", name);
    std::fs::write(out_dir.join("dalin.toml"), toml_content).map_err(|e| format!("Cannot write dalin.toml: {}", e))?;
    println!("  ✅ Created dalin.toml");

    let main_code = if lib_only {
        "@lib\nfn add(a: Int, b: Int) -> Int { return a + b; }\n"
    } else {
        r#"@main
fn main() -> Int {
    println("Hello, Dalin L 3.0!");
    return 0;
}"#
    };
    std::fs::write(out_dir.join("src/main.dal"), main_code).map_err(|e| format!("Cannot write src/main.dal: {}", e))?;
    println!("  ✅ Created src/main.dal");

    let test_code = r"?test
fn test_basic() -> Bool { return true; }
";
    std::fs::write(out_dir.join("tests/basic_test.dal"), test_code).map_err(|e| format!("Cannot write tests/basic_test.dal: {}", e))?;
    println!("  ✅ Created tests/basic_test.dal");

    std::fs::write(out_dir.join(".gitignore"), "target/\n.dalan/\n*.rlib\n").map_err(|e| format!("Cannot write .gitignore: {}", e))?;
    println!("  ✅ Created .gitignore");

    println!("\n  Project '{}' initialized!", name);
    println!("  Navigate with: cd {} && dalib check", name);

    if git_init {
        println!("  Initializing git...");
        match std::process::Command::new("git").arg("init").current_dir(out_dir).status() {
            Ok(s) if s.success() => println!("  ✅ Git repository initialized"),
            _ => util::warn("git", "Failed or not available"),
        }
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   INIT COMPLETE ✓                 ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}
