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
        Self { packages: HashMap::new() }
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
            .filter(|p| p.name.to_lowercase().contains(&q) || p.description.to_lowercase().contains(&q))
            .collect()
    }

    pub fn list(&self) -> Vec<&Package> {
        self.packages.values().flat_map(|pkgs| pkgs.iter()).collect()
    }
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
}
