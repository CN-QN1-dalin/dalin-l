/// Dalin L 3.0 — Performance Profiler
///
/// Lightweight runtime instrumentation: tracks function call counts,
/// wall-clock execution time, and generates hot-spot reports.
///
/// Integration: pass `&mut Profiler` into `Interpreter::eval_expr` and
/// `Interpreter::call_function` to collect per-call metrics.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per-function profiling statistics
#[derive(Debug, Clone)]
pub struct FnStats {
    pub name: String,
    pub calls: u64,
    pub total_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
}

impl Default for FnStats {
    fn default() -> Self {
        Self {
            name: String::new(),
            calls: 0,
            total_time: Duration::ZERO,
            min_time: Duration::MAX,
            max_time: Duration::ZERO,
        }
    }
}

impl FnStats {
    pub fn avg_time(&self) -> Duration {
        if self.calls == 0 {
            Duration::ZERO
        } else {
            self.total_time / self.calls as u32
        }
    }

    /// Pct of total profiled time this function consumed
    pub fn pct_of(&self, total: Duration) -> f64 {
        if total.is_zero() {
            0.0
        } else {
            self.total_time.as_secs_f64() / total.as_secs_f64() * 100.0
        }
    }
}

/// Expression-level timings (fine-grained, for hot-spot analysis)
#[derive(Debug, Clone)]
pub struct ExprSample {
    pub line: usize,
    pub op: String,
    pub duration: Duration,
}

impl ExprSample {
    pub fn new(line: usize, op: impl Into<String>, duration: Duration) -> Self {
        Self {
            line,
            op: op.into(),
            duration,
        }
    }
}

