use dalin_runtime::interpreter::run_source;

fn main() {
    // Test: simple moving average calculation with loops
    let src = r#"
fn main() @ pure @ cpu {
    let prices = [100.0, 101.5, 103.0, 104.5, 102.0, 105.0, 107.0, 106.5]
    let period = 3
    
    // Calculate MA: sum 3 consecutive and divide by 3
    // MA[0] = avg of prices[0..3]
    let ma_result = (prices[0] + prices[1] + prices[2]) / 3.0
    return int(ma_result * 100)
}
"#;
    match run_source(src) {
        Ok(results) => println!("Simple MA OK: {:?}", results),
        Err(e) => println!("Simple MA FAILED: {}", e),
    }

    // Test: PnL calculation for a trade
    let src2 = r#"
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

fn main() @ pure @ cpu {
    let t = make_trade(100.0, 105.0, 10)
    let pnl = calc_pnl(t)
    return int(pnl * 100)
}
"#;
    match run_source(src2) {
        Ok(results) => println!("PnL calc OK: {:?}", results),
        Err(e) => println!("PnL calc FAILED: {}", e),
    }

    // Test: cross strategy - detect crossover
    let src3 = r#"
fn main() @ pure @ cpu {
    let ma_short = [101.0, 102.0, 103.0, 101.5]
    let ma_long = [100.5, 100.8, 101.0, 102.0]
    
    // Golden cross: short crosses above long
    let signal = 0
    let i = 1
    while i < 4 {
        if ma_short[i] > ma_long[i] {
            if ma_short[i - 1] <= ma_long[i - 1] {
                signal = 1  // golden cross
            }
        }
        let i = i + 1
    }
    
    return signal
}
"#;
    match run_source(src3) {
        Ok(results) => println!("Cross strategy OK: {:?}", results),
        Err(e) => println!("Cross strategy FAILED: {}", e),
    }
}
