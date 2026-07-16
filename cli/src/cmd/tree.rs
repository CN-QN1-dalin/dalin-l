use crate::util;

pub fn run(package: &str) -> Result<(), String> {
    let banner = util::banner("TREE");
    println!("{}", banner);

    let toml_path = if package.is_empty() { "dalin.toml".to_string() } 
                     else { format!("{}/dalin.toml", package) };

    let content = match std::fs::read_to_string(&toml_path) {
        Ok(c) => c,
        Err(_) => { println!("\n  [mock] No dalin.toml found at '{}'", toml_path); return mock_tree(); }
    };

    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in content.lines() {
        if line.trim() == "[dependencies]" { in_deps = true; continue; }
        if line.starts_with('[') && in_deps { break; }
        if in_deps && line.contains('=') {
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() == 2 {
                deps.push((parts[0].trim(), parts[1].trim().trim_matches('"')));
            }
        }
    }

    if deps.is_empty() { println!("\n  (no dependencies found)"); }
    else {
        println!("\n  Dependencies:");
        for (name, ver) in &deps { println!("  - {} @ {}", name, ver); }
    }

    println!("\n  ┌─────────────────────────────────┐");
    println!("  │  Dependency Tree               │");
    println!("  ├─────────────────────────────────┤");
    println!("  │  project (workspace root)      │");
    for (i, (name, ver)) in deps.iter().enumerate() {
        let prefix = if i == deps.len()-1 { "└──" } else { "├──" };
        println!("  │  {} {} @ {}", prefix, name, ver);
    }
    println!("  │  └── stdlib (built-in)         │");
    println!("  └─────────────────────────────────┘");
    Ok(())
}

fn mock_tree() -> Result<(), String> {
    println!("\n  ┌─────────────────────────────────┐");
    println!("  │  Dependency Tree (mock)        │");
    println!("  ├─────────────────────────────────┤");
    println!("  │  mock-project v0.1.0           │");
    println!("  │  ├── serde @ 1.0.0             │");
    println!("  │  │   ├── syn @ 2.0.0           │");
    println!("  │  │   └── quote @ 1.0.0         │");
    println!("  │  ├── tokio @ 1.35.0            │");
    println!("  │  └── stdlib (built-in)        │");
    println!("  └─────────────────────────────────┘");
    Ok(())
}
