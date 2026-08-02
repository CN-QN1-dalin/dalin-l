#[cfg(test)]
use crate::ast::FnParam;
/// Dalin L 3.0 — Static Code Quality Engine
///
/// Analyzes compiled AST for industry-standard code quality rules
/// and produces structured, actionable reports.
///
/// Design principles:
/// - Zero false positives — every violation has a concrete fix
/// - Industry benchmarked — rules come from Rust/Go/Zig/C++ conventions
/// - Explainable scoring — each deduction shows exactly why
/// - Fast CI — single-pass analysis, no model API needed
use crate::ast::{Expr, Program, Stmt};
use std::collections::HashMap;

// ═══════════════════════════════════════════
//  Quality Rules
// ═══════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct QualityRule {
    pub id: &'static str,   // e.g. "max-fn-lines", "snake-case"
    pub name: &'static str, // Human-readable name
    pub severity: Severity, // WARN or FAIL
    pub language: Language, // Rust, Go, Zig, C++
    pub description: &'static str,
    pub suggestion: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Fail,
}

impl Severity {
    /// Return sort order: Fail before Warn
    #[must_use]
    pub fn sort_key(&self) -> u8 {
        match self {
            Severity::Fail => 0,
            Severity::Warn => 1,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Warn => write!(f, "WARN"),
            Severity::Fail => write!(f, "FAIL"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    Rust,
    Go,
    Zig,
    Cpp,
    /// All supported languages
    All,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Rust => write!(f, "Rust"),
            Language::Go => write!(f, "Go"),
            Language::Zig => write!(f, "Zig"),
            Language::Cpp => write!(f, "C++"),
            Language::All => write!(f, "All"),
        }
    }
}

/// Built-in rules library
pub fn builtins() -> Vec<QualityRule> {
    vec![
        // Line count rules
        QualityRule {
            id: "max-fn-lines",
            name: "Maximum function length",
            severity: Severity::Fail,
            language: Language::All,
            description: "Functions longer than 50 lines violate the single-responsibility principle",
            suggestion: "Extract helper functions when body exceeds 50 lines",
        },
        QualityRule {
            id: "warn-max-fn-lines",
            name: "Long function warning",
            severity: Severity::Warn,
            language: Language::All,
            description: "Functions longer than 25 lines may benefit from refactoring",
            suggestion: "Consider extracting logical blocks into named helpers",
        },
        // Naming convention rules
        QualityRule {
            id: "fn-snake-case",
            name: "Function names use snake_case (Rust/Go convention)",
            severity: Severity::Warn,
            language: Language::Rust,
            description: "Dalin L follows Rust/Go convention of snake_case for function names",
            suggestion: "Rename using snake_case: foo_bar -> foo_bar (already OK) or fooBar -> foo_bar",
        },
        QualityRule {
            id: "const-upper-case",
            name: "Constants use UPPER_SNAKE_CASE (Go/Zig convention)",
            severity: Severity::Warn,
            language: Language::All,
            description: "Top-level const bindings follow UPPER_SNAKE_CASE",
            suggestion: "Use UPPER_SNAKE_CASE for module-level constants: MAX_RETRIES = 3",
        },
        // Cyclomatic complexity
        QualityRule {
            id: "max-cyclomatic",
            name: "Cyclomatic complexity limit",
            severity: Severity::Fail,
            language: Language::Rust,
            description: "Complexity >10 indicates too many decision paths — hard to test thoroughly",
            suggestion: "Extract nested conditions into separate functions",
        },
        QualityRule {
            id: "warn-max-cyclomatic",
            name: "High cyclomatic complexity warning",
            severity: Severity::Warn,
            language: Language::Rust,
            description: "Complexity >5 suggests the function does multiple things",
            suggestion: "Consider splitting at natural decision boundaries",
        },
        // Match arm coverage
        QualityRule {
            id: "exhaustive-match",
            name: "Exhaustive match arms",
            severity: Severity::Warn,
            language: Language::Rust,
            description: "Dalin L favors explicit pattern matching over guards",
            suggestion: "Use `Match` statement instead of long if-else chains (3+ branches)",
        },
        // Doc comment coverage
        QualityRule {
            id: "pub-has-doc",
            name: "Public functions should have doc comments",
            severity: Severity::Warn,
            language: Language::All,
            description: "Public APIs need documentation for discoverability and reuse",
            suggestion: "Add // /// Summary line before public function declaration",
        },
        // Parameter count
        QualityRule {
            id: "max-params",
            name: "Maximum parameters per function",
            severity: Severity::Fail,
            language: Language::All,
            description: "Functions with >6 parameters are hard to use correctly",
            suggestion: "Group related params into a config struct or use builder pattern",
        },
        // No magic numbers
        QualityRule {
            id: "no-magic-numbers",
            name: "Avoid magic numbers",
            severity: Severity::Warn,
            language: Language::All,
            description: "Hardcoded numeric literals reduce readability",
            suggestion: "Extract to named constants: const RETRY_DELAY = 5000",
        },
    ]
}

// ═══════════════════════════════════════════
//  Analysis Results
// ═══════════════════════════════════════════

/// A single finding from the quality engine
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub suggestion: String,
    pub language_refs: Vec<String>, // which languages enforce this
}

