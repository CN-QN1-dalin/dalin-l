use dalin_runtime::interpreter::{RuntimeError, run_source};

// ========== Demo 1: Monitor Platform ==========

fn run_monitor_demo() -> Result<(), RuntimeError> {
    let src = r#"
struct MetricsPoint {
    cpu: float,
    mem: float,
    err: float
}

fn make_point(c, m, e) @ pure @ cpu {
    return MetricsPoint(c, m, e)
}

fn count_alerts(metrics, cpu_limit, mem_limit, err_limit) @ pure @ cpu {
    let count = len(metrics)
    let alerts = 0
    
    let i = 0
    while i < count {
        let p = metrics[i]
        if p.cpu > cpu_limit {
            let alerts = alerts + 1
        }
        if p.mem > mem_limit {
            let alerts = alerts + 1
        }
        if p.err > err_limit {
            let alerts = alerts + 1
        }
        let i = i + 1
    }
    
    return alerts
}

fn main() @ pure @ cpu {
    let metrics = [
        make_point(65.0, 55.0, 2.0),
        make_point(78.0, 62.0, 1.5),
        make_point(92.0, 71.0, 3.2),
        make_point(85.0, 80.0, 2.8),
        make_point(45.0, 60.0, 0.5),
        make_point(88.0, 82.0, 6.1),
        make_point(72.0, 68.0, 1.8),
        make_point(95.0, 88.0, 7.5)
    ]
    
    let total = count_alerts(metrics, 90.0, 85.0, 5.0)
    let n = len(metrics)
    return total + int(n)
}
"#;
    let results = run_source(src)?;
    println!("Monitor demo results: {:?}", results);
    Ok(())
}

// ========== Demo 2: Trading Engine ==========

fn run_trading_demo() -> Result<(), RuntimeError> {
    let src = r#"
struct Trade {
    buy_price: float,
    sell_price: float,
    quantity: int
}

fn make_trade(bp, sp, qty) @ pure @ cpu {
    return Trade(bp, sp, qty)
}

fn calc_pnl(trade) @ pure @ cpu {
    let diff = trade.sell_price - trade.buy_price
    let qty = float(trade.quantity)
    return diff * qty
}

fn sum_array(arr) @ pure @ cpu {
    let total = 0.0
    let n = len(arr)
    let i = 0
    while i < n {
        let total = total + arr[i]
        let i = i + 1
    }
    return total
}

fn count_positive(arr) @ pure @ cpu {
    let cnt = 0
    let n = len(arr)
    let i = 0
    while i < n {
        if arr[i] > 0 {
            let cnt = cnt + 1
        }
        let i = i + 1
    }
    return cnt
}

fn main() @ pure @ cpu {
    let trades = [
        make_trade(100.0, 105.0, 10),
        make_trade(200.0, 190.0, 10),
        make_trade(50.0, 60.0, 20),
        make_trade(80.0, 85.0, 15)
    ]
    
    // Calculate PnL for each trade
    let pnls = [
        (105.0 - 100.0) * 10.0,
        (190.0 - 200.0) * 10.0,
        (60.0 - 50.0) * 20.0,
        (85.0 - 80.0) * 15.0
    ]
    
    let total_pnl = sum_array(pnls)
    let wins = count_positive(pnls)
    let n = len(trades)
    
    return int(total_pnl * 100 + float(wins) * 100 + float(n))
}
"#;
    let results = run_source(src)?;
    println!("Trading demo results: {:?}", results);
    Ok(())
}

// ========== Demo 3: Code Review System ==========

fn run_codereview_demo() -> Result<(), RuntimeError> {
    let src = r#"
struct Issue {
    message: string,
    severity: int
}

fn make_issue(msg, sev) @ pure @ cpu {
    return Issue(msg, sev)
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
        if issue.severity > 0 {
            let total_score = total_score - issue.severity
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
    let ok = check_naming("myVar")
    
    return score + int(n) + int(ok)
}
"#;
    let results = run_source(src)?;
    println!("CodeReview demo results: {:?}", results);
    Ok(())
}

// ========== Demo 4: DevOps Pipeline ==========

fn run_pipeline_demo() -> Result<(), RuntimeError> {
    let src = r#"
enum StageStatus {
    Pending,
    Running,
    Passed,
    Failed
}

struct PipelineStage {
    name: string,
    status: int
}

fn make_stage(name, status) @ pure @ cpu {
    return PipelineStage(name, status)
}

fn run_lint(packages) @ pure @ cpu {
    let n = len(packages)
    if n > 0 {
        let name = "lint"
        return make_stage(name, 2)
    }
    let name = "lint"
    return make_stage(name, 3)
}

fn run_tests(packages) @ pure @ cpu {
    let n = len(packages)
    if n > 0 {
        let name = "test"
        return make_stage(name, 2)
    }
    let name = "test"
    return make_stage(name, 3)
}

fn build_artifacts(packages) @ pure @ cpu {
    let n = len(packages)
    if n > 0 {
        let name = "build"
        return make_stage(name, 2)
    }
    let name = "build"
    return make_stage(name, 3)
}

fn execute_stages(lint_result, test_result, build_result) @ pure @ cpu {
    let passed = 0
    let failed = 0
    let stages_count = 3
    
    let i = 0
    while i < 1 {
        let s1 = lint_result
        if s1.status != 2 {
            let failed = failed + 1
        } else {
            let passed = passed + 1
        }
        let i = i + 1
    }
    
    return passed + failed
}

fn main() @ pure @ cpu {
    let packages = ["core", "api", "cli"]
    
    let lint = run_lint(packages)
    let test = run_tests(packages)
    let build = build_artifacts(packages)
    
    let result = execute_stages(lint, test, build)
    return result
}
"#;
    let results = run_source(src)?;
    println!("Pipeline demo results: {:?}", results);
    Ok(())
}

fn main() {
    println!("\n=== Demo 1: Monitor Platform ===");
    match run_monitor_demo() {
        Ok(_) => println!("[PASS] Monitor demo ran successfully\n"),
        Err(e) => println!("[FAIL] Monitor demo: {}\n", e),
    }

    println!("=== Demo 2: Trading Engine ===");
    match run_trading_demo() {
        Ok(_) => println!("[PASS] Trading demo ran successfully\n"),
        Err(e) => println!("[FAIL] Trading demo: {}\n", e),
    }

    println!("=== Demo 3: Code Review System ===");
    match run_codereview_demo() {
        Ok(_) => println!("[PASS] CodeReview demo ran successfully\n"),
        Err(e) => println!("[FAIL] CodeReview demo: {}\n", e),
    }

    println!("=== Demo 4: DevOps Pipeline ===");
    match run_pipeline_demo() {
        Ok(_) => println!("[PASS] Pipeline demo ran successfully\n"),
        Err(e) => println!("[FAIL] Pipeline demo: {}\n", e),
    }
}
