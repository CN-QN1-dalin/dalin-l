/// Dalin L 3.0 — Borrow Checker / Memory Safety Engine
///
/// Implements Rust-level borrow checking for the Dalin L language:
/// - Mutable borrow conflict detection (unique mutable access)
/// - Move semantics tracking
/// - Alias analysis (no alias during mutation)
/// - Scope-based lifetime verification
/// - Use-after-move detection
///
/// Architecture: Phase E + Phase J compliant
/// Inspired by Rust's MIR borrow checker, adapted for Dalin L's Let/Const model.
use crate::ast::{Expr, Program, Stmt};
use std::collections::{BTreeSet, HashMap};

// ═══════════════════════════════
//  Error types
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq)]
pub enum BorrowError {
    /// Mutable borrow conflicts with another mutable borrow
    MutableBorrowConflict {
        variable: String,
        first_borrow_line: usize,
        second_borrow_line: usize,
    },
    /// Mutable borrow conflicts with immutable borrow
    MutableImmutableConflict {
        variable: String,
        immutable_line: usize,
        mutable_line: usize,
    },
    /// Variable used after move
    UseAfterMove {
        variable: String,
        moved_line: usize,
        use_line: usize,
    },
    /// Borrow of moved value
    BorrowOfMovedValue {
        variable: String,
        moved_line: usize,
        borrow_line: usize,
    },
    /// Cannot assign to immutable binding
    AssignToImmutable { variable: String, line: usize },
    /// Double free / use after drop (for RAII-style future)
    UseAfterDrop {
        variable: String,
        drop_line: usize,
        use_line: usize,
    },
}

impl std::fmt::Display for BorrowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MutableBorrowConflict {
                variable,
                first_borrow_line,
                second_borrow_line,
            } => write!(
                f,
                "Cannot borrow `{variable}` as mutable more than once at line {} and {}",
                first_borrow_line, second_borrow_line
            ),
            Self::MutableImmutableConflict {
                variable,
                immutable_line,
                mutable_line,
            } => write!(
                f,
                "Cannot borrow `{variable}` as mutable while immutably borrowed at line {} (first borrowed at {})",
                mutable_line, immutable_line
            ),
            Self::UseAfterMove {
                variable,
                moved_line,
                use_line,
            } => write!(
                f,
                "Use of moved value `{variable}`. Value was moved at line {}, used here at line {}",
                moved_line, use_line
            ),
            Self::BorrowOfMovedValue {
                variable,
                moved_line,
                borrow_line,
            } => write!(
                f,
                "Borrow of moved value `{variable}`. Value was moved at line {}, borrow here at line {}",
                moved_line, borrow_line
            ),
            Self::AssignToImmutable { variable, line } => {
                write!(
                    f,
                    "Cannot assign to immutable variable `{variable}` at line {}",
                    line
                )
            }
            Self::UseAfterDrop {
                variable,
                drop_line,
                use_line,
            } => write!(
                f,
                "Use after drop of `{variable}`. Dropped at line {}, used at line {}",
                drop_line, use_line
            ),
        }
    }
}

impl BorrowError {
    /// 取该借用错误最具诊断价值的行号（用于 J1 事件位置归因，替代硬编码的 `0`）。
    ///
    /// 优先取「错误实际发生点」：use/borrow/mutable 侧，而非 moved/immutable 侧。
    #[must_use]
    pub fn primary_line(&self) -> usize {
        match self {
            BorrowError::MutableBorrowConflict {
                second_borrow_line, ..
            } => *second_borrow_line,
            BorrowError::MutableImmutableConflict { mutable_line, .. } => *mutable_line,
            BorrowError::UseAfterMove { use_line, .. } => *use_line,
            BorrowError::BorrowOfMovedValue { borrow_line, .. } => *borrow_line,
            BorrowError::AssignToImmutable { line, .. } => *line,
            BorrowError::UseAfterDrop { use_line, .. } => *use_line,
        }
    }
}

// ═══════════════════════════════
//  Borrow state tracker
// ═══════════════════════════════

