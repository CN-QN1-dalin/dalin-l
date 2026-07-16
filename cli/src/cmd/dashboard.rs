use std::net::TcpListener;
use std::io::{Read, Write};
use crate::util;

pub fn run() -> Result<(), String> {
    let banner = util::banner("DASHBOARD");
    println!("{}", banner);
    println!("\n  Starting dashboard on http://127.0.0.1:9898 ...");
    println!("  Ctrl+C to stop");

    let listener = TcpListener::bind("127.0.0.1:9898")
        .map_err(|e| format!("Cannot bind :9898: {}", e))?;

    println!("\n  ✅ Dashboard listening on :9898\n");

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let mut buf = [0u8; 1024];
                if let Ok(n) = s.read(&mut buf) {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let (code, body, ct) = if req.contains("GET / ") || req.contains("/dashboard") {
                        (200, html_dashboard(), "text/html")
                    } else if req.contains("/api/status") {
                        (200, json_status(), "application/json")
                    } else {
                        (404, "<h1>Not Found</h1>".into(), "text/html")
                    };
                    let resp = format!("HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}", code, ct, body.len(), body);
                    let _ = s.write(resp.as_bytes());
                }
            }
            Err(e) => eprintln!("  ❌ Stream: {}", e),
        }
    }
    Ok(())
}

fn html_dashboard() -> String {
    r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Dalin L 2.0</title>
<style>*{margin:0;padding:0;box-sizing:border-box}body{font-family:-apple-system,sans-serif;background:#0a0a0a;color:#e0e0e0;padding:2rem}h1{font-size:2.5rem;text-align:center;background:linear-gradient(135deg,#667eea,#764ba2);-webkit-background-clip:text;-webkit-text-fill-color:transparent}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:1rem;max-width:1000px;margin:2rem auto}.card{background:#161616;border:1px solid #2a2a2a;border-radius:12px;padding:1.5rem}.card h3{color:#888;font-size:.8rem;text-transform:uppercase}.bar-chart{display:flex;gap:3px;height:30px;align-items:flex-end;margin-top:.5rem}.bar{width:25px;border-radius:3px 3px 0 0}</style></head>
<body><h1>Dalin L 2.0 Dashboard</h1><p style="text-align:center;color:#666;margin-top:.5rem">Compiler Control Panel</p>
<div class="grid"><div class="card"><h3>Status</h3><p style="color:#4caf50;font-size:2rem">●</p></div>
<div class="card"><h3>Phases</h3><p style="font-size:2rem;color:#4caf50">A-H</p></div>
<div class="card"><h3>Build Perf</h3><div class="bar-chart"><div class="bar" style="height:60%;background:#4caf50"></div><div class="bar" style="height:80%;background:#4caf50"></div><div class="bar" style="height:90%;background:#4caf50"></div><div class="bar" style="height:70%;background:#4caf50"></div><div class="bar" style="height:95%;background:#4caf50"></div></div></div>
</div></body></html>"#.to_string()
}

fn json_status() -> String {
    format!("{{\"version\":\"0.1.0\",\"status\":\"ok\",\"phases\":[\"A\",\"B\",\"C\",\"D\",\"E\",\"F\",\"G\",\"H\"]}}")
}
