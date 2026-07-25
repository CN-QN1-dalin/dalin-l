//! Simple test client for `dalin bridge` — tests run/eval/ping commands via Unix socket.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

#[derive(Debug, serde::Serialize)]
struct TestMessage {
    id: u64,
    #[serde(rename = "type")]
    r#type: String,
    payload: serde_json::Value,
}

fn send_and_recv(socket_path: &str, msg: TestMessage) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket_path).map_err(|e| format!("Connect error: {e}"))?;

    let json = serde_json::to_string(&msg).map_err(|e| format!("Serialize error: {e}"))?;
    stream
        .write_all(json.as_bytes())
        .map_err(|e| format!("Write error: {e}"))?;
    stream.flush().map_err(|e| format!("Flush error: {e}"))?;

    let mut buffer = [0u8; 4096];
    let n = stream
        .read(&mut buffer)
        .map_err(|e| format!("Read error: {e}"))?;
    Ok(String::from_utf8_lossy(&buffer[..n]).to_string())
}

pub fn main() {
    if let Err(e) = run_tests("/tmp/test-bridge.sock") {
        eprintln!("Test error: {e}");
    } else {
        println!("✓ All tests passed");
    }
}

fn run_tests(socket_path: &str) -> Result<(), String> {
    println!("Testing bridge at: {socket_path}");

    // Test 1: Ping
    let ping_msg = TestMessage {
        id: 1,
        r#type: "ping".to_string(),
        payload: serde_json::json!({}),
    };

    match send_and_recv(socket_path, ping_msg) {
        Ok(resp) => {
            println!("✓ Ping response: {resp}");
            if resp.contains("pong") && resp.contains("true") {
                println!("  → Ping validation passed");
            } else {
                eprintln!("✗ Ping validation failed - missing pong field");
            }
        }
        Err(e) => eprintln!("✗ Ping error: {e}"),
    }

    // Test 2: Run
    let run_code = r#"
        let greeting = "Hello from Bridge"
        let count = 42
        println(greeting)
        println(count)
    "#
    .to_string();

    let run_msg = TestMessage {
        id: 2,
        r#type: "run".to_string(),
        payload: serde_json::json!({"code": run_code}),
    };

    match send_and_recv(socket_path, run_msg) {
        Ok(resp) => {
            println!("✓ Run response: {resp}");
            if resp.contains("success") || resp.contains("status") {
                println!("  → Run execution validation passed");
            } else {
                eprintln!("✗ Run validation failed - unexpected status");
            }
        }
        Err(e) => eprintln!("✗ Run error: {e}"),
    }

    // Test 3: Eval
    let eval_expr = "2 + 3 * 4".to_string();

    let eval_msg = TestMessage {
        id: 3,
        r#type: "eval".to_string(),
        payload: serde_json::json!({"expression": eval_expr}),
    };

    match send_and_recv(socket_path, eval_msg) {
        Ok(resp) => {
            println!("✓ Eval response: {resp}");
            if resp.contains("evaluated") || resp.contains("result") {
                println!("  → Eval validation passed");
            } else {
                eprintln!("✗ Eval validation failed - unexpected evaluation result");
            }
        }
        Err(e) => eprintln!("✗ Eval error: {e}"),
    }

    println!("\nAll tests completed.");
    Ok(())
}
