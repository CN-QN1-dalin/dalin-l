use crate::util;

pub fn run(name: &str, lib_only: bool, git_init: bool) -> Result<(), String> {
    let banner = util::banner("INIT");
    println!("{}", banner);

    let out_dir = std::path::Path::new(name);
    if out_dir.exists() {
        return Err(format!("Directory '{}' already exists", name));
    }

    std::fs::create_dir_all(out_dir.join("src"))
        .map_err(|e| format!("Cannot create src/: {}", e))?;
    std::fs::create_dir_all(out_dir.join("tests"))
        .map_err(|e| format!("Cannot create tests/: {}", e))?;

    let toml_content = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\nstdlib = \"latest\"\n",
        name
    );
    std::fs::write(out_dir.join("dalin.toml"), toml_content)
        .map_err(|e| format!("Cannot write dalin.toml: {}", e))?;
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
    std::fs::write(out_dir.join("src/main.dal"), main_code)
        .map_err(|e| format!("Cannot write src/main.dal: {}", e))?;
    println!("  ✅ Created src/main.dal");

    let test_code = r"?test
fn test_basic() -> Bool { return true; }
";
    std::fs::write(out_dir.join("tests/basic_test.dal"), test_code)
        .map_err(|e| format!("Cannot write tests/basic_test.dal: {}", e))?;
    println!("  ✅ Created tests/basic_test.dal");

    std::fs::write(out_dir.join(".gitignore"), "target/\n.dalan/\n*.rlib\n")
        .map_err(|e| format!("Cannot write .gitignore: {}", e))?;
    println!("  ✅ Created .gitignore");

    println!("\n  Project '{}' initialized!", name);
    println!("  Navigate with: cd {} && dalib check", name);

    if git_init {
        println!("  Initializing git...");
        match std::process::Command::new("git")
            .arg("init")
            .current_dir(out_dir)
            .status()
        {
            Ok(s) if s.success() => println!("  ✅ Git repository initialized"),
            _ => util::warn("git", "Failed or not available"),
        }
    }

    println!("\n  ╔═══════════════════════════════════╗");
    println!("  ║   INIT COMPLETE ✓                 ║");
    println!("  ╚═══════════════════════════════════╝");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_dir() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("dalin-init-test-{}-{}", pid, n))
    }

    #[test]
    fn test_init_creates_project() {
        let dir = test_dir();
        let project_name = dir.join("test-project");
        let name = project_name.to_str().unwrap().to_string();

        let result = run(&name, false, false);
        assert!(result.is_ok(), "Init should succeed: {:?}", result.err());

        // Verify directory structure
        assert!(project_name.join("src").exists(), "src/ should exist");
        assert!(project_name.join("tests").exists(), "tests/ should exist");
        assert!(
            project_name.join("dalin.toml").exists(),
            "dalin.toml should exist"
        );
        assert!(
            project_name.join("src/main.dal").exists(),
            "src/main.dal should exist"
        );
        assert!(
            project_name.join("tests/basic_test.dal").exists(),
            "tests/basic_test.dal should exist"
        );
        assert!(
            project_name.join(".gitignore").exists(),
            ".gitignore should exist"
        );

        // Verify content
        let toml = std::fs::read_to_string(project_name.join("dalin.toml")).unwrap();
        assert!(
            toml.contains("test-project"),
            "TOML should contain project name"
        );
        assert!(toml.contains("dalin"), "TOML should reference dalin");
    }

    #[test]
    fn test_init_lib_only() {
        let dir = test_dir();
        let project_name = dir.join("test-lib");
        let name = project_name.to_str().unwrap().to_string();

        let result = run(&name, true, false);
        assert!(
            result.is_ok(),
            "Init lib should succeed: {:?}",
            result.err()
        );

        let main_dal = std::fs::read_to_string(project_name.join("src/main.dal")).unwrap();
        assert!(main_dal.contains("@lib"), "Lib project should contain @lib");
    }

    #[test]
    fn test_init_existing_dir_fails() {
        let dir = test_dir();
        let project_name = dir.join("existing-project");
        std::fs::create_dir_all(&project_name).unwrap();
        let name = project_name.to_str().unwrap().to_string();

        let result = run(&name, false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_init_with_git() {
        let dir = test_dir();
        let project_name = dir.join("git-project");
        let name = project_name.to_str().unwrap().to_string();

        let result = run(&name, false, true);
        // Git init may fail in CI, that's acceptable
        // The function should still create files
        assert!(
            project_name.join("src/main.dal").exists(),
            "src/main.dal should exist"
        );
    }
}
