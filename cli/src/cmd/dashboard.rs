use crate::util;
use std::io::{Read, Write};
use std::net::TcpListener;

pub fn run() -> Result<(), String> {
    let banner = util::banner("DASHBOARD");
    println!("{}", banner);
    println!("\n  Starting dashboard on http://127.0.0.1:9898 ...");
    println!("  Ctrl+C to stop");

    let listener =
        TcpListener::bind("127.0.0.1:9898").map_err(|e| format!("Cannot bind :9898: {}", e))?;

    println!("\n  ✅ Dashboard listening on :9898\n");

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let mut buf = [0u8; 4096];
                if let Ok(n) = s.read(&mut buf) {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let (code, body, ct) = if req.contains("GET / ") || req.contains("/dashboard") {
                        (200, html_dashboard(), "text/html")
                    } else {
                        (404, "<h1>Not Found</h1>".into(), "text/html")
                    };
                    let resp = format!(
                        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
                        code, ct, body.len(), body
                    );
                    let _ = s.write(resp.as_bytes());
                }
            }
            Err(e) => eprintln!("  ❌ Stream: {}", e),
        }
    }
    Ok(())
}

fn html_dashboard() -> String {
    r#"
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Dalin L 3.0 — Cognitive Dashboard</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,sans-serif;background:#0a0a0a;color:#e0e0e0;padding:2rem}
h1{font-size:2rem;text-align:center;color:#c0b8ff;margin-bottom:1.5rem}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:1rem;max-width:1200px;margin:0 auto}
.card{background:#161616;border:1px solid #2a2a2a;border-radius:12px;padding:1.25rem}
.card h3{color:#888;font-size:.75rem;text-transform:uppercase;letter-spacing:.05em;margin-bottom:.75rem}
.badge{display:inline-block;padding:2px 8px;border-radius:4px;font-size:.8rem;font-weight:500}
.badge.idle{background:#2a2a2a;color:#888}
.badge.perceive{background:#1a3a2a;color:#4caf50}
.badge.reason{background:#1a2a3a;color:#64b5f6}
.badge.decide{background:#3a2a1a;color:#ffb74d}
.badge.act{background:#3a1a1a;color:#ef5350}
.badge.loop{background:#2a1a3a;color:#b39ddb}
.phase-indicator{display:flex;gap:4px;margin-top:.5rem}
.phase-step{flex:1;height:8px;border-radius:4px;background:#2a2a2a;transition:background .3s}
.phase-step.active{background:#7c6fda}
.phase-step.done{background:#4caf50}
.check-log{margin-top:.5rem;font-size:.8rem;line-height:1.8}
.check-log .ok{color:#4caf50}
.check-log .fail{color:#ef5350}
.stat{display:flex;justify-content:space-between;padding:4px 0;border-bottom:1px solid #1a1a1a;font-size:.85rem}
.stat:last-child{border:none}
.stat .label{color:#888}
.stat .value{color:#e0e0e0;font-weight:500}
.card ul{list-style:none;font-size:.8rem;color:#aaa;line-height:1.8}
</style>
</head>
<body>
<h1>Dalin L 3.0 &mdash; Cognitive Dashboard</h1>
<div class="grid">

  <div class="card">
    <h3>Cognitive Loop Phase</h3>
    <div><span class="badge idle">idle</span></div>
    <div class="phase-indicator">
      <div class="phase-step" id="p-perceive"></div>
      <div class="phase-step" id="p-reason"></div>
      <div class="phase-step" id="p-decide"></div>
      <div class="phase-step" id="p-act"></div>
      <div class="phase-step" id="p-loop"></div>
    </div>
    <ul id="phase-history" style="margin-top:8px">
      <li>no phase transitions recorded</li>
    </ul>
  </div>

  <div class="card">
    <h3>Governance</h3>
    <div class="stat"><span class="label">session level</span><span class="value">execute</span></div>
    <div class="stat"><span class="label">checks passed</span><span class="value" id="gov-passed">0</span></div>
    <div class="stat"><span class="label">checks failed</span><span class="value" id="gov-failed">0</span></div>
    <div class="check-log" id="gov-log">no checks yet</div>
  </div>

  <div class="card">
    <h3>Confidence Gate</h3>
    <div class="stat"><span class="label">threshold</span><span class="value">inferred</span></div>
    <div class="stat"><span class="label">fast path</span><span class="value" id="conf-fast">0</span></div>
    <div class="stat"><span class="label">guarded</span><span class="value" id="conf-guard">0</span></div>
    <div class="check-log" id="conf-log">no checks yet</div>
  </div>

  <div class="card">
    <h3>Time Monitor</h3>
    <ul id="time-log"><li>no timings yet</li></ul>
  </div>

  <div class="card">
    <h3>System</h3>
    <div class="stat"><span class="label">compiler</span><span class="value">dalin 3.0.0-dev</span></div>
    <div class="stat"><span class="label">phases</span><span class="value">A-H + J</span></div>
    <div class="stat"><span class="label">standards</span><span class="value">128 modules</span></div>
    <div class="stat"><span class="label">tests</span><span class="value">265+ total</span></div>
  </div>

</div>
</body>
</html>
"#.to_string()
}
