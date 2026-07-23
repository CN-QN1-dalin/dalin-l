use crate::ast::Expr;

/// 时间约束：描述函数的延迟/超时/吞吐量保证
#[derive(Debug, Clone, PartialEq)]
pub struct TimeConstraint {
    pub latency_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub throughput: Option<u64>,
}

impl Default for TimeConstraint { fn default() -> Self { Self::new() } }

impl TimeConstraint {
    pub fn new() -> Self {
        Self { latency_ms: None, timeout_ms: None, throughput: None }
    }
    /// 合并两个约束：取最严格的值（最小值）
    pub fn meet(a: &TimeConstraint, b: &TimeConstraint) -> TimeConstraint {
        TimeConstraint {
            latency_ms: match (a.latency_ms, b.latency_ms) {
                (Some(x), Some(y)) => Some(x.min(y)), (Some(x), None) => Some(x), (None, Some(y)) => Some(y), _ => None,
            },
            timeout_ms: match (a.timeout_ms, b.timeout_ms) {
                (Some(x), Some(y)) => Some(x.min(y)), (Some(x), None) => Some(x), (None, Some(y)) => Some(y), _ => None,
            },
            throughput: match (a.throughput, b.throughput) {
                (Some(x), Some(y)) => Some(x.min(y)), (Some(x), None) => Some(x), (None, Some(y)) => Some(y), _ => None,
            },
        }
    }
    /// 检查时间约束是否满足要求。actual 实际约束必须 ≥ required 要求。
    pub fn satisfies(&self, required: &TimeConstraint) -> bool {
        if let (Some(req_lat), Some(act_lat)) = (required.latency_ms, self.latency_ms)
            && act_lat > req_lat
        { return false; }
        if let (Some(req_timeout), Some(act_timeout)) = (required.timeout_ms, self.timeout_ms)
            && act_timeout > req_timeout
        { return false; }
        if let (Some(req_tput), Some(act_tput)) = (required.throughput, self.throughput)
            && act_tput < req_tput
        { return false; }
        true
    }
}

impl std::fmt::Display for TimeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(ms) = self.latency_ms { parts.push(format!("latency({}ms)", ms)); }
        if let Some(ms) = self.timeout_ms { parts.push(format!("timeout({}ms)", ms)); }
        if let Some(t) = self.throughput { parts.push(format!("throughput({}/s)", t)); }
        if parts.is_empty() { write!(f, "no-time-constraint") } else { write!(f, "{}", parts.join(", ")) }
    }
}

/// 从字符串解析时间约束：`@latency(50ms)` → `TimeConstraint { latency_ms: Some(50), ... }`
pub fn parse_time_constraint(key: &str, value: &str) -> TimeConstraint {
    let mut tc = TimeConstraint::new();
    match key {
        "latency" => { tc.latency_ms = value.trim_end_matches("ms").trim().parse::<u64>().ok(); }
        "timeout" => {
            if value.ends_with("s") && !value.ends_with("ms") {
                tc.timeout_ms = value.trim_end_matches("s").trim().parse::<u64>().ok().map(|x| x * 1000);
            } else {
                tc.timeout_ms = value.trim_end_matches("ms").trim().parse::<u64>().ok();
            }
        }
        "throughput" => { tc.throughput = value.trim_end_matches("/s").trim().parse::<u64>().ok(); }
        _ => {}
    }
    tc
}

/// 时间约束推断器
#[derive(Debug)]
pub struct TimeConstraintInferencer { pub errors: Vec<String> }

impl Default for TimeConstraintInferencer { fn default() -> Self { Self::new() } }

impl TimeConstraintInferencer {
    pub fn new() -> Self { Self { errors: Vec::new() } }

    pub fn infer_expr(&mut self, expr: &Expr) -> TimeConstraint {
        use crate::ast::Expr;
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::StringLiteral(_)
            | Expr::BoolLiteral(_) | Expr::CharLiteral(_) | Expr::Ident(_)
            | Expr::Array(_) | Expr::Range { .. } | Expr::OptionValue { .. } | Expr::ResultValue { .. }
            | Expr::BinaryOp { .. } | Expr::UnaryOp { .. } | Expr::IfExpr { .. } | Expr::MatchExpr { .. }
            => TimeConstraint { latency_ms: Some(0), timeout_ms: None, throughput: None },
            Expr::Call { .. } => TimeConstraint { latency_ms: Some(10), timeout_ms: None, throughput: None }, // default 10ms
            _ => TimeConstraint::new(),
        }
    }

    pub fn check(&mut self, actual: &TimeConstraint, required: &TimeConstraint, location: &str) {
        if !actual.satisfies(required) {
            self.errors.push(format!(
                "时间约束违规: {} 需要 {}，但实际仅 {}", location, required, actual
            ));
        }
    }
}