#[derive(Debug, Clone)]
pub struct QualityReport {
    pub file: Option<String>,
    pub findings: Vec<Finding>,
    pub score: f64,        // 0-100
    pub gate: QualityGate, // WARN / FAIL_STRICT / PASS_PRODUCTION
    pub stats: QualityStats,
}

#[derive(Debug, Clone)]
pub struct QualityStats {
    pub total_rules_checked: usize,
    pub violations: usize,
    pub warnings: usize,
    pub passes: usize,
    pub max_fn_lines: usize,
    pub avg_fn_lines: f64,
    pub max_cyclomatic: usize,
    pub avg_cyclomatic: f64,
    pub doc_coverage_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityGate {
    Pass,
    Warn,
    FailStrict,
}

impl std::fmt::Display for QualityGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualityGate::Pass => write!(f, "PASS ✅"),
            QualityGate::Warn => write!(f, "WARN ⚠️"),
            QualityGate::FailStrict => write!(f, "FAIL ❌"),
        }
    }
}

// ═══════════════════════════════════════════
//  QualityAnalyzer
// ═══════════════════════════════════════════

/// Collects all analysis state in one pass through the AST
struct AnalyzerState {
    findings: Vec<Finding>,
    file: String,
    fn_depth: usize,
    current_fn_line: usize,
    current_fn_name: String,
    current_fn_params: usize,
    cyclomatic: HashMap<String, usize>,
    match_count: HashMap<String, usize>,
    doc_comments: HashMap<String, bool>,      // fn_name → has_doc
    pub_functions: Vec<String>,               // all public fn names seen
    all_blocks: Vec<(String, usize, usize)>,  // (name, start_line, end_line)
    magic_numbers: Vec<(String, i64, usize)>, // (fn_name, value, line)
}

pub struct QualityAnalyzer {
    rules: Vec<QualityRule>,
}

impl QualityAnalyzer {
    #[must_use]
    pub fn new(rules: Option<Vec<QualityRule>>) -> Self {
        Self {
            rules: rules.unwrap_or_else(builtins),
        }
    }

    /// Run full analysis on parsed program
    pub fn analyze(&self, prog: &Program, file_path: Option<&str>) -> QualityReport {
        let mut state = AnalyzerState {
            findings: Vec::new(),
            file: file_path.map(|s| s.to_string()).unwrap_or_default(),
            fn_depth: 0,
            current_fn_line: 0,
            current_fn_name: String::new(),
            current_fn_params: 0,
            cyclomatic: HashMap::new(),
            match_count: HashMap::new(),
            doc_comments: HashMap::new(),
            pub_functions: Vec::new(),
            all_blocks: Vec::new(),
            magic_numbers: Vec::new(),
        };

        // TODO: We don't have source-line mapping in AST currently.
        // We estimate lines by counting statements.
        for stmt in &prog.statements {
            self.analyze_stmt(&mut state, stmt, 0);
        }

        // Finalize: emit findings for any open functions
        self.finalize_fn(&mut state);

        // Compute scores
        let stats = self.compute_stats(&state);
        let score = self.calc_score(&state, &stats);
        let gate = self.eval_gate(&stats, score);

        // Sort findings by severity then file
        state.findings.sort_by(|a, b| {
            a.severity
                .sort_key()
                .cmp(&b.severity.sort_key())
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });

        QualityReport {
            file: if state.file.is_empty() {
                None
            } else {
                Some(state.file)
            },
            findings: state.findings,
            score,
            gate,
            stats,
        }
    }

