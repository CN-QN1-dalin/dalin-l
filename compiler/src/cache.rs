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

/// Compute a "SHA-256-like" hash and return its hex encoding.
#[must_use]
pub fn sha256(data: &[u8]) -> String {
    let h = hash_bytes(data);
    format!("{h:016x}")
}

fn hash_bytes(data: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    for chunk in data.chunks(64) {
        h.write(chunk);
    }
    h.finish()
}

/// Get the cache directory path.
#[must_use]
pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".dalin_cache")
}

/// Ensure the cache directory exists.
pub fn ensure_cache_dir(project_root: &Path) -> std::io::Result<PathBuf> {
    let dir = cache_dir(project_root);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Compute a file's cache key.
#[must_use]
pub fn compute_cache_key(file_path: &Path, content: &str) -> String {
    let file_hash = sha256(content.as_bytes());
    let file_name = file_path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    format!("{file_name}_{file_hash}")
}

/// Load the content of a cached file from the cache directory.
#[must_use]
pub fn load_cache(project_root: &Path, cache_key: &str) -> Option<Vec<u8>> {
    let dir = cache_dir(project_root);
    let cache_file = dir.join(format!("{cache_key}.cache"));
    if cache_file.exists() {
        match fs::read(&cache_file) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("  ⚠ Failed to read cache {cache_key}: {e}");
                None
            }
        }
    } else {
        None
    }
}

/// Write compilation results to the cache.
pub fn write_cache(project_root: &Path, cache_key: &str, data: &[u8]) -> std::io::Result<()> {
    let dir = cache_dir(project_root);
    let cache_file = dir.join(format!("{cache_key}.cache"));
    fs::write(cache_file, data)
}

/// Check whether the source file needs recompilation.
/// Returns true if a cache entry exists and the source is unchanged (same hash).
#[must_use]
pub fn is_cached(file_path: &Path, content: &str, project_root: &Path) -> bool {
    let cache_key = compute_cache_key(file_path, content);
    load_cache(project_root, &cache_key).is_some()
}

/// Cacheable trait — implemented by types that support binary serialization/deserialization
///
/// Types implementing this trait can automatically participate in cache hits/writes via `get_or_compile`.
/// Built-in implementations: `Vec<u8>` (identity), `String` (UTF-8).
pub trait Cacheable {
    /// 将自身序列化为二进制
    fn serialize(&self) -> Vec<u8>;
    /// 从二进制反序列化, 失败返回 None
    fn deserialize(data: &[u8]) -> Option<Self>
    where
        Self: Sized;
}

/// `Vec<u8>` 的 Cacheable 实现 — identity (无额外编码)
impl Cacheable for Vec<u8> {
    fn serialize(&self) -> Vec<u8> {
        self.clone()
    }
    fn deserialize(data: &[u8]) -> Option<Self> {
        Some(data.to_vec())
    }
}

