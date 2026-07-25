use chrono::Local;

pub fn banner(title: &str) -> String {
    let width = 60;
    let sep = "=".repeat(width);
    let date = Local::now().format("%Y-%m-%d %H:%M");
    format!("{}\n  Dalin L 3.0 — {} | {}\n{}", sep, title, date, sep)
}

pub fn ok(label: &str, msg: &str) -> Result<(), String> {
    println!("  [OK] {} ✓", label);
    println!("       {}", msg);
    Ok(())
}

pub fn err(label: &str, msg: &str) -> Result<(), String> {
    eprintln!("  [FAIL] {} ✗", label);
    eprintln!("         {}", msg);
    Err(msg.to_string())
}

pub fn warn(label: &str, msg: &str) {
    eprintln!("  [WARN] {} ⚠", label);
    eprintln!("         {}", msg);
}

pub fn section(title: &str) {
    println!("\n  --- {} ---", title);
}
