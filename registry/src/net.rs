//! 零依赖 HTTP/1.1 客户端 + 注册表联网下载。
//!
//! 仅使用 `std::net::TcpStream`，不引入任何外部 HTTP 库。用于：
//! - 从注册表索引端点解析可用包版本（`fetch_package_index`）
//! - 将 `Package.artifact_url` 指向的 `.dal` 工件真正下载到本地缓存（`download_artifact`）
//! - 按版本需求选取最优版本（`resolve_best`）
//!
//! 设计取舍：注册表传输采用明文 HTTP（`http://`），便于零依赖实现；
//! 生产部署应在注册表服务端前置 TLS 终止（反向代理）。
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

use dalin_compiler::package::SemVer;

use crate::Package;
use crate::sha256::sha256_hex;

/// 一次成功下载的产物记录。
#[derive(Debug, Clone)]
pub struct DownloadedArtifact {
    pub path: std::path::PathBuf,
    pub sha256: String,
    pub bytes: usize,
}

/// 版本需求匹配模式（解析 `dalan.toml` 中的 `version` 字段）。
enum ReqMode {
    Any,
    Exact(SemVer),
    Caret(SemVer),
    EqAbove(SemVer),
}

fn parse_req(req: &str) -> ReqMode {
    let r = req.trim();
    if r.is_empty() || r == "*" || r.eq_ignore_ascii_case("latest") {
        return ReqMode::Any;
    }
    if let Some(rest) = r.strip_prefix('^')
        && let Ok(v) = SemVer::parse(rest)
    {
        return ReqMode::Caret(v);
    }
    if let Some(rest) = r.strip_prefix(">=")
        && let Ok(v) = SemVer::parse(rest)
    {
        return ReqMode::EqAbove(v);
    }
    if let Ok(v) = SemVer::parse(r) {
        return ReqMode::Exact(v);
    }
    ReqMode::Any
}

/// 从远端索引条目中按版本需求选取最优（最高满足）版本。
#[must_use]
pub fn resolve_best(index: &[Package], req: &str) -> Option<Package> {
    let mode = parse_req(req);
    let mut best: Option<(SemVer, Package)> = None;
    for p in index {
        let v = SemVer::parse(&p.version).ok()?;
        let ok = match &mode {
            ReqMode::Any => true,
            ReqMode::Exact(rv) => &v == rv,
            ReqMode::Caret(rv) => v.major == rv.major && v.cmp(rv) >= 0,
            ReqMode::EqAbove(rv) => v.cmp(rv) >= 0,
        };
        if ok {
            match &best {
                Some((bv, _)) if v.cmp(bv) <= 0 => {}
                _ => best = Some((v, p.clone())),
            }
        }
    }
    best.map(|(_, p)| p)
}

// ═══════════════════════════════
//  零依赖 HTTP/1.1 客户端
// ═══════════════════════════════

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

/// 解析 `http://host[:port]/path` 为 (host, port, path)。
fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("仅支持 http:// 协议（收到 {url}）"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (rest[..i].to_string(), format!("/{}", &rest[i + 1..])),
        None => (rest.to_string(), "/".to_string()),
    };
    let (host, port) = match authority.rfind(':') {
        Some(i) => {
            let p: u16 = authority[i + 1..]
                .parse()
                .map_err(|_| format!("端口解析失败：{url}"))?;
            (authority[..i].to_string(), p)
        }
        None => (authority, 80),
    };
    if host.is_empty() {
        return Err(format!("host 为空：{url}"));
    }
    Ok((host, port, path))
}

fn find_header_end(buf: &[u8]) -> Result<usize, String> {
    let pat = [b'\r', b'\n', b'\r', b'\n'];
    buf.windows(4)
        .position(|w| w == pat)
        .ok_or_else(|| "响应缺少头部/正文边界".to_string())
}

fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let line_end = i + body[i..]
            .iter()
            .position(|&b| b == b'\r' || b == b'\n')
            .ok_or_else(|| "chunked：缺少分块大小行".to_string())?;
        let size_hex = String::from_utf8_lossy(&body[i..line_end])
            .trim()
            .to_string();
        let size = usize::from_str_radix(&size_hex, 16)
            .map_err(|_| format!("chunked：非法分块大小 '{size_hex}'"))?;
        i = line_end;
        while i < body.len() && (body[i] == b'\r' || body[i] == b'\n') {
            i += 1;
        }
        if size == 0 {
            break;
        }
        if i + size > body.len() {
            return Err("chunked：分块被截断".to_string());
        }
        out.extend_from_slice(&body[i..i + size]);
        i += size;
        while i < body.len() && (body[i] == b'\r' || body[i] == b'\n') {
            i += 1;
        }
    }
    Ok(out)
}

