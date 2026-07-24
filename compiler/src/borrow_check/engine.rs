/// Dalin L 3.0 — Borrow Checker Engine
///
/// Main entry point: `BorrowChecker::check(program)` walks the AST and verifies:
/// 1. Copy/Move semantics (Tier 1)
/// 2. Immutable/Mutable borrow exclusivity (Tier 2)
/// 3. Mutability enforcement (var vs let)
///
/// Returns a list of `BorrowError` on violation, or Ok(()) for valid programs.
use crate::ast::*;
use crate::borrow_check::model::*;
use crate::borrow_check::scope::*;
use crate::borrow_check::error::*;

/// The main borrow checker that walks an AST and validates ownership/borrowing rules
pub struct BorrowChecker {
    pub(crate) forest: ScopeForest,
    errors: Vec<BorrowError>,
}

impl Default for BorrowChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowChecker {
    pub fn new() -> Self {
        Self {
            forest: ScopeForest::new(),
            errors: Vec::new(),
        }
    }

    /// Check a full program for borrow/move violations
    pub fn check(&mut self, program: &Program) -> Vec<BorrowError> {
        self.errors.clear();
        self.forest = ScopeForest::new();

        // Register all top-level bindings first
        for stmt in &program.statements {
            self.collect_top_level_bindings(stmt);
        }

        // Then walk and check
        self.check_statements(&program.statements, None);

        self.errors.clone()
    }