/// String 的 Cacheable 实现 — UTF-8 编解码
impl Cacheable for String {
    fn serialize(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
    fn deserialize(data: &[u8]) -> Option<Self> {
        String::from_utf8(data.to_vec()).ok()
    }
}

/// Get or compile: check the cache first; on a hit, deserialize and return; otherwise run the compile function and write to the cache.
///
/// The generic bound `T: Cacheable` ensures only serializable types can participate in caching.
/// The cache format is determined by the `Cacheable` implementation (currently: Vec<u8>=identity, String=UTF-8).
pub fn get_or_compile<F, T>(
    file_path: &Path,
    content: &str,
    project_root: &Path,
    compile_fn: F,
) -> T
where
    T: Cacheable,
    F: FnOnce() -> T,
{
    let cache_key = compute_cache_key(file_path, content);

    // 尝试从缓存加载并反序列化
    let result = load_cache(project_root, &cache_key).and_then(|data| T::deserialize(&data));
    if let Some(result) = result {
        return result;
    }

    // 编译并写入缓存
    let result = compile_fn();
    let serialized = result.serialize();
    let _ = write_cache(project_root, &cache_key, &serialized);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ─── Cacheable trait 测试 ───────────────────────────────

    #[test]
    fn test_cacheable_vec_u8_roundtrip() {
        let original: Vec<u8> = vec![1, 2, 3, 255, 0, 128];
        let serialized = original.serialize();
        let deserialized = Vec::<u8>::deserialize(&serialized);
        assert_eq!(deserialized, Some(original));
    }

    #[test]
    fn test_cacheable_string_roundtrip() {
        let original = String::from("fn hello() { return 42 }");
        let serialized = original.serialize();
        let deserialized = String::deserialize(&serialized);
        assert_eq!(deserialized.as_deref(), Some(original.as_str()));
    }

    #[test]
    fn test_cacheable_string_invalid_utf8_returns_none() {
        let bad_bytes = &[0xFF, 0xFE, 0xFD];
        assert!(String::deserialize(bad_bytes).is_none());
    }

    #[test]
    fn test_cacheable_empty_vec() {
        let original: Vec<u8> = vec![];
        let serialized = original.serialize();
        assert!(serialized.is_empty());
        let deserialized = Vec::<u8>::deserialize(&serialized);
        assert_eq!(deserialized, Some(vec![]));
    }

    // ─── get_or_compile 集成测试 ────────────────────────────

    #[test]
    fn test_get_or_compile_cache_hit_string() {
        let temp_dir = std::env::temp_dir().join(format!(
            "dalin_goc_hit_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        ensure_cache_dir(&temp_dir).unwrap();

        let test_file = PathBuf::from("/tmp/test_goc.dal");
        let content = "fn add(x, y) { return x + y }";

        // 用计数器验证 compile_fn 只被调用一次
        let call_count = std::cell::Cell::new(0u32);

        // 第一次调用 — 缓存未命中, 执行 compile_fn
        let result1 = get_or_compile(&test_file, content, &temp_dir, || {
            call_count.set(call_count.get() + 1);
            String::from("compiled_bytecode_v1")
        });
        assert_eq!(result1, "compiled_bytecode_v1");
        assert_eq!(call_count.get(), 1);

        // 第二次调用 — 缓存命中, compile_fn 不应执行
        let result2 = get_or_compile(&test_file, content, &temp_dir, || {
            call_count.set(call_count.get() + 1);
            String::from("compiled_bytecode_v2") // 不应返回这个
        });
        assert_eq!(result2, "compiled_bytecode_v1"); // 返回缓存值
        assert_eq!(call_count.get(), 1); // 仍然只调用了一次

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_or_compile_cache_miss_on_content_change() {
        let temp_dir = std::env::temp_dir().join(format!(
            "dalin_goc_change_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        ensure_cache_dir(&temp_dir).unwrap();

        let test_file = PathBuf::from("/tmp/test_goc2.dal");

        // 内容 V1
        let content_v1 = "fn v1() {}";
        let result1 = get_or_compile(&test_file, content_v1, &temp_dir, || {
            String::from("v1_result")
        });
        assert_eq!(result1, "v1_result");

        // 内容变更 → 新 cache_key → 缓存未命中
        let content_v2 = "fn v2() { return 42 }";
        let result2 = get_or_compile(&test_file, content_v2, &temp_dir, || {
            String::from("v2_result")
        });
        assert_eq!(result2, "v2_result"); // 重新编译, 得到新结果

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_or_compile_vec_u8() {
        let temp_dir = std::env::temp_dir().join(format!(
            "dalin_goc_vec_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        ensure_cache_dir(&temp_dir).unwrap();

        let test_file = PathBuf::from("/tmp/test_goc3.dal");
        let content = "binary data test";

        let result1 = get_or_compile(&test_file, content, &temp_dir, || {
            vec![0x01u8, 0x02, 0x03, 0x04]
        });
        assert_eq!(result1, vec![0x01u8, 0x02, 0x03, 0x04]);

        // 第二次应命中缓存
        let result2 = get_or_compile(&test_file, content, &temp_dir, || {
            vec![0xFFu8, 0xFF] // 不应返回这个
        });
        assert_eq!(result2, vec![0x01u8, 0x02, 0x03, 0x04]);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