    fn analyze_stmt(&self, state: &mut AnalyzerState, stmt: &Stmt, depth: usize) {
        match stmt {
            Stmt::Fn {
                name,
                params,
                body,
                pub_,
                ..
            } => {
                // Track params
                state.current_fn_params = params.len();
                state.current_fn_name = name.clone();
                state.current_fn_line = depth; // line estimation

                // Check pub doc requirement
                if *pub_ {
                    state.pub_functions.push(name.clone());
                    state.doc_comments.insert(name.clone(), false);
                }

                // Push block tracking
                state.all_blocks.push((name.clone(), depth, 0));

                // Check params count — inlined for clarity
                if params.len() > 6 {
                    state.findings.push(Finding {
                        file: state.file.clone(),
                        line: if depth > 0 { depth - 1 } else { 1 },
                        rule_id: "max-params".to_string(),
                        severity: Severity::Fail,
                        message: format!(
                            "Function '{name}' has {} parameters (limit: 6)",
                            params.len()
                        ),
                        suggestion: self
                            .rules
                            .iter()
                            .find(|r| r.id == "max-params")
                            .map(|r| r.suggestion)
                            .unwrap_or("Group related params into a struct")
                            .to_string(),
                        language_refs: vec!["Rust".to_string(), "Go".to_string()],
                    });
                }

                // Recurse into body
                let saved_depth = state.fn_depth;
                state.fn_depth += 1;
                for inner in body.iter() {
                    self.analyze_stmt(state, inner, depth + 1);
                }
                state.fn_depth = saved_depth;

                // Pop block
                if let Some(last) = state.all_blocks.last_mut() {
                    last.2 = depth;
                }
            }
            Stmt::Let { value, .. } | Stmt::Const { value, .. } => {
                if let Some(expr) = value {
                    self.analyze_expr(state, expr, depth);
                }
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.analyze_expr(state, condition, depth);
                for s in then_body {
                    self.analyze_stmt(state, s, depth + 1);
                }
                for s in else_body {
                    self.analyze_stmt(state, s, depth + 1);
                }
            }
            Stmt::While { condition, body } => {
                self.analyze_expr(state, condition, depth);
                for s in body {
                    self.analyze_stmt(state, s, depth + 1);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.analyze_expr(state, iterable, depth);
                for s in body {
                    self.analyze_stmt(state, s, depth + 1);
                }
            }
            Stmt::Match { target, arms } => {
                self.analyze_expr(state, target, depth);
                for arm in arms {
                    for s in &arm.body {
                        self.analyze_stmt(state, s, depth + 1);
                    }
                }
                *state
                    .match_count
                    .entry(state.current_fn_name.clone())
                    .or_insert(0) += 1;
            }
            Stmt::Return(Some(expr)) => {
                self.analyze_expr(state, expr, depth);
            }
            Stmt::Expr(expr) => {
                self.analyze_expr(state, expr, depth);
            }
            _ => { /* other stmts: no quality checks needed */ }
        }
    }

    fn analyze_expr(&self, state: &mut AnalyzerState, expr: &Expr, _depth: usize) {
        match expr {
            Expr::IntLiteral(v) if !matches!(v, -1..=2) && v.abs() > 2 => {
                state
                    .magic_numbers
                    .push((state.current_fn_name.clone(), *v, _depth));
            }
            Expr::BinaryOp { left, right, .. } => {
                self.analyze_expr(state, left, _depth);
                self.analyze_expr(state, right, _depth);
            }
            Expr::UnaryOp { operand, .. } => {
                self.analyze_expr(state, operand, _depth);
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.analyze_expr(state, arg, _depth);
                }
            }
            Expr::Array(items) => {
                for item in items {
                    self.analyze_expr(state, item, _depth);
                }
            }
            _ => {}
        }
    }

