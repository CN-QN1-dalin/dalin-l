use dalin_runtime::interpreter::run_source;

fn main() {
    // Complete Monitor Platform demo
    let src = r#"
struct MetricsPoint {
    cpu: float,
    mem: float,
    err: float
}

fn make_point(c, m, e) @ pure @ cpu {
    return MetricsPoint(c, m, e)
}

// Print header and table
fn print_header() @ pure @ cpu {
    print("========================================")
    print("  Server Monitoring Dashboard")
    print("========================================")
    print("")
    return 0
}

// Process metrics: print data + check thresholds
fn process_metrics(metrics, cpu_limit, mem_limit, err_limit) @ pure @ cpu {
    let count = len(metrics)
    
    print_header()
    
    // Print each sample
    let i = 0
    while i < count {
        let p = metrics[i]
        print("Sample #" + str(i) + ": CPU=" + str(p.cpu) + " MEM=" + str(p.mem) + " ERR=" + str(p.err))
        let i = i + 1
    }
    
    print("")
    print("----------------------------------------")
    print("Thresholds: CPU>" + str(cpu_limit) + " MEM>" + str(mem_limit) + " ERR>" + str(err_limit))
    print("----------------------------------------")
    
    // Check thresholds
    let j = 0
    while j < count {
        let pt = metrics[j]
        if pt.cpu > cpu_limit {
            print("  ALERT: CPU at sample #" + str(j) + " is " + str(pt.cpu) + " (limit " + str(cpu_limit) + ")")
        }
        if pt.mem > mem_limit {
            print("  ALERT: Memory at sample #" + str(j) + " is " + str(pt.mem) + " (limit " + str(mem_limit) + ")")
        }
        if pt.err > err_limit {
            print("  ALERT: Error Rate at sample #" + str(j) + " is " + str(pt.err) + " (limit " + str(err_limit) + ")")
        }
        let j = j + 1
    }
    
    print("")
    print("Monitoring complete.")
    return count
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
    
    process_metrics(metrics, 90.0, 85.0, 5.0)
    return 0
}
"#;
    match run_source(src) {
        Ok(results) => {
            println!("\n=== Monitor Demo SUCCESS ===");
            println!("Final results: {:?}", results);
        },
        Err(e) => println!("\n=== Monitor Demo ERROR: {} ===", e),
    }
}
