use dalin_runtime::interpreter::run_source;

#[test]
fn test_monitor_platform() {
    let src = r#"
struct MetricsPoint {
    cpu: float,
    mem: float,
    err: float
}

fn make_point(c, m, e) @ pure @ cpu {
    return MetricsPoint(c, m, e)
}

fn check_thresholds(metrics, cpu_limit, mem_limit, err_limit) @ pure @ cpu {
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
    
    let total_alerts = check_thresholds(metrics, 90.0, 85.0, 5.0)
    let n = len(metrics)
    return total_alerts
}
"#;
    let results = run_source(src).expect("Monitor demo should run without error");
    assert!(!results.is_empty());
}
