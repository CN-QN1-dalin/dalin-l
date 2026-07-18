//! Dalin L Language Server — smoke tests
#![cfg(test)]

#[test]
fn test_lsp_compiles() {
    // Smoke test: crate compiles without errors
}

#[test]
fn test_lsp_protocol_version() {
    let version = env!("CARGO_PKG_VERSION");
    assert!(version.contains("3.0"), "LSP version {}", version);
}

#[test]
fn test_json_rpc_format() {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///test.dal",
                "languageId": "dalin",
                "version": 1,
                "text": "fn main() {}"
            }
        }
    });
    assert_eq!(msg["jsonrpc"], "2.0");
    assert_eq!(msg["method"], "textDocument/didOpen");
}