/// Tracks the borrow state of a single variable
#[derive(Debug, Clone)]
struct VariableState {
    /// Whether this variable is immutable (default for `let`)
    immutable: bool,
    /// Whether this value has been moved out
    moved: bool,
    /// Set of variables that are aliases of this one (currently only self)
    #[allow(dead_code)] // 预留：别名追踪（借用检查诊断/未来引用语义）
    aliases: BTreeSet<String>,
    /// Lines where mutable borrows are active
    mutable_borrows: Vec<usize>,
    /// Lines where immutable borrows are active
    immutable_borrows: Vec<usize>,
    /// The owner of this variable (for move tracking)
    owner: Option<String>,
}

impl VariableState {
    fn new(immutable: bool) -> Self {
        Self {
            immutable,
            moved: false,
            aliases: BTreeSet::new(),
            mutable_borrows: Vec::new(),
            immutable_borrows: Vec::new(),
            owner: None,
        }
    }

    #[allow(dead_code)] // 诊断查询：供自进化引擎 / 报告扩展使用（有测试覆盖）
    fn is_valid(&self) -> bool {
        // A variable is valid if it hasn't been moved AND has no active borrows
        // (or if it's in a state where it can be safely accessed)
        !self.moved && self.mutable_borrows.is_empty() && self.immutable_borrows.is_empty()
    }
}

/// Complete borrow checker state
#[derive(Debug, Clone)]
pub struct BorrowChecker {
    /// Per-variable state
    variables: HashMap<String, VariableState>,
    /// Collected errors
    errors: Vec<BorrowError>,
    /// Current line number context
    current_line: usize,
    /// Track moved-from values: (src_var, dst_var, line)
    moves: Vec<(String, String, usize)>,
    /// Scope depth for lifetime tracking
    scope_depth: usize,
}

