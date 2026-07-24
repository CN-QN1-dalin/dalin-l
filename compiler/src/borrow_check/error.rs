/// Dalin L 3.0 — Borrow Checker Error types with codes for IDE diagnostics
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorrowErrorCode {
    /// Cannot move out of `name` because it was borrowed immutably
    BorrowedMutably,
    /// Cannot mutably borrow `name` while immutable borrows exist
    ImmutableBorrowAliased,
    /// Variable `name` does not live long enough (lifetime too short)
    LifetimeTooShort,
    /// Cannot use `name` because it was moved (ownership transferred)
    MoveOccurred,
    /// Cannot assign to `name` because it is not mutable (missing `mut` / `var`)
    NotMutable,
    /// Cannot drop `name` before end of lifetime
    DropBeforeEndOfLifetime,
}

impl fmt::Display for BorrowErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BorrowedMutably => write!(f, "cannot borrow as mutable"),
            Self::ImmutableBorrowAliased => write!(f, "cannot borrow as mutable because previous immutable borrow still exists"),
            Self::LifetimeTooShort => write!(f, "`name` does not live long enough"),
            Self::MoveOccurred => write!(f, "use of moved value"),
            Self::NotMutable => write!(f, "cannot assign to immutable binding"),
            Self::DropBeforeEndOfLifetime => write!(f, "cannot drop `name` early"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BorrowError {
    pub code: BorrowErrorCode,
    pub name: String,
    pub line: usize,
    pub column: usize,
    /// Optional suggestion: "consider adding `mut` here"
    pub suggestion: Option<String>,
}

impl BorrowError {
    pub fn new(code: BorrowErrorCode, name: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            code,
            name: name.into(),
            line,
            column,
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl fmt::Display for BorrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}:{}]: {}",
            self.line, self.column, self.code
        )?;
        if !self.name.is_empty() {
            write!(f, " `{}`", self.name)?;
        }
        Ok(())
    }
}
