//! break / continue 循环控制流测试
//!
//! 回归护栏：Dalin L 3.0 曾长期缺失 break/continue 关键字，
//! stdlib 中 11 处已有用法在运行时报 `Undefined variable: 'break'`。
//! 本套测试锁定语义，防止后续重构再次退化。

use dalin_runtime::env::Value;
use dalin_runtime::interpreter::run_source;

fn run_last(src: &str) -> Value {
    let full = format!("{src}\nmain()");
    run_source(&full)
        .unwrap_or_else(|e| panic!("run failed: {e}\nsrc: {full}"))
        .into_iter()
        .last()
        .unwrap_or(Value::None)
}

fn run_err(src: &str) -> String {
    let full = format!("{src}\nmain()");
    match run_source(&full) {
        Ok(_) => panic!("expected runtime error, but run succeeded\nsrc: {full}"),
        Err(e) => e.to_string(),
    }
}

// ── while ──

#[test]
fn while_break_terminates_loop() {
    // i==3 时终止：累加 0+1+2 = 3
    let v = run_last(
        "fn main() @ pure @ cpu {
            let i = 0
            let sum = 0
            while i < 10 {
                if i == 3 { break }
                sum = sum + i
                i = i + 1
            }
            return sum
        }",
    );
    assert_eq!(v, Value::Int(3), "break 应在 i==3 时终止循环");
}

#[test]
fn while_continue_skips_rest_of_iteration() {
    // 跳过偶数：1+3+5+7+9 = 25
    let v = run_last(
        "fn main() @ pure @ cpu {
            let i = 0
            let sum = 0
            while i < 10 {
                i = i + 1
                if i % 2 == 0 { continue }
                sum = sum + i
            }
            return sum
        }",
    );
    assert_eq!(v, Value::Int(25), "continue 应跳过本轮剩余语句");
}

#[test]
fn break_only_exits_innermost_loop() {
    // 外层 3 轮 × 内层各累加 2 次 = 6
    let v = run_last(
        "fn main() @ pure @ cpu {
            let outer = 0
            let count = 0
            while outer < 3 {
                let inner = 0
                while inner < 5 {
                    if inner == 2 { break }
                    count = count + 1
                    inner = inner + 1
                }
                outer = outer + 1
            }
            return count
        }",
    );
    assert_eq!(v, Value::Int(6), "break 只应终止最内层循环");
}

#[test]
fn return_penetrates_loop_interception() {
    // return 哨兵不能被循环的 break/continue 拦截层吞掉
    let v = run_last(
        "fn main() @ pure @ cpu {
            let i = 0
            while i < 10 {
                if i == 4 { return 99 }
                i = i + 1
            }
            return -1
        }",
    );
    assert_eq!(v, Value::Int(99), "return 应穿透循环直接返回");
}

// ── for ──

#[test]
fn for_break_stops_iteration() {
    let v = run_last(
        "fn main() @ pure @ cpu {
            let total = 0
            for v in [5, 7, 0, 100] {
                if v == 0 { break }
                total = total + v
            }
            return total
        }",
    );
    assert_eq!(v, Value::Int(12), "for + break 应在 0 处停止：5+7=12");
}

#[test]
fn for_continue_skips_element() {
    let v = run_last(
        "fn main() @ pure @ cpu {
            let total = 0
            for v in [3, -1, 4, -9] {
                if v < 0 { continue }
                total = total + v
            }
            return total
        }",
    );
    assert_eq!(v, Value::Int(7), "for + continue 应跳过负数：3+4=7");
}

// ── 误用诊断 ──

#[test]
fn break_outside_loop_reports_clear_error() {
    let err = run_err(
        "fn main() @ pure @ cpu {
            break
            return 1
        }",
    );
    assert!(
        err.contains("break") && err.contains("循环"),
        "循环外 break 应给出明确诊断，实际: {err}"
    );
}

#[test]
fn continue_outside_loop_reports_clear_error() {
    let err = run_err(
        "fn main() @ pure @ cpu {
            continue
            return 1
        }",
    );
    assert!(
        err.contains("continue") && err.contains("循环"),
        "循环外 continue 应给出明确诊断，实际: {err}"
    );
}

// ── 已有 stdlib 用法回归 ──

#[test]
fn stdlib_strings_str_count_uses_break() {
    // strings::str_count（原 strutil.dal，去重后并入 canonical strings 模块）内部用 break
    let v = run_last("fn main() @ pure @ cpu { return strings::str_count(\"ababab\", \"ab\") }");
    assert_eq!(v, Value::Int(3), "str_count(ababab, ab) = 3");
}