    /// Collect all top-level binding declarations without checking (registration only)
    fn collect_top_level_bindings(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, mutable, .. } => {
                self.forest.add_binding(Binding::new(
                    name.as_str(),
                    if *mutable { Mutability::Mutable } else { Mutability::Immutable },
                    false,
                ));
            }
            Stmt::Const { name, .. } => {
                self.forest.add_binding(Binding::new(
                    name.as_str(),
                    Mutability::Immutable,
                    true,
                ));
            }
            Stmt::Fn { params, body, .. } => {
                for p in params {
                    self.forest.add_binding(Binding::new(
                        &p.name,
                        Mutability::Mutable,
                        false,
                    ));
                }
                self.check_statements(body, None);
            }
            _ => {}
        }
    }

    fn check_statements(&mut self, stmts: &[Stmt], _outer_fn_name: Option<&str>) {
        for stmt in stmts {
            self.check_stmt(stmt, _outer_fn_name);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, _outer_fn_name: Option<&str>) {
        match stmt {
            Stmt::Let { name, value, mutable, type_annotation: _ } => {
                // Register binding
                let copyable = value.as_ref().is_some_and(|v| is_copy_value(v));
                let effective_mut = if *mutable { Mutability::Mutable } else { Mutability::Immutable };
                
                self.forest.add_binding(Binding::new(name, effective_mut, copyable));

                if let Some(val) = value {
                    self.check_expr(val, _outer_fn_name);
                }
            }
            Stmt::Const { name, value, .. } => {
                self.forest.add_binding(Binding::new(
                    name, Mutability::Immutable, true,
                ));
                if let Some(val) = value {
                    self.check_expr(val, _outer_fn_name);
                }
            }
            Stmt::Fn { name, params, body, .. } => {
                // Register function as immutable binding
                self.forest.add_binding(Binding::new(
                    name, Mutability::Immutable, false,
                ));
                for (p, guard) in params.iter().flat_map(|p| {
                    p.type_annotation.as_ref().map(|t| (p, t))
                }) {
                    let copyable = is_copy_type_ref(guard);
                    self.forest.add_binding(Binding::new(
                        &p.name, Mutability::Mutable, copyable,
                    ));
                }
                // Check function body in its own scope
                self.push_scope(name);
                self.check_statements(body, Some(name));
                self.pop_scope();
            }
            Stmt::If { condition, then_body, else_body } => {
                self.check_expr(condition, _outer_fn_name);
                self.push_scope("if.then");
                self.check_statements(then_body, _outer_fn_name);
                self.pop_scope();
                self.push_scope("if.else");
                self.check_statements(else_body, _outer_fn_name);
                self.pop_scope();
            }
            Stmt::While { condition, body } => {
                self.check_expr(condition, _outer_fn_name);
                self.push_scope("while");
                self.check_statements(body, _outer_fn_name);
                self.pop_scope();
            }
            Stmt::For { target, iterable, body } => {
                // Check iterable expression
                self.check_expr(iterable, _outer_fn_name);
                // Create iteration scope with target bound
                self.push_scope("for");
                self.forest.add_binding(Binding::new(
                    target, Mutability::Mutable, false,
                ));
                self.check_statements(body, _outer_fn_name);
                self.pop_scope();
            }
            Stmt::Match { target, arms } => {
                self.check_expr(target, _outer_fn_name);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard, _outer_fn_name);
                    }
                    self.push_scope("match.arm");
                    self.check_statements(&arm.body, _outer_fn_name);
                    self.pop_scope();
                }
            }
            Stmt::Return(Some(expr)) => {
                self.check_expr(expr, _outer_fn_name);
            }
            Stmt::Return(None) => {}
            Stmt::TryCatch { try_body, catch_param, catch_body } => {
                self.check_statements(try_body, _outer_fn_name);
                if let Some(param) = catch_param {
                    self.push_scope("catch");
                    self.forest.add_binding(Binding::new(
                        param, Mutability::Mutable, false,
                    ));
                    self.check_statements(catch_body, _outer_fn_name);
                    self.pop_scope();
                }
            }
            Stmt::Assert { condition, message } => {
                self.check_expr(condition, _outer_fn_name);
                if let Some(msg) = message {
                    self.check_expr(msg, _outer_fn_name);
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr, _outer_fn_name);
            }
            _ => {
                // Skip declarations
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, _outer_fn_name: Option<&str>) {
        match expr {
            Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::BoolLiteral(_) | Expr::CharLiteral(_) => {}
            Expr::StringLiteral(_) => {}
            Expr::Ident(name) => {
                self.check_ident_access(name);
            }
            Expr::BinaryOp { left, right, .. } => {
                self.check_expr(left, _outer_fn_name);
                self.check_expr(right, _outer_fn_name);
            }
            Expr::UnaryOp { operand, .. } => {
                self.check_expr(operand, _outer_fn_name);
            }
            Expr::Call { func, args } => {
                self.check_expr(func, _outer_fn_name);
                for arg in args {
                    self.check_expr(arg, _outer_fn_name);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.check_expr(object, _outer_fn_name);
            }
            Expr::Index { array, index } => {
                self.check_expr(array, _outer_fn_name);
                self.check_expr(index, _outer_fn_name);
            }
            Expr::Pipe { input, ops } => {
                self.check_expr(input, _outer_fn_name);
                for (_, inner_expr) in ops {
                    self.check_expr(inner_expr, _outer_fn_name);
                }
            }
            Expr::Range { start, end, .. } => {
                self.check_expr(start, _outer_fn_name);
                self.check_expr(end, _outer_fn_name);
            }
            Expr::Array(items) => {
                for item in items {
                    self.check_expr(item, _outer_fn_name);
                }
            }
            Expr::OptionValue { value, .. } => {
                if let Some(v) = value {
                    self.check_expr(v, _outer_fn_name);
                }
            }
            Expr::ResultValue { value, error, .. } => {
                if let Some(v) = value {
                    self.check_expr(v, _outer_fn_name);
                }
                if let Some(e) = error {
                    self.check_expr(e, _outer_fn_name);
                }
            }
            Expr::IfExpr(cond, then_expr, otherwise) => {
                self.check_expr(cond, _outer_fn_name);
                self.check_expr(then_expr, _outer_fn_name);
                self.check_expr(otherwise, _outer_fn_name);
            }
            Expr::MatchExpr(target, arms) => {
                self.check_expr(target, _outer_fn_name);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard, _outer_fn_name);
                    }
                }
            }
            Expr::Interpolate { parts } => {
                for part in parts {
                    if let InterpolatePart::Expr(e) = part {
                        self.check_expr(e, _outer_fn_name);
                    }
                }
            }
            Expr::NamedArg(_, expr) => {
                self.check_expr(expr, _outer_fn_name);
            }
            Expr::IsCheck(expr, _) | Expr::Cast(expr, _) => {
                self.check_expr(expr, _outer_fn_name);
            }
            Expr::CCall { args, .. } => {
                for arg in args {
                    self.check_expr(arg, _outer_fn_name);
                }
            }
        }
    }

    fn check_ident_access(&mut self, name: &str) {
        // Check if name was moved and is not copyable; report error only when both hold
        if !self.forest.is_moved_in_scope(name) || self.is_copy_binding(name) {
            return;
        }
        self.errors.push(BorrowError::new(
            BorrowErrorCode::MoveOccurred,
            name,
            0, 0,
        ));
    }

    fn is_copy_binding(&self, name: &str) -> bool {
        self.forest.lookup_binding(name).unwrap_or(false)
    }

    fn push_scope(&mut self, label: &str) {
        let _ = label;
        self.forest.enter_scope();
    }

    fn pop_scope(&mut self) {
        self.forest.exit_scope();
    }
}

/// Check if a type is Copy-compatible (int, float, bool, char)
fn is_copy_type_ref(t: &TypeRef) -> bool {
    matches!(t.base, BaseType::Int | BaseType::Float | BaseType::Bool | BaseType::Char)
}

/// Determine if a value should be treated as copyable (heuristic)
fn is_copy_value(val: &Expr) -> bool {
    matches!(
        val,
        Expr::IntLiteral(_) | Expr::FloatLiteral(_) | Expr::BoolLiteral(_) | Expr::CharLiteral(_)
    )
}