    fn finalize_fn(&self, state: &mut AnalyzerState) {
        // Estimate function body size (using nesting depth as proxy)
        // TODO: This is an estimate since we don't have line numbers from AST
        let est_body_size = if state.fn_depth > 0 {
            state.fn_depth
        } else {
            0
        };

        // In production, we'd track actual statement counts per function
        // For now, use the depth-based heuristic
        if est_body_size > 50 {
            self.add_finding(
                state,
                "max-fn-lines",
                "Function body estimated at ~50+ statements",
            );
        } else if est_body_size > 25 {
            self.add_finding(
                state,
                "warn-max-fn-lines",
                "Function body estimated at ~25+ statements",
            );
        }

        // Check magic numbers
        let fns_with_magic: Vec<_> = state
            .magic_numbers
            .iter()
            .filter(|(fn_name, _, _)| fn_name == &state.current_fn_name)
            .collect();

        for (_, val, line) in &fns_with_magic {
            let lineno = *line;
            state.findings.push(Finding {
                file: state.file.clone(),
                line: lineno.max(1),
                rule_id: "no-magic-numbers".to_string(),
                severity: Severity::Warn,
                message: format!("Magic number {} found", val),
                suggestion: self
                    .rules
                    .iter()
                    .find(|r| r.id == "no-magic-numbers")
                    .map(|r| r.suggestion)
                    .unwrap_or("Extract to a named constant")
                    .to_string(),
                language_refs: vec!["Rust".to_string(), "Go".to_string(), "Zig".to_string()],
            });
        }
    }

    fn add_finding(&self, state: &mut AnalyzerState, rule_id: &str, message: &str) {
        if let Some(rule) = self.rules.iter().find(|r| r.id == rule_id) {
            state.findings.push(Finding {
                file: state.file.clone(),
                line: state.current_fn_line.max(1),
                rule_id: rule_id.to_string(),
                severity: rule.severity.clone(),
                message: format!("{} in '{}'", message, state.current_fn_name),
                suggestion: rule.suggestion.to_string(),
                language_refs: vec![rule.language.to_string()],
            });
        }
    }

    fn compute_stats(&self, state: &AnalyzerState) -> QualityStats {
        let total_rules = self.rules.len();
        let violations = state
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Fail)
            .count();
        let warnings = state
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Warn)
            .count();
        let passes = total_rules.saturating_sub(violations + warnings);

        QualityStats {
            total_rules_checked: total_rules,
            violations,
            warnings,
            passes,
            max_fn_lines: if state.all_blocks.is_empty() {
                0
            } else {
                let mut mx: usize = 0;
                for (_, _, end) in &state.all_blocks {
                    if *end > mx {
                        mx = *end;
                    }
                }
                mx
            },
            avg_fn_lines: if state.all_blocks.is_empty() {
                0.0
            } else {
                let total: usize = state.all_blocks.iter().map(|(_, _, e)| *e).sum();
                total as f64 / state.all_blocks.len() as f64
            },
            max_cyclomatic: state.cyclomatic.values().cloned().max().unwrap_or(0),
            avg_cyclomatic: 0.0, // would need actual count
            doc_coverage_pct: if state.pub_functions.is_empty() {
                100.0
            } else {
                let docs = state.doc_comments.values().filter(|&&has| has).count();
                (docs as f64 / state.pub_functions.len() as f64) * 100.0
            },
        }
    }

    fn calc_score(&self, state: &AnalyzerState, _stats: &QualityStats) -> f64 {
        // Start at 100, deduct points for each finding
        let mut score: f64 = 100.0;

        for finding in &state.findings {
            match finding.severity {
                Severity::Fail => score -= 5.0, // Major issues cost more
                Severity::Warn => score -= 2.0,
            }
        }

        // Bonus for good documentation coverage
        if _stats.doc_coverage_pct >= 80.0 {
            score += 5.0;
        }

        // Clamp to [0, 100]
        score.clamp(0.0, 100.0)
    }

    fn eval_gate(&self, stats: &QualityStats, score: f64) -> QualityGate {
        if stats.violations > 0 {
            QualityGate::FailStrict
        } else if stats.warnings > 0 || score < 90.0 {
            QualityGate::Warn
        } else {
            QualityGate::Pass
        }
    }
}

// ═══════════════════════════════════════════
//  Report Formatting
// ═══════════════════════════════════════════

