/// Dalin L 3.0 — DAP Debug Server Entry Point
///
/// Communicates with VSCode via stdin/stdout using the Debug Adapter Protocol.
/// Start with: `dalin-dap` (VSCode launch configuration handles activation)
fn main() {
    let mut server = dalin_dap::DebugServer::new();
    if let Err(e) = server.run() {
        eprintln!("[dalin-dap] Fatal error: {}", e);
        std::process::exit(1);
    }
}
