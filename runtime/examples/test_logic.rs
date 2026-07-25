use dalin_runtime::interpreter::run_source;

fn main() {
    // Test: array of structs, iterate and compute total P&L
    let src = r#"
struct Trade {
    buy_price: float,
    sell_price: float,
    quantity: int
}

fn calc_pnl(trade) @ pure @ cpu {
    let diff = trade.sell_price - trade.buy_price
    let qty = float(trade.quantity)
    return diff * qty
}

fn make_trade(bp, sp, qty) @ pure @ cpu {
    return Trade(bp, sp, qty)
}

fn main() @ pure @ cpu {
    let trades = [
        make_trade(100.0, 105.0, 10),
        make_trade(200.0, 190.0, 10),
        make_trade(50.0, 60.0, 20),
        make_trade(80.0, 85.0, 15)
    ]
    
    let total_pnl = 0.0
    let win_count = 0
    let trade_count = len(trades)
    let n = trade_count
    
    let i = 0
    while i < n {
        let t = trades[i]
        let pnl = calc_pnl(t)
        let total_pnl = total_pnl + pnl
        if pnl > 0 {
            let win_count = win_count + 1
        }
        let i = i + 1
    }
    
    let wins = win_count
    let wins_f = float(wins)
    let rate = (wins_f / float(n)) * 100.0
    return int(rate)
}
"#;
    match run_source(src) {
        Ok(results) => println!("Portfolio PnL test OK: {:?}", results),
        Err(e) => println!("Portfolio PnL test FAILED: {}", e),
    }

    // Test: array construction in function call
    let src2 = r#"
fn get_count(arr) @ pure @ cpu {
    return len(arr)
}

fn main() @ pure @ cpu {
    let nums = [1, 2, 3]
    let c = get_count(nums)
    return c
}
"#;
    match run_source(src2) {
        Ok(results) => println!("Array param test OK: {:?}", results),
        Err(e) => println!("Array param test FAILED: {}", e),
    }

    // Test: for loop over array with print
    let src3 = r#"
fn main() @ pure @ cpu {
    let items = [10, 20, 30]
    let sum = 0
    for v in items {
        let sv = int(v)
        let sum = sum + sv
    }
    return sum
}
"#;
    match run_source(src3) {
        Ok(results) => println!("For loop test OK: {:?}", results),
        Err(e) => println!("For loop test FAILED: {}", e),
    }
}