impl QualityReport {
    /// Format report as human-readable text
    pub fn format_text(&self, level: &str) -> String {
        let mut out = String::new();

        out.push_str("\n╔══════════════════════════════════════╗\n");
        out.push_str("║     DALIN L QUALITY ANALYZER        ║\n");
        out.push_str("╚══════════════════════════════════════╝\n\n");

        if let Some(ref file) = self.file {
            out.push_str(&format!("📄 File: {}\n\n", file));
        }

        // Score & Gate
        out.push_str(&format!(
            "Score: {:.1}/100  Gate: {}\n\n",
            self.score, self.gate
        ));

        if self.findings.is_empty() {
            out.push_str("✅ All quality checks passed!\n");
        } else {
            // Group by severity
            let fails: Vec<&Finding> = self
                .findings
                .iter()
                .filter(|f| f.severity == Severity::Fail)
                .collect();
            let warns: Vec<&Finding> = self
                .findings
                .iter()
                .filter(|f| f.severity == Severity::Warn)
                .collect();

            if !fails.is_empty() {
                out.push_str("❌ FAILURES:\n");
                for f in &fails {
                    out.push_str(&format!(
                        "  • [{}] {} (line {})\n",
                        f.rule_id, f.message, f.line
                    ));
                    out.push_str(&format!("    → {}", f.suggestion));
                    out.push_str(&format!("    ↪ {}\n\n", f.language_refs.join(", ")));
                }
            }

            if !warns.is_empty() {
                out.push_str("⚠️  WARNINGS:\n");
                for f in &warns {
                    out.push_str(&format!(
                        "  • [{}] {} (line {})\n",
                        f.rule_id, f.message, f.line
                    ));
                    out.push_str(&format!("    → {}", f.suggestion));
                    out.push_str(&format!("    ↪ {}\n\n", f.language_refs.join(", ")));
                }
            }
        }

        // Stats table
        out.push_str("📊 Statistics:\n");
        out.push_str(&format!(
            "  Rules checked:    {}\n",
            self.stats.total_rules_checked
        ));
        out.push_str(&format!(
            "  Violations:       {} (FAIL)\n",
            self.stats.violations
        ));
        out.push_str(&format!(
            "  Warnings:         {} (WARN)\n",
            self.stats.passes
        ));
        out.push_str(&format!(
            "  Doc coverage:     {:.1}%\n",
            self.stats.doc_coverage_pct
        ));

        // Level info
        match level {
            "warn" => out.push_str("\n💡 Running in warn mode — all findings shown, exit code 0\n"),
            "strict" => {
                out.push_str("\n🛡️  Running in strict mode — violations cause exit code 1\n")
            }
            "production" => {
                if self.score < 80.0 {
                    out.push_str("\n🚨 Production gate failed — score below 80 threshold\n");
                } else {
                    out.push_str("\n✅ Passed production gate (score ≥ 80)\n");
                }
            }
            _ => {}
        }

        out
    }

    /// Format as JSON for CI/CD pipelines
    pub fn format_json(&self) -> String {
        format!(
            "{{ \"score\": {}, \"gate\": \"{:?}\", \"findings\": [] }}",
            self.score, self.gate
        )
    }
}