/// The main profiler that collects timing data during interpretation
#[derive(Debug)]
pub struct Profiler {
    /// Per-function stats map: function name -> stats
    pub fn_stats: HashMap<String, FnStats>,
    /// Expression-level samples (for hot-spot analysis)
    pub expr_samples: Vec<ExprSample>,
    /// Overall start time
    start: Option<Instant>,
    /// Current function call stack (for nested timing)
    call_stack: Vec<(String, Instant)>,
    /// Sampling rate: 0 = off, 1 = every call, 10 = every 10th call, etc.
    pub sample_rate: u64,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            fn_stats: HashMap::new(),
            expr_samples: Vec::new(),
            start: None,
            call_stack: Vec::new(),
            sample_rate: 1, // Profile every call by default
        }
    }

    /// Start profiling session
    pub fn start(&mut self) {
        self.start = Some(Instant::now());
        self.fn_stats.clear();
        self.expr_samples.clear();
        self.call_stack.clear();
    }

    /// Record the start of a function call
    pub fn enter_fn(&mut self, name: &str) {
        let now = Instant::now();
        self.call_stack.push((name.to_string(), now));
    }

    /// Record the end of a function call
    pub fn exit_fn(&mut self) {
        let now = Instant::now();
        if let Some((name, enter_time)) = self.call_stack.pop() {
            let duration = now.duration_since(enter_time);
            let entry = self.fn_stats.entry(name.clone()).or_default();
            entry.name = name;
            entry.calls += 1;
            entry.total_time += duration;
            if duration < entry.min_time {
                entry.min_time = duration;
            }
            if duration > entry.max_time {
                entry.max_time = duration;
            }
        }
    }

    /// Record an expression sample (line-level hot-spot data)
    pub fn sample_expr(&mut self, line: usize, op: &str, duration: Duration) {
        self.expr_samples.push(ExprSample::new(line, op, duration));
    }

    /// Total execution time since start
    pub fn total_time(&self) -> Duration {
        self.start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Generate a human-readable profiling report
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str("╔════════════════════════════════════════════╗\n");
        out.push_str("║        Dalin L Performance Profile        ║\n");
        out.push_str("╚════════════════════════════════════════════╝\n");

        let total = self.total_time();
        out.push_str(&format!("Total wall time: {:?}\n", total));
        if total.is_zero() {
            out.push_str("(No profiling data collected)\n");
            return out;
        }

        out.push_str(&format!(
            "Total function calls: {}\n\n",
            self.fn_stats.values().map(|s| s.calls).sum::<u64>()
        ));

        // Sort by total time, descending
        let mut sorted: Vec<&FnStats> = self.fn_stats.values().collect();
        sorted.sort_by_key(|s| s.total_time);
        sorted.reverse();

        out.push_str(&format!(
            "{:<30} {:>8} {:>12} {:>12} {:>12} {:>8}\n",
            "Function", "Calls", "Total", "Avg", "Min", "Max"
        ));
        out.push_str(&"-".repeat(82));
        out.push('\n');

        for stat in sorted {
            out.push_str(&format!(
                "{:<30} {:>8} {:>8?} {:>8?} {:>8?} {:>8?}\n",
                truncate(&stat.name, 28),
                stat.calls,
                        stat.total_time,
                stat.avg_time(),
                stat.min_time,
                stat.max_time,
            ));
        }

        // Top hot-spots (expression-level), top 10
        if !self.expr_samples.is_empty() {
            out.push_str("\n\n── Hot Spots (top 10) ──\n");
            let mut hot_exprs = self.expr_samples.clone();
            hot_exprs.sort_by_key(|e| e.duration);
            hot_exprs.reverse();
            for sample in hot_exprs.iter().take(10) {
                out.push_str(&format!(
                    "  Ln{:>4} {:<20} {:?}\n",
                    sample.line, sample.op, sample.duration
                ));
            }
        }

        out
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}..", &s[..max.saturating_sub(2)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_empty() {
        let p = Profiler::new();
        assert!(p.fn_stats.is_empty());
        let r = p.report();
        assert!(r.contains("No profiling data"));
    }

    #[test]
    fn test_profiler_one_call() {
        let mut p = Profiler::new();
        p.start();
        p.enter_fn("foo");
        std::thread::sleep(std::time::Duration::from_millis(1));
        p.exit_fn();
        assert_eq!(p.fn_stats.len(), 1);
        assert_eq!(p.fn_stats["foo"].calls, 1);
    }

    #[test]
    fn test_profiler_nested_calls() {
        let mut p = Profiler::new();
        p.start();
        p.enter_fn("outer");
        p.enter_fn("inner");
        std::thread::sleep(std::time::Duration::from_millis(1));
        p.exit_fn();
        p.exit_fn();
        assert_eq!(p.fn_stats.len(), 2);
        assert_eq!(p.fn_stats["inner"].calls, 1);
        assert_eq!(p.fn_stats["outer"].calls, 1);
    }

    #[test]
    fn test_profiler_many_calls() {
        let mut p = Profiler::new();
        p.start();
        for _ in 0..10 {
            p.enter_fn("hot");
            p.exit_fn();
        }
        assert_eq!(p.fn_stats["hot"].calls, 10);
        assert_eq!(p.fn_stats.len(), 1);
    }

    #[test]
    fn test_profiler_avg_time() {
        let mut p = Profiler::new();
        p.start();
        p.enter_fn("add");
        p.exit_fn();
        p.enter_fn("add");
        p.exit_fn();
        let s = &p.fn_stats["add"];
        assert_eq!(s.calls, 2);
        assert_eq!(s.avg_time(), s.total_time / 2);
    }

    #[test]
    fn test_expr_sampling() {
        let mut p = Profiler::new();
        p.start();
        let d = Duration::from_micros(42);
        p.sample_expr(10, "BinaryOp(+)", d);
        assert_eq!(p.expr_samples.len(), 1);
        let s = &p.expr_samples[0];
        assert_eq!(s.line, 10);
        assert_eq!(s.op, "BinaryOp(+)");
        assert_eq!(s.duration, d);
    }
}
