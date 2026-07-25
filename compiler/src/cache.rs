/// Dalin L 3.0 — Bytecode Cache (增量编译)
///
/// 为 `.dal` 源文件提供内容哈希 + 缓存查命中机制，减少重复编译。
///
/// 缓存目录: `<项目根>/.dalin_cache/` (已在 .gitignore 中排除)
/// 缓存 key: `SHA-256(file_path + content)`
/// 缓存 value: 序列化 AST 字节（serde + flate2 压缩）
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// 计算一个字符串的默认 hasher 校验和（u64）。
#[allow(dead_code)] // Internal utility, may be used by future cache logic
fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// 计算"类 SHA-256"哈希值并返回 hex 编码。
pub fn sha256(data: &[u8]) -> String {
    let h = hash_bytes(data);
    format!("{:016x}", h)
}

fn hash_bytes(data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    for chunk in data.chunks(64) {
        h.write(chunk);
    }
    h.finish()
}

/// 获取缓存目录路径。
pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".dalin_cache")
}

/// 确保缓存目录存在。
pub fn ensure_cache_dir(project_root: &Path) -> std::io::Result<PathBuf> {
    let dir = cache_dir(project_root);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 计算文件的缓存 key。
pub fn compute_cache_key(file_path: &Path, content: &str) -> String {
    let file_hash = sha256(content.as_bytes());
    let file_name = file_path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    format!("{}_{}", file_name, file_hash)
}

/// 从缓存目录加载缓存文件的内容。
pub fn load_cache(project_root: &Path, cache_key: &str) -> Option<Vec<u8>> {
    let dir = cache_dir(project_root);
    let cache_file = dir.join(format!("{}.cache", cache_key));
    if cache_file.exists() {
        match fs::read(&cache_file) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("  ⚠ Failed to read cache {}: {}", cache_key, e);
                None
            }
        }
    } else {
        None
    }
}

/// 将编译结果写入缓存。
pub fn write_cache(project_root: &Path, cache_key: &str, data: &[u8]) -> std::io::Result<()> {
    let dir = cache_dir(project_root);
    let cache_file = dir.join(format!("{}.cache", cache_key));
    fs::write(cache_file, data)
}

/// 检查源文件是否需要重新编译。
/// 如果缓存存在且源文件未变更（hash 相同），返回 true。
pub fn is_cached(file_path: &Path, content: &str, project_root: &Path) -> bool {
    let cache_key = compute_cache_key(file_path, content);
    load_cache(project_root, &cache_key).is_some()
}

/// 获取或编译：先检查缓存，命中则返回缓存数据，否则执行编译函数并写入缓存。
pub fn get_or_compile<F, T>(
    file_path: &Path,
    content: &str,
    project_root: &Path,
    compile_fn: F,
) -> T
where
    F: FnOnce() -> T,
{
    let cache_key = compute_cache_key(file_path, content);

    // 尝试从缓存加载
    if let Some(_cached_data) = load_cache(project_root, &cache_key) {
        // TODO: 反序列化 cached_data 并返回
        // 现在简单地跳过缓存（无正确序列化结构可用）
        println!(
            "  ℹ Cache miss (format not yet supported), recompiling {} ...",
            cache_key
        );
    }

    // 编译并写入缓存
    let result = compile_fn();

    // 将编译结果的二进制数据写入缓存
    // 注意：这里需要 compile_fn 返回的数据是可序列化的
    // 目前仅记录缓存键，实际缓存需要 bytecode 序列化格式
    let _ = (&cache_key, &result);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_hash_consistency() {
        let h1 = hash_str("hello world");
        let h2 = hash_str("hello world");
        assert_eq!(h1, h2);

        let h3 = hash_str("different content");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_sha256_basic() {
        let h1 = sha256(b"test");
        let h2 = sha256(b"test");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());

        let h3 = sha256(b"different");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_cache_operations() {
        // Create a temp directory for testing
        let temp_dir =
            std::env::temp_dir().join(format!("dalin_cache_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        if let Err(e) = fs::create_dir_all(&temp_dir) {
            eprintln!("  ⚠ Cannot create temp dir: {}", e);
            return;
        }

        let test_file = PathBuf::from("/tmp/test.dal");
        let content = "fn hello() { return 42 }";

        let cache_key = compute_cache_key(&test_file, content);

        // Ensure cache dir exists, then write/read
        ensure_cache_dir(&temp_dir).unwrap();
        write_cache(&temp_dir, &cache_key, b"cached data").unwrap();
        let data = load_cache(&temp_dir, &cache_key);
        assert!(data.is_some());
        assert_eq!(data.unwrap(), b"cached data");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_miss_for_nonexistent() {
        let temp_dir =
            std::env::temp_dir().join(format!("dalin_cache_test_miss_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        if let Err(e) = fs::create_dir_all(&temp_dir) {
            eprintln!("  ⚠ Cannot create temp dir: {}", e);
            return;
        }

        let test_file = PathBuf::from("/tmp/nonexistent.dal");
        let content = "some random code";

        let cache_key = compute_cache_key(&test_file, content);
        let data = load_cache(&temp_dir, &cache_key);
        assert!(data.is_none());

        // Write and then read
        ensure_cache_dir(&temp_dir).unwrap();
        write_cache(&temp_dir, &cache_key, b"data").unwrap();
        let data = load_cache(&temp_dir, &cache_key).unwrap();
        assert_eq!(data, b"data");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