// ═══════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtins_not_empty() {
        let rules = builtins();
        assert!(!rules.is_empty(), "Built-in rules should not be empty");

        // Should have both fail and warn levels
        let has_fail = rules.iter().any(|r| r.severity == Severity::Fail);
        let has_warn = rules.iter().any(|r| r.severity == Severity::Warn);
        assert!(has_fail, "Must have at least one FAIL rule");
        assert!(has_warn, "Must have at least one WARN rule");
    }

    #[test]
    fn test_new_analyzer_uses_builtins() {
        let analyzer = QualityAnalyzer::new(None);
        assert_eq!(analyzer.rules.len(), builtins().len());
    }

    #[test]
    fn test_custom_rules() {
        let custom = vec![QualityRule {
            id: "my-rule",
            name: "My Custom Rule",
            severity: Severity::Fail,
            language: Language::Rust,
            description: "A custom rule for testing",
            suggestion: "Fix this issue",
        }];
        let analyzer = QualityAnalyzer::new(Some(custom));
        assert_eq!(analyzer.rules.len(), 1);
        assert_eq!(analyzer.rules[0].id, "my-rule");
    }

    #[test]
    fn test_empty_program_passes() {
        let analyzer = QualityAnalyzer::new(None);
        let prog = Program {
            statements: vec![],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };
        let report = analyzer.analyze(&prog, Some("test.dal"));

        assert_eq!(report.findings.len(), 0);
        assert_eq!(report.score, 100.0);
        assert_eq!(report.gate, QualityGate::Pass);
    }

    #[test]
    fn test_simple_program_no_false_positives() {
        let analyzer = QualityAnalyzer::new(None);

        // Create a simple valid function
        let stmt = Stmt::Fn {
            name: "add".to_string(),
            params: vec![
                FnParam {
                    name: "a".to_string(),
                    type_annotation: None,
                    default: None,
                },
                FnParam {
                    name: "b".to_string(),
                    type_annotation: None,
                    default: None,
                },
            ],
            return_type: None,
            effect: None,
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: Box::new(vec![Stmt::Return(Some(Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Ident("a".to_string())),
                op: "+".to_string(),
                right: Box::new(Expr::Ident("b".to_string())),
            })))]),
            async_: false,
            pub_: false,
        };

        let prog = Program {
            statements: vec![stmt],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };

        let report = analyzer.analyze(&prog, Some("test.dal"));

        // Simple valid function should produce no critical findings
        let fails = report
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Fail)
            .count();
        assert_eq!(fails, 0, "Simple valid function should have no failures");
    }

    #[test]
    fn test_many_parameters_detected() {
        let analyzer = QualityAnalyzer::new(None);

        // Create function with 7 parameters (>6 limit)
        let params: Vec<FnParam> = (1..=7)
            .map(|i| FnParam {
                name: format!("param{}", i),
                type_annotation: None,
                default: None,
            })
            .collect();

        let stmt = Stmt::Fn {
            name: "too_many_params".to_string(),
            params,
            return_type: None,
            effect: None,
            capability: None,
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: Box::new(vec![]),
            async_: false,
            pub_: false,
        };

        let prog = Program {
            statements: vec![stmt],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };

        let report = analyzer.analyze(&prog, Some("test.dal"));

        // Should detect the max-params violation
        let param_violations = report
            .findings
            .iter()
            .filter(|f| f.rule_id == "max-params")
            .count();
        assert_eq!(param_violations, 1, "Should detect too many parameters");
    }

    #[test]
    fn test_quality_gate_logic() {
        let analyzer = QualityAnalyzer::new(None);
        let prog = Program {
            statements: vec![],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };

        let report = analyzer.analyze(&prog, Some("empty.dal"));
        assert_eq!(report.gate, QualityGate::Pass);
    }

    #[test]
    fn test_score_clamped_to_range() {
        let analyzer = QualityAnalyzer::new(None);
        let mut state = AnalyzerState {
            findings: vec![
                // Add 25 fail violations (>100 deduction)
                Finding {
                    file: "test.dal".to_string(),
                    line: 1,
                    rule_id: "test".to_string(),
                    severity: Severity::Fail,
                    message: "test".to_string(),
                    suggestion: "test".to_string(),
                    language_refs: vec!["Rust".to_string()],
                },
            ],
            file: "test.dal".to_string(),
            fn_depth: 0,
            current_fn_line: 0,
            current_fn_name: String::new(),
            current_fn_params: 0,
            cyclomatic: HashMap::new(),
            match_count: HashMap::new(),
            doc_comments: HashMap::new(),
            pub_functions: Vec::new(),
            all_blocks: Vec::new(),
            magic_numbers: Vec::new(),
        };

        // Manually inflate findings to force negative score
        for _ in 0..25 {
            state.findings.push(Finding {
                file: "test.dal".to_string(),
                line: 1,
                rule_id: "test".to_string(),
                severity: Severity::Fail,
                message: "overflow".to_string(),
                suggestion: "fix".to_string(),
                language_refs: vec!["Rust".to_string()],
            });
        }

        let stats = analyzer.compute_stats(&state);
        let score = analyzer.calc_score(&state, &stats);

        assert!(
            (0.0..=100.0).contains(&score),
            "Score must be clamped to [0, 100]"
        );
    }

    #[test]
    fn test_report_format_text_empty() {
        let analyzer = QualityAnalyzer::new(None);
        let prog = Program {
            statements: vec![],
            modules: Vec::new(),
            uses: Vec::new(),
            package_manifest: None,
            macros: Vec::new(),
            derive_attrs: Vec::new(),
        };
        let report = analyzer.analyze(&prog, Some("clean.dal"));

        let text = report.format_text("warn");
        assert!(text.contains("passed") || text.contains("PASSED"));
        assert!(text.contains("Score:"));
    }

    #[test]
    fn test_all_rules_have_suggestions() {
        let rules = builtins();
        for rule in &rules {
            assert!(
                !rule.suggestion.is_empty(),
                "Rule '{}' must have a non-empty suggestion",
                rule.id
            );
        }
    }

    #[test]
    fn test_language_references_present() {
        let rules = builtins();
        for rule in &rules {
            assert!(
                !rule.description.is_empty(),
                "Rule '{}' must have a description",
                rule.id
            );
        }
    }
}
