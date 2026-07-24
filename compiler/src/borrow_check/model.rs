/// Dalin L 3.0 — Borrow Checker Data Model
///
/// Core abstractions: Binding, Borrow, ScopeDomain
/// Represents the mutability state of a binding in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Immutable, // `let x = ...` or `const`
    Mutable,   // `var x = ...` or `mut x` parameter
}

/// An individual binding introduced by let/var/fn-param/const
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub mutability: Mutability,
    /// Whether the type is Copy-compatible (int, float, bool, char)
    /// Moving a Copy type does NOT transfer ownership
    pub copyable: bool,
}

impl Binding {
    pub fn new(
        name: impl Into<String>,
        mutability: Mutability,
        copyable: bool,
    ) -> Self {
        Self {
            name: name.into(),
            mutability,
            copyable,
        }
    }
}

/// A borrow annotation: who borrows whom, how, and where
#[derive(Debug, Clone)]
pub struct BorrowAnnotation {
    /// The borrower's identifier
    pub borrower: String,
    /// What is being borrowed (target binding name)
    pub target: String,
    /// Mutable or immutable borrow
    pub mutable: bool,
    /// Source location line number
    pub line: usize,
}

impl BorrowAnnotation {
    pub fn new(borrower: impl Into<String>, target: impl Into<String>, mutable: bool, line: usize) -> Self {
        Self {
            borrower: borrower.into(),
            target: target.into(),
            mutable,
            line,
        }
    }
}

/// A single active borrow within a scope
#[derive(Debug, Clone)]
pub struct ActiveBorrow {
    /// Unique borrow ID (generation counter)
    pub id: u64,
    pub borrower: String,
    pub target: String,
    pub mutable: bool,
    pub created_line: usize,
}

impl ActiveBorrow {
    pub fn new(id: u64, borrower: impl Into<String>, target: impl Into<String>, mutable: bool, line: usize) -> Self {
        Self {
            id,
            borrower: borrower.into(),
            target: target.into(),
            mutable,
            created_line: line,
        }
    }
}

/// Ownership transfer event
#[derive(Debug, Clone)]
pub struct MoveEvent {
    pub source: String,
    pub target: Option<String>,
    pub line: usize,
}

impl MoveEvent {
    pub fn new(source: impl Into<String>, target: Option<String>, line: usize) -> Self {
        Self {
            source: source.into(),
            target,
            line,
        }
    }
}