fn resolve_location(base: &str, rel: &str) -> String {
    if let Some(scheme_end) = base.find("//")
        && let Some(path_start) = base[scheme_end + 2..].find('/')
    {
        let authority = &base[..scheme_end + 2 + path_start];
        return format!("{authority}{rel}");
    }
    base.to_string()
}

fn parse_response(buf: &[u8], url: &str, redirects: u8) -> Result<HttpResponse, String> {
    let idx = find_header_end(buf)?;
    let header_str = String::from_utf8_lossy(&buf[..idx]);
    let mut body = buf[idx + 4..].to_vec();

    let mut lines = header_str.split("\r\n");
    let status_line = lines.next().ok_or_else(|| "空响应".to_string())?;
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "缺少状态码".to_string())?
        .parse()
        .map_err(|_| "非法状态码".to_string())?;

    let mut location: Option<String> = None;
    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "location" => location = Some(v.trim().to_string()),
                "transfer-encoding" if v.to_ascii_lowercase().contains("chunked") => chunked = true,
                "content-length" => content_length = v.trim().parse().ok(),
                _ => {}
            }
        }
    }

    if (300..=399).contains(&status_code) {
        return match location {
            Some(loc) if redirects < 5 => {
                let next = if loc.starts_with("http://") {
                    loc
                } else {
                    resolve_location(url, &loc)
                };
                http_get_impl(&next, redirects + 1)
            }
            Some(_) => Err(format!("重定向次数过多：{url}")),
            None => Err(format!("HTTP {status_code} 但缺少 Location 头")),
        };
    }

    if status_code != 200 {
        return Err(format!("HTTP {status_code}（{url}）"));
    }

    if chunked {
        body = decode_chunked(&body)?;
    } else if let Some(cl) = content_length
        && body.len() > cl
    {
        body.truncate(cl);
    }

    Ok(HttpResponse {
        status: status_code,
        body,
    })
}

