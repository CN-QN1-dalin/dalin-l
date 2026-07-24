/// Dalin L 3.0 — Borrow Checker / Memory Safety Engine
///
/// Implements ownership semantics, borrow validity, and lifetime tracking.
/// Two-tier model:
///   Tier 1 (Lexical Ownership): copy/move semantics at compile-time
///   Tier 2 (Reference Borrowing): &T / &mut T mutual exclusion + alias-vs-mutation
///
/// Algorithm:
/// - Ownership domains tracked via ScopeForest (hierarchical scope tree)
/// - Mutability tracked per binding with generation counters
/// - Reference validity checked by verifying all borrows complete before use
pub mod engine;
pub mod error;
pub mod model;
pub mod scope;
#[cfg(test)]
pub mod tests;

pub use engine::BorrowChecker;
pub use error::{BorrowError, BorrowErrorCode};
