//! Dalin L 包注册表 — 包发布 / 版本管理 / 依赖解析
//!
//! Schema 与架构设计文档对齐：
//! packages(id PK, name, version, capability, effect_level, artifact_url, signature, created_at)

use std::collections::HashMap;

/// 包的元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub capability: String,
    pub effect_level: String,
    pub artifact_url: String,
    pub description: String,
    pub author: String,
}

/// 内存包注册表
#[derive(Default)]
pub struct PackageRegistry {
    packages: HashMap<String, Vec<Package>>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn publish(&mut self, pkg: Package) -> Result<(), String> {
        let entry = self.packages.entry(pkg.name.clone()).or_default();
        if entry.iter().any(|p| p.version == pkg.version) {
            return Err(format!("version {} already exists", pkg.version));
        }
        entry.push(pkg);
        Ok(())
    }

    pub fn resolve(&self, name: &str, version_req: &str) -> Option<&Package> {
        match self.packages.get(name) {
            Some(pkgs) => {
                if version_req == "latest" || version_req.is_empty() {
                    pkgs.last()
                } else {
                    pkgs.iter().find(|p| p.version == version_req)
                }
            }
            None => None,
        }
    }

    pub fn search(&self, query: &str) -> Vec<&Package> {
        let q = query.to_lowercase();
        self.packages
            .values()
            .flat_map(|pkgs| pkgs.iter())
            .filter(|p| {
                p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn list(&self) -> Vec<&Package> {
        self.packages
            .values()
            .flat_map(|pkgs| pkgs.iter())
            .collect()
    }
}

// ═══════════════════════════════
//  本地文件系统 Registry (服务端)
// ═══════════════════════════════

use std::fs;
use std::path::Path;

/// 索引中的单个版本引用 (与客户端 `dalin_compiler::package::PackageVersionRef` 字段对齐)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageVersionRef {
    pub name: String,
    pub version: String,
    pub artifact_url: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub effect_level: Option<String>,
}

/// registry 索引 (某包的全部可用版本)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageIndex {
    pub packages: Vec<PackageVersionRef>,
}

/// 本地文件系统 registry 服务端
///
/// 将包发布为目录结构: `{root}/{name}/index.json` + `{root}/{name}/{version}.dal`
/// 与客户端 [`dalin_compiler::package::LocalRegistryClient`] 兼容。
pub struct LocalRegistry {
    root: std::path::PathBuf,
}

impl LocalRegistry {
    pub fn from_dir<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// 发布一个包版本: 写入 artifact 并更新 index.json (含 SHA-256)
    pub fn publish(&self, pkg: &Package, artifact: &[u8]) -> Result<(), String> {
        let dir = self.root.join(&pkg.name);
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;

        let artifact_path = dir.join(format!("{}.dal", pkg.version));
        fs::write(&artifact_path, artifact).map_err(|e| format!("write artifact: {}", e))?;

        let mut index = self
            .read_index(&pkg.name)
            .unwrap_or(PackageIndex { packages: vec![] });

        let entry = PackageVersionRef {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            artifact_url: format!("{}.dal", pkg.version),
            checksum: Some(sha256_hex_registry(artifact)),
            capability: Some(pkg.capability.clone()),
            effect_level: Some(pkg.effect_level.clone()),
        };
        if let Some(pos) = index.packages.iter().position(|p| p.version == pkg.version) {
            index.packages[pos] = entry;
        } else {
            index.packages.push(entry);
        }

        let idx_path = dir.join("index.json");
        let json =
            serde_json::to_string_pretty(&index).map_err(|e| format!("serialize index: {}", e))?;
        fs::write(&idx_path, json).map_err(|e| format!("write index: {}", e))?;
        Ok(())
    }

    fn read_index(&self, name: &str) -> Option<PackageIndex> {
        let p = self.root.join(name).join("index.json");
        fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// 返回某包的索引 JSON (供 HTTP 服务端直接 serve)
    pub fn index_json(&self, name: &str) -> Result<String, String> {
        let idx = self
            .read_index(name)
            .ok_or_else(|| format!("no index for '{}'", name))?;
        serde_json::to_string_pretty(&idx).map_err(|e| format!("serialize index: {}", e))
    }
}

/// SHA-256 (hex) — registry 侧校验和
pub fn sha256_hex_registry(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_pkg() -> Package {
        Package {
            name: "demo".into(),
            version: "0.1.0".into(),
            capability: "cpu".into(),
            effect_level: "pure".into(),
            artifact_url: "https://registry.dalinl.dev/pkgs/demo-0.1.0.dal".into(),
            description: "A demo package".into(),
            author: "test".into(),
        }
    }

    #[test]
    fn publish_and_resolve() {
        let mut reg = PackageRegistry::new();
        reg.publish(demo_pkg()).unwrap();
        let resolved = reg.resolve("demo", "0.1.0");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().version, "0.1.0");
    }

    #[test]
    fn reject_duplicate_version() {
        let mut reg = PackageRegistry::new();
        reg.publish(demo_pkg()).unwrap();
        assert!(reg.publish(demo_pkg()).is_err());
    }

    #[test]
    fn search_by_name() {
        let mut reg = PackageRegistry::new();
        reg.publish(demo_pkg()).unwrap();
        let results = reg.search("demo");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn resolve_latest() {
        let mut reg = PackageRegistry::new();
        let mut v1 = demo_pkg();
        v1.version = "0.1.0".into();
        reg.publish(v1).unwrap();
        let mut v2 = demo_pkg();
        v2.version = "0.2.0".into();
        reg.publish(v2).unwrap();
        let resolved = reg.resolve("demo", "latest");
        assert_eq!(resolved.unwrap().version, "0.2.0");
    }

    #[test]
    fn local_registry_publish_and_index() {
        let base = std::env::temp_dir().join(format!("dalin_reg_srv_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let reg = LocalRegistry::from_dir(&base);

        let pkg = Package {
            name: "demo".into(),
            version: "0.1.0".into(),
            capability: "cpu".into(),
            effect_level: "pure".into(),
            artifact_url: "x".into(),
            description: "d".into(),
            author: "t".into(),
        };
        let artifact = b"@lib\nfn x() -> Int { return 1; }";
        reg.publish(&pkg, artifact).unwrap();

        let json = reg.index_json("demo").unwrap();
        let idx: PackageIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(idx.packages.len(), 1);
        assert_eq!(idx.packages[0].version, "0.1.0");
        assert_eq!(
            idx.packages[0].checksum.as_deref(),
            Some(sha256_hex_registry(artifact).as_str())
        );
        assert!(base.join("demo").join("0.1.0.dal").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn local_registry_publish_same_version_overwrites() {
        let base = std::env::temp_dir().join(format!("dalin_reg_srv2_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let reg = LocalRegistry::from_dir(&base);
        let mk = |v: &str| Package {
            name: "p".into(),
            version: v.into(),
            capability: "cpu".into(),
            effect_level: "pure".into(),
            artifact_url: "x".into(),
            description: "d".into(),
            author: "t".into(),
        };
        reg.publish(&mk("1.0.0"), b"v1").unwrap();
        reg.publish(&mk("2.0.0"), b"v2").unwrap();
        reg.publish(&mk("1.0.0"), b"v1-updated").unwrap();
        let idx: PackageIndex = serde_json::from_str(&reg.index_json("p").unwrap()).unwrap();
        assert_eq!(idx.packages.len(), 2);

        let _ = fs::remove_dir_all(&base);
    }
}