fn http_get_impl(url: &str, redirects: u8) -> Result<HttpResponse, String> {
    let (host, port, path) = parse_url(url)?;
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|e| format!("连接 {addr} 失败：{e}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: dalib/3.0\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("发送请求失败：{e}"))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("读取响应失败：{e}"))?;
    parse_response(&buf, url, redirects)
}

/// 零依赖 HTTP GET：返回状态码与响应正文。自动跟随最多 5 次重定向。
pub fn http_get(url: &str) -> Result<(u16, Vec<u8>), String> {
    let r = http_get_impl(url, 0)?;
    Ok((r.status, r.body))
}

// ═══════════════════════════════
//  注册表下载 API
// ═══════════════════════════════

/// 从注册表索引端点拉取某包的可用版本列表。
///
/// 端点约定：`GET http://<host>/index/<name>` 返回 `Package` 的 JSON 数组。
pub fn fetch_package_index(host: &str, name: &str) -> Result<Vec<Package>, String> {
    let url = format!("http://{host}/index/{name}");
    let (_status, body) = http_get(&url)?;
    let text = String::from_utf8_lossy(&body);
    serde_json::from_str(text.trim()).map_err(|e| format!("解析索引 JSON 失败：{e}"))
}

/// 将 `artifact_url` 指向的工件真正下载到 `dest`，并返回路径与 SHA-256。
pub fn download_artifact(url: &str, dest: &Path) -> Result<DownloadedArtifact, String> {
    let (_status, body) = http_get(url)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建缓存目录失败：{e}"))?;
    }
    std::fs::write(dest, &body).map_err(|e| format!("写入工件失败：{e}"))?;
    let hash = sha256_hex(&body);
    Ok(DownloadedArtifact {
        path: dest.to_path_buf(),
        sha256: hash,
        bytes: body.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn test_parse_req_modes() {
        assert!(matches!(parse_req("*"), ReqMode::Any));
        assert!(matches!(parse_req("latest"), ReqMode::Any));
        assert!(matches!(parse_req(""), ReqMode::Any));
        assert!(matches!(parse_req("1.2.3"), ReqMode::Exact(_)));
        assert!(matches!(parse_req("^1.2.3"), ReqMode::Caret(_)));
        assert!(matches!(parse_req(">=1.2.3"), ReqMode::EqAbove(_)));
    }

    #[test]
    fn test_resolve_best_latest() {
        let index = vec![
            pkg("demo", "1.0.0", "http://h/a-1.0.0.dal"),
            pkg("demo", "1.2.0", "http://h/a-1.2.0.dal"),
            pkg("demo", "2.0.0", "http://h/a-2.0.0.dal"),
        ];
        let best = resolve_best(&index, "*").unwrap();
        assert_eq!(best.version, "2.0.0");
    }

    #[test]
    fn test_resolve_best_caret() {
        let index = vec![
            pkg("demo", "1.0.0", "u"),
            pkg("demo", "1.5.0", "u"),
            pkg("demo", "2.0.0", "u"),
        ];
        // ^1.2.3 → 同主版本、>=1.2.3 → 1.5.0
        let best = resolve_best(&index, "^1.2.3").unwrap();
        assert_eq!(best.version, "1.5.0");
    }

    #[test]
    fn test_resolve_best_exact_miss() {
        let index = vec![pkg("demo", "1.0.0", "u")];
        assert!(resolve_best(&index, "1.2.3").is_none());
        assert!(resolve_best(&index, ">=2.0.0").is_none());
    }

    #[test]
    fn test_parse_response_status_and_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello";
        let r = parse_response(raw, "http://h/x", 0).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"hello");
    }

    #[test]
    fn test_parse_response_non_200_errors() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        assert!(parse_response(raw, "http://h/x", 0).is_err());
    }

    #[test]
    fn test_decode_chunked() {
        // 两个分块：3 字节 "abc" 与 2 字节 "de"，最后 0 终止
        let body = b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        let out = decode_chunked(body).unwrap();
        assert_eq!(out, b"abcde");
    }

    #[test]
    fn integration_http_index_and_download() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = format!("127.0.0.1:{port}");
        let host_for_server = host.clone();

        let served = Arc::new(AtomicUsize::new(0));
        let served_srv = served.clone();

        let server = thread::spawn(move || {
            // 截止时间保证即使客户端异常也不会挂死测试线程
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let path = req
                            .lines()
                            .next()
                            .and_then(|l| l.split_whitespace().nth(1))
                            .unwrap_or("/")
                            .to_string();

                        if path == "/__shutdown" {
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            );
                            break;
                        }

                        let (status, body): (u16, Vec<u8>) = if path.starts_with("/index/") {
                            let pkgs = serde_json::json!([
                                {"name":"demo","version":"1.0.0","capability":"cpu","effect_level":"pure","artifact_url":format!("http://{host_for_server}/artifact/demo-1.0.0.dal"),"description":"d","author":"t"},
                                {"name":"demo","version":"1.2.0","capability":"cpu","effect_level":"pure","artifact_url":format!("http://{host_for_server}/artifact/demo-1.2.0.dal"),"description":"d","author":"t"}
                            ]);
                            (200, serde_json::to_vec(&pkgs).unwrap())
                        } else if path.starts_with("/artifact/") {
                            (200, b"fn add(a: Int, b: Int) -> Int { a + b }".to_vec())
                        } else {
                            (404, b"not found".to_vec())
                        };

                        let resp = format!(
                            "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                        served_srv.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) if Instant::now() > deadline => break,
                    Err(_) => break,
                }
            }
        });

        // 等待服务端就绪
        thread::sleep(Duration::from_millis(50));

        // 1) 拉取索引
        let index = fetch_package_index(&host, "demo").expect("fetch index");
        assert_eq!(index.len(), 2);

        // 2) 解析最新版本
        let best = resolve_best(&index, "*").expect("resolve best");
        assert_eq!(best.version, "1.2.0");

        // 3) 下载工件
        let tmp = std::env::temp_dir().join(format!("dalin_reg_test_{port}.dal"));
        let art = download_artifact(&best.artifact_url, &tmp).expect("download");
        let expected = b"fn add(a: Int, b: Int) -> Int { a + b }";
        assert_eq!(art.bytes, expected.len());
        assert_eq!(art.sha256, sha256_hex(expected));
        let written = std::fs::read(&tmp).unwrap();
        assert_eq!(written, expected);
        let _ = std::fs::remove_file(&tmp);

        // 触发服务端关闭，保证 join 立即返回
        let _ = http_get(&format!("http://{host}/__shutdown"));
        let _ = server.join();
        assert!(served.load(Ordering::SeqCst) >= 2);
    }

    fn pkg(name: &str, version: &str, url: &str) -> Package {
        Package {
            name: name.to_string(),
            version: version.to_string(),
            capability: "cpu".to_string(),
            effect_level: "pure".to_string(),
            artifact_url: url.to_string(),
            description: String::new(),
            author: String::new(),
        }
    }
}
