/// Dalin L 3.0 — Borrow Checker Tests
///
/// Tests cover: copy/move semantics, immutable/mutable exclusivity,
/// nested scopes, function parameter lifetimes

use crate::ast::*;
use super::BorrowChecker;
use crate::borrow_check::scope::*;
use crate::borrow_check::model::*;

fn int_literal(val: i64) -> Expr {
    Expr::IntLiteral(val)
}

fn ident(name: &str) -> Expr {
    Expr::Ident(name.to_string())
}

fn let_stmt(name: &str, value: Expr, mutable: bool) -> Stmt {
    Stmt::Let {
        name: name.to_string(),
        value: Some(Box::new(value)),
        type_annotation: None,
        mutable,
    }
}

fn expr_stmt(expr: Expr) -> Stmt {
    Stmt::Expr(Box::new(expr))
}

fn fn_stmt(name: &str) -> Stmt {
    Stmt::Fn {
        name: name.to_string(),
        type_params: Vec::new(),
        params: vec![crate::ast::FnParam {
            name: "x".to_string(),
            type_annotation: None,
            default: None,
        }],
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
        body: vec![
            let_stmt("y", int_literal(10), false),
            expr_stmt(ident("y")),
        ],
        async_: false,
        pub_: false,
    }
}

// ─── Tier 1: Copy/Move Semantics ──────────────────────────────

#[test]
fn test_copy_type_no_move() {
    let program = Program {
        statements: vec![
            let_stmt("x", int_literal(42), false),
            let_stmt("y", ident("x"), false),
            expr_stmt(ident("x")),
        ],
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    };

    let mut checker = BorrowChecker::new();
    let errors = checker.check(&program);
    assert!(errors.is_empty(), "Copy types should not prevent reuse: {:?}", errors);
}

#[test]
fn test_simple_let_binding() {
    let program = Program {
        statements: vec![
            let_stmt("x", int_literal(42), false),
            expr_stmt(ident("x")),
        ],
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    };

    let mut checker = BorrowChecker::new();
    let errors = checker.check(&program);
    assert!(errors.is_empty(), "Simple let binding should work: {:?}", errors);
}

// ─── Tier 2: Borrow Mutability ────────────────────────────────

#[test]
fn test_multiple_immutable_borrows_ok() {
    let mut checker = BorrowChecker::new();
    checker.forest.add_binding(Binding::new("data", Mutability::Immutable, false));
    
    let _id1 = checker.forest.add_immutable_borrow("r1", "data", 1).unwrap();
    let _id2 = checker.forest.add_immutable_borrow("r2", "data", 2).unwrap();
    
    let result = checker.forest.add_mutable_borrow("w1", "data", 3);
    assert!(result.is_err(), "Should reject mutable borrow while immutables exist");
}

#[test]
fn test_mutable_borrow_excludes_all() {
    let mut checker = BorrowChecker::new();
    checker.forest.add_binding(Binding::new("data", Mutability::Mutable, false));
    
    let _id1 = checker.forest.add_mutable_borrow("w1", "data", 1).unwrap();
    
    let result = checker.forest.add_immutable_borrow("r1", "data", 2);
    assert!(result.is_err(), "Should reject any borrow while mutable borrow exists");
}

// ─── Scope Management ─────────────────────────────────────────

#[test]
fn test_scope_enter_exit() {
    let mut forest = ScopeForest::new();
    assert_eq!(forest.current, 0);
    
    let inner = forest.enter_scope();
    assert_ne!(inner, 0);
    assert_ne!(forest.current, 0);
    
    forest.exit_scope();
    assert_eq!(forest.current, 0);
}

#[test]
fn test_bindings_scoped_to_function() {
    let program = Program {
        statements: vec![
            fn_stmt("test_fn"),
        ],
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    };

    let mut checker = BorrowChecker::new();
    let errors = checker.check(&program);
    assert!(errors.is_empty(), "Function-scoped bindings should work: {:?}", errors);
}

#[test]
fn test_nested_if_let() {
    let program = Program {
        statements: vec![
            let_stmt("x", int_literal(1), true),
            Stmt::If {
                condition: Box::new(Expr::BinaryOp {
                    left: Box::new(ident("x")),
                    op: ">=".to_string(),
                    right: Box::new(int_literal(0)),
                }),
                then_body: vec![
                    let_stmt("y", int_literal(42), false),
                    expr_stmt(ident("y")),
                ],
                else_body: vec![
                    let_stmt("z", int_literal(-1), false),
                    expr_stmt(ident("z")),
                ],
            },
        ],
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    };

    let mut checker = BorrowChecker::new();
    let errors = checker.check(&program);
    assert!(errors.is_empty(), "Nested if-let scopes should work: {:?}", errors);
}

#[test]
fn test_while_loop_scope() {
    let program = Program {
        statements: vec![
            let_stmt("i", int_literal(0), true),
            let_stmt("sum", int_literal(0), true),
            Stmt::While {
                condition: Box::new(Expr::BinaryOp {
                    left: Box::new(ident("i")),
                    op: "<".to_string(),
                    right: Box::new(int_literal(10)),
                }),
                body: vec![
                    let_stmt("_tmp", ident("sum"), false),
                    expr_stmt(ident("_tmp")),
                ],
            },
        ],
        modules: Vec::new(),
        uses: Vec::new(),
        package_manifest: None,
        macros: Vec::new(),
        derive_attrs: Vec::new(),
    };

    let mut checker = BorrowChecker::new();
    let errors = checker.check(&program);
    assert!(errors.is_empty(), "While loop with scoped binding should work: {:?}", errors);
}
