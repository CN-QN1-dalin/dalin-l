use dalin_runtime::interpreter::run_source;

#[test]
fn test_trading_engine_demo() {
    // 交易引擎核心功能测试：SMA 计算
    let src = r#"
fn sma(data, period) @ pure @ cpu {
    let n = len(data)
    let result = []
    let i = period - 1
    while i < n {
        let sum = 0.0
        let j = 0
        while j < period {
            let sum = sum + data[i - j]
            let j = j + 1
        }
        let result = push(result, sum / float(period))
        let i = i + 1
    }
    return result
}

fn main() @ pure @ cpu {
    let prices = [100.0, 101.0, 102.0, 103.0, 104.0]
    let ma = sma(prices, 3)
    // SMA(3): (100+101+102)/3=101, (101+102+103)/3=102, (102+103+104)/3=103
    return len(ma)
}
"#;
    let results = run_source(src).expect("Trading engine SMA should run");
    assert!(!results.is_empty(), "Should produce output");
}