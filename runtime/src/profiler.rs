/// Dalin L 3.0 — 采样式 Profiler
///
/// 在解释器入口拦截函数调用，统计每个函数的调用次数、总耗时、最大耗时，
/// 并提供按总时间排序的格式化报告。
use std::collections::HashMap;
use std::fmt::Write;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct CallStats {
    pub count: u64,
    pub total_ms: f64,
    pub max_ms: f64,
}

impl CallStats {
    fn new() -> Self {
        Self {
            count: 0,
            total_ms: 0.0,
            max_ms: 0.0,
        }
    }

    fn record(&mut self, duration_ms: f64) {
        self.count += 1;
        self.total_ms += duration_ms;
        if duration_ms > self.max_ms {
            self.max_ms = duration_ms;
        }
    }
}

/// 帧：记录一个被追踪的调用入口。
struct Frame {
    name: String,
    started: Instant,
}

/// 全局 Profiler 实例（用于 CLI 级别的全局 profiling）。
/// 支持嵌套调用（同一函数的多个活跃调用栈帧）。
pub struct Profiler {
    calls: HashMap<String, CallStats>,
    stack: Vec<Frame>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            calls: HashMap::new(),
            stack: Vec::new(),
        }
    }

    /// 开始追踪一个名为 `name` 的函数/调用。
    /// 支持任意深度的嵌套调用。
    #[allow(dead_code)] // Public API for future interpreter integration
    pub fn start_call(&mut self, name: &str) {
        self.stack.push(Frame {
            name: name.to_string(),
            started: Instant::now(),
        });
    }

    /// 结束最近一个名为 `name` 的活跃调用。
    #[allow(dead_code)] // Public API for future interpreter integration
    pub fn end_call(&mut self, name: &str) {
        // 从栈顶往下找第一个匹配的 name
        for i in (0..self.stack.len()).rev() {
            if self.stack[i].name == name {
                let frame = self.stack.remove(i);
                let elapsed = frame.started.elapsed().as_secs_f64() * 1000.0;
                let entry = self
                    .calls
                    .entry(name.to_string())
                    .or_insert_with(CallStats::new);
                entry.record(elapsed);
                return;
            }
        }
        // 未找到匹配的——忽略（不匹配的 end_call）
    }

    /// 返回按总时间排序的报告表格。
    pub fn report(&self) -> String {
        let mut items: Vec<_> = self.calls.iter().collect();
        items.sort_by(|a, b| {
            b.1.total_ms
                .partial_cmp(&a.1.total_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_ms: f64 = items.iter().map(|(_, s)| s.total_ms).sum();

        let mut out = String::new();
        writeln!(out, "=== Dalin L 3.0 Profiler Report ===").unwrap();
        writeln!(out, "Total time: {:.1}ms", total_ms).unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "{:<24} {:>6}   {:>10}   {:>10}",
            "Function", "Count", "Total (ms)", "Max (ms)"
        )
        .unwrap();
        writeln!(out, "{}", "-".repeat(58)).unwrap();
        for (name, stats) in &items {
            writeln!(
                out,
                "{:<24} {:>6}   {:>10.1}   {:>10.1}",
                name, stats.count, stats.total_ms, stats.max_ms
            )
            .unwrap();
        }
        writeln!(out).unwrap();
        out
    }

    pub fn reset(&mut self) {
        self.calls.clear();
        self.stack.clear();
    }
}

// Simplified: no RAII guard; just use start_call / end_call directly.

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_basic_profile() {
        let mut p = Profiler::new();
        p.start_call("foo");
        sleep(Duration::from_millis(10));
        p.end_call("foo");
        let report = p.report();
        assert!(report.contains("foo"));
        // count shows as "   1" (right-aligned in 6-char field)
        assert!(
            report.contains("   1"),
            "report should contain count of 1: {}",
            report
        );
    }

    #[test]
    fn test_nested_calls() {
        let mut p = Profiler::new();
        p.start_call("outer");
        p.start_call("inner");
        sleep(Duration::from_millis(5));
        p.end_call("inner");
        sleep(Duration::from_millis(10));
        p.end_call("outer");
        let report = p.report();
        assert!(report.contains("inner"));
        assert!(report.contains("outer"));
        // outer 应排在 inner 前面（总时间更多）
        let outer_pos = report.find("outer").unwrap();
        let inner_pos = report.find("inner").unwrap();
        assert!(outer_pos < inner_pos);
    }

    #[test]
    fn test_reset() {
        let mut p = Profiler::new();
        p.start_call("x");
        p.end_call("x");
        p.reset();
        let report = p.report();
        // After reset, "x" should not appear
        assert!(
            !report.contains("\nx"),
            "after reset, 'x' should not be in report: {}",
            report
        );
    }

    #[test]
    fn test_multiple_same_call() {
        let mut p = Profiler::new();
        for _ in 0..3 {
            p.start_call("bar");
            p.end_call("bar");
        }
        let stats = p.calls.get("bar").unwrap();
        assert_eq!(stats.count, 3);
    }
}