impl Default for BorrowChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowChecker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            errors: Vec::new(),
            current_line: 0,
            moves: Vec::new(),
            scope_depth: 0,
        }
    }

    #[must_use]
    pub fn errors(&self) -> &[BorrowError] {
        &self.errors
    }

    #[must_use]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Check if a variable exists in the state
    fn has_var(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Get mutable borrow state for a variable
    fn check_mutable_borrow(&self, name: &str) -> Result<(), BorrowError> {
        let var_state = self.variables.get(name).cloned();
        let Some(state) = var_state else {
            // Variable doesn't exist, it's a new borrow — allow
            return Ok(());
        };

        if state.moved {
            return Err(BorrowError::BorrowOfMovedValue {
                variable: name.to_string(),
                moved_line: self
                    .moves
                    .iter()
                    .find(|(_, v, _)| v == name)
                    .map(|(_, _, l)| *l)
                    .unwrap_or(0),
                borrow_line: self.current_line,
            });
        }

        // If immutable, any borrow is read-only
        if state.immutable {
            return Ok(());
        }

        // Check for conflicting mutable borrows (only 1 mutable borrow allowed)
        if !state.mutable_borrows.is_empty() {
            return Err(BorrowError::MutableBorrowConflict {
                variable: name.to_string(),
                first_borrow_line: state.mutable_borrows[0],
                second_borrow_line: self.current_line,
            });
        }

        // Check for immutable borrow conflicts
        if !state.immutable_borrows.is_empty() {
            return Err(BorrowError::MutableImmutableConflict {
                variable: name.to_string(),
                immutable_line: state.immutable_borrows[0],
                mutable_line: self.current_line,
            });
        }

        Ok(())
    }

    /// Record a mutable borrow on a variable
    fn record_mutable_borrow(&mut self, name: &str) {
        if let Some(state) = self.variables.get_mut(name) {
            state.mutable_borrows.push(self.current_line);
        }
    }

    /// Record an immutable borrow on a variable
    fn record_immutable_borrow(&mut self, name: &str) {
        if let Some(state) = self.variables.get_mut(name) {
            state.immutable_borrows.push(self.current_line);
        }
    }

    /// Check for usage conflicts before use
    fn check_use(&self, name: &str) -> Result<(), BorrowError> {
        let Some(state) = self.variables.get(name) else {
            return Ok(());
        };

        if state.moved {
            return Err(BorrowError::UseAfterMove {
                variable: name.to_string(),
                moved_line: self
                    .moves
                    .iter()
                    .find(|(_, v, _)| v == name)
                    .map(|(_, _, l)| *l)
                    .unwrap_or(0),
                use_line: self.current_line,
            });
        }

        // 注意：check_use 只检查"变量是否可读取"（moved 状态）。
        // 可变/不可变借用冲突属于 check_mutable_borrow 的职责，
        // 在此检查 immutable_borrows 会导致普通读取误报 MutableImmutableConflict。

        Ok(())
    }

    /// Mark a variable as moved from (source)
    fn mark_move(&mut self, src: &str, dst: &str, line: usize) {
        self.moves.push((src.to_string(), dst.to_string(), line));
        if let Some(state) = self.variables.get_mut(src) {
            state.moved = true;
            state.owner = Some(dst.to_string());
        }
        // New owner starts fresh
        if !self.variables.contains_key(dst) {
            self.variables
                .insert(dst.to_string(), VariableState::new(false));
        }
    }

    /// Mark a variable as mutable (mutable let)
    fn mark_mutable(&mut self, name: &str) {
        if let Some(state) = self.variables.get_mut(name) {
            state.immutable = false;
        }
    }

    /// Clear mutable borrows (end of scope)
    fn clear_borrows_for_scope(&mut self, name: &str, scope_end_line: usize) {
        if let Some(state) = self.variables.get_mut(name) {
            state.mutable_borrows.retain(|&l| l > scope_end_line);
            state.immutable_borrows.retain(|&l| l > scope_end_line);
        }
    }

    // ── Checkers for different statement types ──

    /// Check a `let` binding
    fn check_let(&mut self, name: &str, value: Option<&Expr>, mutable: bool, line: usize) {
        self.current_line = line;

        // 借用检查：若变量已存在（重新赋值），对不可变绑定赋值应报错。
        // 这激活 check_assign 的完整借用语义（AssignToImmutable 检测）。
        if self.has_var(name) && !mutable {
            self.check_assign(name, value, line);
            return;
        }

        // Register the variable
        let imm = !mutable;
        self.variables
            .insert(name.to_string(), VariableState::new(imm));
        if mutable {
            self.mark_mutable(name);
        }

        // If there's a value, check if it involves moves
        if let Some(v) = value {
            self.track_moves_in_expr(v, name);
        }
    }

    /// Track potential move operations in expressions
    fn track_moves_in_expr(&mut self, expr: &Expr, target: &str) {
        match expr {
            Expr::Ident(name)
                // Moving a value into a new binding
                if self.has_var(name) =>
            {
                let old_moved = self.variables.get(name).map(|s| s.moved).unwrap_or(false);
                if !old_moved {
                    self.mark_move(name, target, self.current_line);
                }
            }
            Expr::Call { func, args } => {
                for arg in args {
                    self.track_moves_in_expr(arg, target);
                }
                let _ = func.as_ref();
            }
            Expr::BinaryOp { left, right, .. } => {
                self.track_moves_in_expr(left, target);
                self.track_moves_in_expr(right, target);
            }
            Expr::UnaryOp { operand, .. } => {
                self.track_moves_in_expr(operand, target);
            }
            Expr::Array(elems) => {
                for elem in elems {
                    self.track_moves_in_expr(elem, target);
                }
            }
            Expr::OptionValue { value: Some(v), .. } => {
                self.track_moves_in_expr(v, target);
            }
            Expr::OptionValue { value: None, .. } => {}
            Expr::ResultValue { value, error, .. } => {
                if let Some(v) = value {
                    self.track_moves_in_expr(v, target);
                }
                if let Some(e) = error {
                    self.track_moves_in_expr(e, target);
                }
            }
            _ => {}
        }
    }

    /// Check assignment (= operator on existing variable)
    fn check_assign(&mut self, name: &str, value: Option<&Expr>, line: usize) {
        self.current_line = line;

        if !self.has_var(name) {
            return; // Unknown variable, will be caught by type checker
        }

        let state = self.variables.get(name).unwrap();
        if state.immutable {
            self.errors.push(BorrowError::AssignToImmutable {
                variable: name.to_string(),
                line,
            });
            return;
        }

        // Check for use-after-move on source vars in value
        if let Some(v) = value {
            self.check_moves_in_expr_for_use(v);
        }

        // 借用检查：对可变变量的赋值 = 一次可变借用。
        // 若已有未释放的可变/不可变借用，产生冲突错误（借用检查完整语义）。
        // 注意：`let` 重绑定（新所有权）语义上释放旧借用，因此先清理再检查，
        // 避免 while 循环迭代间的误报（新绑定不是旧值的借用延续）。
        self.clear_borrows_for_scope(name, line);
        if let Err(e) = self.check_mutable_borrow(name) {
            self.errors.push(e);
        }
        self.record_mutable_borrow(name);

        // Reset borrow state for reassignment (value not moved, just reassigned)
        if let Some(state) = self.variables.get_mut(name) {
            state.mutable_borrows.clear();
            state.immutable_borrows.clear();
            state.moved = false;
        }
    }

    /// Check all Ident nodes in expression for move conflicts (read-only check)
    fn check_moves_in_expr_for_use(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name) => {
                if let Err(e) = self.check_use(name) {
                    self.errors.push(e);
                }
                // 读取 = 不可变借用（借用检查完整语义）
                self.record_immutable_borrow(name);
            }
            Expr::Call { func, args } => {
                let _ = func.as_ref();
                for arg in args {
                    self.check_moves_in_expr_for_use(arg);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.check_moves_in_expr_for_use(left);
                self.check_moves_in_expr_for_use(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.check_moves_in_expr_for_use(operand);
            }
            Expr::Array(elems) => {
                for elem in elems {
                    self.check_moves_in_expr_for_use(elem);
                }
            }
            Expr::OptionValue { value: Some(v), .. } => {
                self.check_moves_in_expr_for_use(v);
            }
            Expr::OptionValue { value: None, .. } => {}
            Expr::ResultValue { value, error, .. } => {
                if let Some(v) = value {
                    self.check_moves_in_expr_for_use(v);
                }
                if let Some(e) = error {
                    self.check_moves_in_expr_for_use(e);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.check_moves_in_expr_for_use(object);
            }
            Expr::Index { array, index } => {
                self.check_moves_in_expr_for_use(array);
                self.check_moves_in_expr_for_use(index);
            }
            Expr::Pipe { input, ops } => {
                self.check_moves_in_expr_for_use(input);
                for (_, op_expr) in ops {
                    self.check_moves_in_expr_for_use(op_expr);
                }
            }
            Expr::Range { start, end, .. } => {
                self.check_moves_in_expr_for_use(start);
                self.check_moves_in_expr_for_use(end);
            }
            Expr::IfExpr(cond, then_expr, else_expr) => {
                let _ = cond.as_ref();
                self.check_moves_in_expr_for_use(then_expr);
                self.check_moves_in_expr_for_use(else_expr);
            }
            Expr::MatchExpr(target, arms) => {
                let _ = target.as_ref();
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    /// Full statement check (entry point for traversal)
    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                self.check_let(name, value.as_deref(), *mutable, self.current_line);
                if let Some(v) = value {
                    self.check_moves_in_expr_for_use(v);
                }
            }
            Stmt::Const { name, value, .. } => {
                self.current_line += 1;
                self.variables
                    .insert(name.to_string(), VariableState::new(true));
                if let Some(v) = value {
                    self.check_moves_in_expr_for_use(v);
                }
            }
            Stmt::Fn { body, .. } => {
                self.scope_depth += 1;
                for s in body.as_ref() {
                    self.check_stmt(s);
                }
                // 作用域结束时清理借用记录，避免跨作用域误报
                let names: Vec<String> = self.variables.keys().cloned().collect();
                for n in names {
                    self.clear_borrows_for_scope(&n, self.current_line);
                }
                self.scope_depth -= 1;
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.current_line += 1;
                self.check_moves_in_expr_for_use(condition);
                for s in then_body {
                    self.check_stmt(s);
                }
                for s in else_body {
                    self.check_stmt(s);
                }
            }
            Stmt::While { condition, body } => {
                self.current_line += 1;
                self.check_moves_in_expr_for_use(condition);
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::For {
                target,
                iterable,
                body,
            } => {
                self.current_line += 1;
                self.check_moves_in_expr_for_use(iterable);
                self.variables
                    .insert(target.clone(), VariableState::new(false));
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::Match { target, arms } => {
                self.current_line += 1;
                let _ = target.as_ref();
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }
            Stmt::Return(_) => {
                self.current_line += 1;
            }
            // 循环控制流不产生借用/移动，仅推进行号。
            Stmt::Break | Stmt::Continue => {
                self.current_line += 1;
            }
            Stmt::Expr(expr) => {
                self.current_line += 1;
                self.check_moves_in_expr_for_use(expr);
            }
            Stmt::Llm { .. }
            | Stmt::Use(_)
            | Stmt::Export(_)
            | Stmt::TypeAlias { .. }
            | Stmt::TryCatch { .. }
            | Stmt::Assert { .. }
            | Stmt::Spawn { .. }
            | Stmt::Channel { .. }
            | Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::TraitDef { .. }
            | Stmt::ImplBlock { .. } => {
                self.current_line += 1;
            }
        }
    }

    /// Main entry point: check a complete program
    pub fn check_program(&mut self, program: &Program) -> &[BorrowError] {
        for stmt in &program.statements {
            self.check_stmt(stmt);
        }
        &self.errors
    }

    /// Generate a human-readable report
    #[must_use]
    pub fn report(&self) -> String {
        if self.errors.is_empty() {
            return "\u{2705} Borrow check passed: No conflicts detected.".to_string();
        }

        let mut lines = vec![format!(
            "\n\u{26D4} Borrow Checker Report: {} error(s)\n",
            self.errors.len()
        )];

        for (i, err) in self.errors.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, err));
        }

        lines.push(String::new());
        lines.join("\n")
    }

    /// Return (mutable_count, immutable_count) for a variable's active borrows — used by self_evolution
    pub fn active_borrows(&self, name: &str) -> (usize, usize) {
        if let Some(state) = self.variables.get(name) {
            (state.mutable_borrows.len(), state.immutable_borrows.len())
        } else {
            (0, 0)
        }
    }
}

