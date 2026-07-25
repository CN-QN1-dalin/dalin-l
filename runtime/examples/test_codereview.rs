use dalin_runtime::interpreter::{RuntimeError, run_source};

fn run_codereview_demo() -> Result<(), RuntimeError> {
    // Issue struct takes (message, severity)
    let src = r#"
struct Issue {
    msg: string,
    sev: int
}

fn make_issue(m, s) @ pure @ cpu {
    return Issue(m, s)
}

fn check_naming(var_name) @ pure @ cpu {
    let first_char = var_name[0]
    if first_char == first_char {
        return true
    }
    return false
}

fn score_issues(issues) @ pure @ cpu {
    let total_score = 0
    let n = len(issues)
    let i = 0
    while i < n {
        let issue = issues[i]
        if issue.sev > 0 {
            let total_score = total_score - issue.sev
        }
        let i = i + 1
    }
    return total_score
}

fn main() @ pure @ cpu {
    let issues = [
        make_issue("bad naming", 1),
        make_issue("deep nesting", 2),
        make_issue("unused var", 1)
    ]
    
    let score = score_issues(issues)
    let n = len(issues)
    let flag = check_naming("myVar")
    
    return score + int(n) + int(flag)
}
"#;
    let results = run_source(src)?;
    println!("CodeReview demo results: {:?}", results);
    Ok(())
}

fn main() {
    match run_codereview_demo() {
        Ok(_) => println!("[PASS]"),
        Err(e) => println!("[FAIL] {}", e),
    }
}
