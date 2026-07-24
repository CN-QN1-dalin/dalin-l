/// Dalin L 3.0 — Borrow Checker Scope Management
///
/// ScopeForest: a hierarchical tree of lexical scopes, each with its own
/// bindings and active borrows. Supports Enter/Exit scoping and cross-scope
/// lifetime queries.
use crate::borrow_check::{model::*, error::*};
use std::collections::{HashMap, HashSet};

/// Represents a single lexical scope (block, function body, etc.)
#[derive(Debug, Clone)]
pub struct Scope {
    /// Bindings declared in this scope
    pub bindings: HashMap<String, Binding>,
    /// Active borrows created in this scope
    pub active_borrows: Vec<ActiveBorrow>,
    /// Moved values in this scope
    pub moved_values: HashSet<String>,
    /// Parent scope (None for root/top-level)
    pub parent: Option<usize>,
}

impl Scope {
    pub fn new(parent: Option<usize>) -> Self {
        Self {
            bindings: HashMap::new(),
            active_borrows: Vec::new(),
            moved_values: HashSet::new(),
            parent,
        }
    }

    /// Add a binding to this scope
    pub fn add_binding(&mut self, binding: Binding) {
        self.bindings.insert(binding.name.clone(), binding);
    }

    /// Check if a value is moved in this scope
    pub fn is_moved(&self, name: &str) -> bool {
        self.moved_values.contains(name)
    }

    /// Check if an immutable borrow exists for the given target
    pub fn has_immutable_borrow(&self, target: &str) -> bool {
        self.active_borrows.iter().any(|b| b.target == target && !b.mutable)
    }

    /// Check if a mutable borrow exists for the given target
    pub fn has_mutable_borrow(&self, target: &str) -> bool {
        self.active_borrows.iter().any(|b| b.target == target && b.mutable)
    }
}

/// Forest of scopes organized hierarchically.
/// Root scope (index 0) represents top-level or function body.
#[derive(Debug, Clone)]
pub struct ScopeForest {
    pub scopes: Vec<Scope>,
    /// Current scope index
    pub current: usize,
    /// Borrow ID counter
    next_borrow_id: u64,
}

impl Default for ScopeForest {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeForest {
    pub fn new() -> Self {
        let root = Scope::new(None);
        Self {
            scopes: vec![root],
            current: 0,
            next_borrow_id: 1,
        }
    }

    /// Get current scope reference
    pub fn current_scope(&self) -> &Scope {
        &self.scopes[self.current]
    }

    /// Get current scope mutable reference
    pub fn current_scope_mut(&mut self) -> &mut Scope {
        &mut self.scopes[self.current]
    }

    /// Enter a new nested scope (push child)
    pub fn enter_scope(&mut self) -> usize {
        let parent = self.current;
        let idx = self.scopes.len();
        self.scopes.push(Scope::new(Some(parent)));
        self.current = idx;
        idx
    }

    /// Leave the current scope (pop), returning to parent
    pub fn exit_scope(&mut self) {
        if let Some(parent) = self.scopes[self.current].parent {
            self.current = parent;
        }
    }

    /// Add a binding to the current scope
    pub fn add_binding(&mut self, binding: Binding) {
        self.current_scope_mut().add_binding(binding);
    }

    /// Register an immutable borrow of `target` by `borrower`
    /// Returns error if mutable borrows already exist
    pub fn add_immutable_borrow(&mut self, borrower: impl Into<String>, target: impl Into<String>, line: usize) -> Result<u64, BorrowError> {
        let borrower = borrower.into();
        let target = target.into();
        let scope = &self.scopes[self.current];
        if scope.has_mutable_borrow(&target) {
            return Err(BorrowError::new(
                BorrowErrorCode::ImmutableBorrowAliased,
                &target,
                line,
                0,
            ).with_suggestion("consider waiting for mutable borrow to end"));
        }
        let id = self.next_borrow_id;
        self.next_borrow_id += 1;
        self.scopes[self.current].active_borrows.push(
            ActiveBorrow::new(id, borrower, target, false, line)
        );
        Ok(id)
    }

