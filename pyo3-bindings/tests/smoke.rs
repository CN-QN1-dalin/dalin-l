//! Dalin L Python bindings — smoke tests
#![cfg(test)]

#[test]
fn test_pyo3_compiles() {
    // Smoke test: pyo3-bindings crate compiles without errors
}

#[test]
fn test_pyo3_version() {
    let v = env!("CARGO_PKG_VERSION");
    assert!(!v.is_empty());
}