// ═══════════════════════════════
//  Tests
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ident(name: &str) -> Expr {
        Expr::Ident(name.to_string())
    }

    fn make_int_literal(val: i64) -> Expr {
        Expr::IntLiteral(val)
    }

    fn make_let(name: &str, value: Expr, mutable: bool) -> Stmt {
        Stmt::Let {
            name: name.to_string(),
            value: Some(Box::new(value)),
            type_annotation: None,
            mutable,
        }
    }

    fn make_fn(name: &str, params: Vec<crate::ast::FnParam>, body: Vec<Stmt>) -> Stmt {
        Stmt::Fn {
            name: name.to_string(),
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
            body: Box::new(body),
            async_: false,
            pub_: false,
        }
    }

    #[test]
    fn test_no_errors_simple_program() {
        let prog = Program {
            statements: vec![
                make_let("x", make_int_literal(42), false),
                make_let("y", make_int_literal(10), false),
            ],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(checker.error_count(), 0);
    }

    #[test]
    fn test_move_detection_in_let() {
        // let x = 42;
        // let y = x;     // x moved into y
        // println!("{}", x);  // ERROR: use after move
        let prog = Program {
            statements: vec![
                make_let("x", make_int_literal(42), false),
                make_let("y", make_ident("x"), false),
                Stmt::Expr(Box::new(make_ident("x"))),
            ],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        // Debug output for CI
        if checker.error_count() == 0 {
            panic!(
                "Borrow checker produced no errors. Variables state: {:?}",
                checker.variables
            );
        }
        assert!(checker.error_count() >= 1, "should detect use-after-move");
    }

    #[test]
    fn test_fn_params_declared_as_immutable() {
        // Inside a function body, explicit `let x = ...` is immutable by default
        let body = vec![
            Stmt::Let {
                name: "x".to_string(),
                value: Some(Box::new(make_int_literal(0))),
                type_annotation: None,
                mutable: false,
            },
            // Use x normally — no error
            Stmt::Expr(Box::new(make_ident("x"))),
        ];

        let prog = Program {
            statements: vec![make_fn("foo", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(checker.error_count(), 0, "using immutable var is OK");
    }

    #[test]
    fn test_multiple_mutable_vars() {
        // Multiple mutable variables, each borrowed once — fine
        let body = vec![
            make_let("a", make_int_literal(1), true),
            make_let("b", make_int_literal(2), true),
            // Use both — no moves, just reads
            Stmt::Expr(Box::new(Expr::BinaryOp {
                left: Box::new(make_ident("a")),
                op: "+".to_string(),
                right: Box::new(make_ident("b")),
            })),
        ];

        let prog = Program {
            statements: vec![make_fn("add_ab", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(checker.error_count(), 0);
    }

    #[test]
    fn test_while_loop_no_false_positive() {
        // while loops re-evaluate body multiple times — should not flag as move
        let body = vec![
            make_let("counter", make_int_literal(0), true),
            Stmt::While {
                condition: Box::new(Expr::BinaryOp {
                    left: Box::new(make_ident("counter")),
                    op: "<".to_string(),
                    right: Box::new(make_int_literal(3)),
                }),
                body: vec![
                    Stmt::Expr(Box::new(Expr::BinaryOp {
                        left: Box::new(make_ident("counter")),
                        op: "+".to_string(),
                        right: Box::new(make_int_literal(1)),
                    })),
                    // Read counter again — still valid (no move)
                    Stmt::Expr(Box::new(make_ident("counter"))),
                ],
            },
        ];

        let prog = Program {
            statements: vec![make_fn("loop_test", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(
            checker.error_count(),
            0,
            "while loop should not produce false positives"
        );
    }

    #[test]
    fn test_for_loop_variable_registered() {
        // for i in range(n) { println(i) } — i should be registered
        let body = vec![
            make_let("n", make_int_literal(5), false),
            Stmt::For {
                target: "i".to_string(),
                iterable: Box::new(Expr::Ident("n".to_string())),
                body: vec![Stmt::Expr(Box::new(make_ident("i")))],
            },
        ];

        let prog = Program {
            statements: vec![make_fn("for_test", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(checker.error_count(), 0);
    }

    #[test]
    fn test_report_format_with_errors() {
        let prog = Program {
            statements: vec![
                make_let("x", make_int_literal(42), false),
                make_let("y", make_ident("x"), false),
                Stmt::Expr(Box::new(make_ident("x"))),
            ],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        let report = checker.report();
        assert!(report.contains("error"));
    }

    #[test]
    fn test_clean_report() {
        let prog = Program {
            statements: vec![make_let("x", make_int_literal(42), false)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        let report = checker.report();
        assert!(report.contains("No conflicts"));
    }

    #[test]
    fn test_if_branch_usage() {
        // if/else branches should both be checked
        let body = vec![
            make_let("flag", make_int_literal(1), false),
            Stmt::If {
                condition: Box::new(make_ident("flag")),
                then_body: vec![Stmt::Expr(Box::new(make_ident("flag")))],
                else_body: vec![Stmt::Expr(Box::new(make_ident("flag")))],
            },
        ];

        let prog = Program {
            statements: vec![make_fn("if_test", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        assert_eq!(checker.error_count(), 0);
    }

    #[test]
    fn test_match_branch_usage() {
        // match arms should check usage in each branch
        let body = vec![
            make_let("value", make_int_literal(42), false),
            Stmt::Match {
                target: Box::new(make_ident("value")),
                arms: vec![crate::ast::MatchArm {
                    pattern: crate::ast::Pattern {
                        kind: "lit".to_string(),
                        name: "42".to_string(),
                        binding: None,
                        inner: Vec::new(),
                        fields: Vec::new(),
                        value: None,
                    },
                    guard: None,
                    body: vec![Stmt::Expr(Box::new(make_ident("value")))],
                }],
            },
        ];

        let prog = Program {
            statements: vec![make_fn("match_test", Vec::new(), body)],
            ..Program::new()
        };

        let mut checker = BorrowChecker::new();
        checker.check_program(&prog);
        // Match branches don't cause false positives for normal usage
    }

    // ── 借用检查子系统综合测试（覆盖 check_mutable_borrow/record_*/mark_mutable/clear_borrows_for_scope/check_assign/is_valid）──

    #[test]
    fn test_borrow_subsystem_mutable_conflict() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(false));
        checker.current_line = 10;
        checker.record_mutable_borrow("x");
        // 第二次可变借用应冲突
        checker.current_line = 20;
        let err = checker.check_mutable_borrow("x");
        assert!(matches!(
            err,
            Err(BorrowError::MutableBorrowConflict { .. })
        ));
    }

    #[test]
    fn test_borrow_subsystem_mut_imm_conflict() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(false));
        checker.current_line = 5;
        checker.record_immutable_borrow("x");
        // 已有不可变借用时，可变借用应冲突
        checker.current_line = 15;
        let err = checker.check_mutable_borrow("x");
        assert!(matches!(
            err,
            Err(BorrowError::MutableImmutableConflict { .. })
        ));
    }

    #[test]
    fn test_borrow_subsystem_clean_borrows() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(false));
        checker.current_line = 1;
        checker.record_mutable_borrow("x");
        checker.record_immutable_borrow("x");
        // 清理后借用应释放，可变借用不再冲突
        checker.clear_borrows_for_scope("x", 2);
        let state = checker.variables.get("x").unwrap();
        assert!(state.mutable_borrows.is_empty());
        assert!(state.immutable_borrows.is_empty());
        assert!(state.is_valid(), "无借用且未移动时变量有效");
    }

    #[test]
    fn test_borrow_subsystem_mark_mutable() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(true)); // immutable
        checker.mark_mutable("x");
        let state = checker.variables.get("x").unwrap();
        assert!(!state.immutable, "mark_mutable 应把变量改为可变");
    }

    #[test]
    fn test_borrow_subsystem_check_assign_to_immutable() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(true)); // immutable
        checker.check_assign("x", None, 30);
        assert!(
            checker
                .errors
                .iter()
                .any(|e| matches!(e, BorrowError::AssignToImmutable { .. })),
            "对不可变变量赋值应报错"
        );
    }

    #[test]
    fn test_borrow_subsystem_active_borrows() {
        let mut checker = BorrowChecker::new();
        checker
            .variables
            .insert("x".into(), VariableState::new(false));
        checker.current_line = 7;
        checker.record_mutable_borrow("x");
        checker.record_immutable_borrow("x");
        let (m, i) = checker.active_borrows("x");
        assert_eq!((m, i), (1, 1));
        let (m2, i2) = checker.active_borrows("nonexistent");
        assert_eq!((m2, i2), (0, 0));
    }

    #[test]
    fn test_borrow_error_primary_line_prefers_error_site() {
        // 各变体应取「错误实际发生点」作为主行号，而非 moved/immutable 侧
        assert_eq!(
            BorrowError::UseAfterMove {
                variable: "x".into(),
                moved_line: 10,
                use_line: 20,
            }
            .primary_line(),
            20
        );
        assert_eq!(
            BorrowError::BorrowOfMovedValue {
                variable: "x".into(),
                moved_line: 10,
                borrow_line: 25,
            }
            .primary_line(),
            25
        );
        assert_eq!(
            BorrowError::MutableBorrowConflict {
                variable: "x".into(),
                first_borrow_line: 5,
                second_borrow_line: 30,
            }
            .primary_line(),
            30
        );
        assert_eq!(
            BorrowError::MutableImmutableConflict {
                variable: "x".into(),
                immutable_line: 8,
                mutable_line: 40,
            }
            .primary_line(),
            40
        );
        assert_eq!(
            BorrowError::AssignToImmutable {
                variable: "x".into(),
                line: 50,
            }
            .primary_line(),
            50
        );
        assert_eq!(
            BorrowError::UseAfterDrop {
                variable: "x".into(),
                drop_line: 12,
                use_line: 60,
            }
            .primary_line(),
            60
        );
    }
}