    /// Register a mutable borrow of `target` by `borrower`
    /// Returns error if immutable borrows already exist
    pub fn add_mutable_borrow(&mut self, borrower: impl Into<String>, target: impl Into<String>, line: usize) -> Result<u64, BorrowError> {
        let borrower = borrower.into();
        let target = target.into();
        let scope = &self.scopes[self.current];
        if scope.has_immutable_borrow(&target) {
            return Err(BorrowError::new(
                BorrowErrorCode::BorrowedMutably,
                &target,
                line,
                0,
            ).with_suggestion("consider using a mutable variable"));
        }
        let id = self.next_borrow_id;
        self.next_borrow_id += 1;
        self.scopes[self.current].active_borrows.push(
            ActiveBorrow::new(id, borrower, target, true, line)
        );
        Ok(id)
    }

    /// Mark a value as moved in the current scope
    pub fn mark_moved(&mut self, name: impl Into<String>, _target: Option<String>, _line: usize) -> Result<(), BorrowError> {
        let name = name.into();
        let is_copyable = self.lookup_binding(&name);
        // Copy types can always be copied — no ownership transfer
        if is_copyable.unwrap_or(false) {
            return Ok(());
        }
        self.scopes[self.current].moved_values.insert(name);
        Ok(())
    }

    /// Clear move tracking when scope exits
    pub fn clear_moves_on_exit(&mut self) {
        if let Some(parent) = self.scopes[self.current].parent {
            // Moves don't escape the inner scope
            let inner_moves: HashSet<String> = self.scopes[self.current].moved_values.iter().cloned().collect();
            self.scopes[parent].moved_values.retain(|k| !inner_moves.contains(k));
        }
    }

    /// Check if a value was moved (and is not copyable)
    pub fn is_moved_in_scope(&self, name: &str) -> bool {
        let mut idx = self.current;
        loop {
            if self.scopes[idx].is_moved(name) {
                return true;
            }
            match self.scopes[idx].parent {
                Some(p) => idx = p,
                None => break,
            }
        }
        false
    }

    /// Check if a binding exists and is copyable
    pub fn lookup_binding(&self, name: &str) -> Option<bool> {
        let mut idx = self.current;
        loop {
            if let Some(b) = self.scopes[idx].bindings.get(name) {
                return Some(b.copyable);
            }
            match self.scopes[idx].parent {
                Some(p) => idx = p,
                None => return None,
            }
        }
    }

    /// Check if a binding exists and returns whether it is mutable
    pub fn lookup_binding_is_mutable(&self, name: &str) -> Option<bool> {
        let mut idx = self.current;
        loop {
            if let Some(b) = self.scopes[idx].bindings.get(name) {
                return Some(b.mutability == Mutability::Mutable);
            }
            match self.scopes[idx].parent {
                Some(p) => idx = p,
                None => return None,
            }
        }
    }

    /// Mark all borrows on `target` in current scope as released (borrow ends at scope exit)
    pub fn release_borrows_on(&mut self, target: &str) {
        self.scopes[self.current].active_borrows.retain(|b| b.target != target);
    }

    /// Check if assigning to a binding requires mutability
    pub fn check_assignable(&self, name: &str) -> Result<(), BorrowError> {
        if let Some(b) = self.lookup_binding_full(name)
            && b.mutability == Mutability::Immutable
        {
            return Err(BorrowError::new(
                BorrowErrorCode::NotMutable,
                name,
                0, 0,
            ).with_suggestion(format!("declare `{}` as `var {}`", name, name)));
        }
        Ok(())
    }

    fn lookup_binding_full(&self, name: &str) -> Option<&Binding> {
        let mut idx = self.current;
        loop {
            if let Some(b) = self.scopes[idx].bindings.get(name) {
                return Some(b);
            }
            match self.scopes[idx].parent {
                Some(p) => idx = p,
                None => return None,
            }
        }
    }
}
