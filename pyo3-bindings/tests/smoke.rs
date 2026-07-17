//! Dalin L Python bindings — smoke tests
#![cfg(test)]

#[test]
fn test_pyo3_compiles() {
    assert!(true, "pyo3 bindings crate compiles");
}

#[test]
fn test_pyo3_version() {
    let v = env!("CARGO_PKG_VERSION");
    assert!(!v.is_empty());
}
