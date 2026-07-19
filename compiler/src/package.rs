/// Dalin L 3.0 — Phase H: 包管理系统 (Package Manager)
///
/// 解析 `dalin.toml`、SemVer 版本解析与比较、依赖解析、缓存机制。
/// 参考 Cargo 的设计，但简化为 DALin L 的最小可用子集。
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

// ═══════════════════════════════
//  SemVer 版本号
// ═══════════════════════════════

/// Semantic Versioning: MAJOR.MINOR.PATCH
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// 从字符串解析 "1.2.3"
    pub fn parse(version_str: &str) -> Result<Self, String> {
        let parts: Vec<&str> = version_str.trim().split('.').collect();
        if parts.len() < 2 || parts.len() > 3 {
            return Err(format!(
                "Invalid SemVer: '{}'. Expected MAJOR[.MINOR[.PATCH]]",
                version_str
            ));
        }
        let major: u64 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid major version: '{}'", parts[0]))?;
        let minor: u64 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid minor version: '{}'", parts[1]))?;
        let patch = if parts.len() == 3 {
            // Parse the leading numeric portion; ignore any pre-release/build suffix like "-alpha"
            let num_part: String = parts[2]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if num_part.is_empty() {
                // No leading digits at all (e.g. "alpha"), default to 0
                0
            } else {
                num_part.parse::<u64>().unwrap_or(0)
            }
        } else {
            0
        };
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// 比较两个版本: -1 (小于), 0 (等于), 1 (大于)
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(&self, other: &SemVer) -> i32 {
        if self.major != other.major {
            return (self.major as i32) - (other.major as i32);
        }
        if self.minor != other.minor {
            return (self.minor as i32) - (other.minor as i32);
        }
        (self.patch as i32) - (other.patch as i32)
    }

    /// 检查是否满足版本要求
    pub fn satisfies(&self, requirement: &VersionRequirement) -> bool {
        match requirement {
            VersionRequirement::Exact(req) => self == req,
            VersionRequirement::EqualOrAbove(req) => self.cmp(req) >= 0,
            VersionRequirement::Caret(req) => {
                // ^req: 允许任意更高的次版本/补丁，直到下一主版本
                if self.major != req.major {
                    return false;
                }
                self.cmp(req) >= 0
            }
            VersionRequirement::Tilde(req) => {
                // ~req: 允许相同主版本和次版本的任意补丁
                if self.major != req.major || self.minor != req.minor {
                    return false;
                }
                self.patch >= req.patch
            }
            VersionRequirement::Any => true,
        }
    }

    pub fn display(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 版本要求匹配模式
#[derive(Debug, Clone)]
pub enum VersionRequirement {
    /// ==1.2.3
    Exact(SemVer),
    /// >=1.2.3
    EqualOrAbove(SemVer),
    /// ^1.2.3 (同主版本)
    Caret(SemVer),
    /// ~1.2.3 (同主版本同次版本)
    Tilde(SemVer),
    /// * / 无限制
    Any,
}

// ═══════════════════════════════
//  dalin.toml 解析
// ═══════════════════════════════

/// dalin.toml 包配置文件结构
#[derive(Debug, Clone)]
pub struct PackageManifest {
    pub name: String,
    pub version: SemVer,
    pub edition: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    /// 依赖区域 (`[dependencies]`)
    pub deps: HashMap<String, DependencyEntry>,
    /// `[dev-dependencies]` 开发依赖
    pub dev_deps: HashMap<String, DependencyEntry>,
    /// 内联标准库模块引用
    pub stdlib_modules: Vec<String>,
    /// 预定义的宏注册
    pub macros: Vec<String>,
}

/// 单个依赖条目
#[derive(Debug, Clone)]
pub struct DependencyEntry {
    pub version: String,
    pub optional: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub source: DependencySource,
}

/// 依赖来源
#[derive(Debug, Clone)]
pub enum DependencySource {
    /// 本地路径依赖
    Path(String),
    /// 远程仓库
    Registry(String),
    /// Git 仓库
    Git(String),
}

impl Default for DependencySource {
    fn default() -> Self {
        Self::Registry("crates.dal.in".to_string())
    }
}

impl Default for DependencyEntry {
    fn default() -> Self {
        Self {
            version: "*".to_string(),
            optional: false,
            default_features: true,
            features: Vec::new(),
            source: DependencySource::default(),
        }
    }
}

/// 简单 TOML 解析器 (仅支持 dalin.toml 的子集)
pub fn parse_package_manifest(content: &str) -> Result<PackageManifest, String> {
    let mut manifest = PackageManifest {
        name: String::new(),
        version: SemVer::new(0, 1, 0),
        edition: "2024".to_string(),
        description: None,
        authors: Vec::new(),
        license: None,
        deps: HashMap::new(),
        dev_deps: HashMap::new(),
        stdlib_modules: Vec::new(),
        macros: Vec::new(),
    };

    let mut current_section: Option<String> = None;
    let mut current_subsection: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Section header: [package] or [dependencies]
        if line.starts_with('[') && line.ends_with(']') {
            let section_name = &line[1..line.len() - 1];
            current_section = Some(section_name.to_string());
            current_subsection = None;
            continue;
        }

        // Subsection: [dependencies.foo]
        if line.starts_with("[") {
            let bracket_end = line.find(']').ok_or("Invalid subsection syntax")?;
            let subsection = &line[1..bracket_end];
            current_subsection = Some(subsection.to_string());
            continue;
        }

        // Key-value pair: key = value
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim();
            let value = line[eq_pos + 1..].trim();

            match (current_section.as_deref(), key) {
                (Some("package"), "name") => manifest.name = strip_toml_string(value),
                (Some("package"), "version") => {
                    manifest.version = SemVer::parse(&strip_toml_string(value))?
                }
                (Some("package"), "edition") => manifest.edition = strip_toml_string(value),
                (Some("package"), "description") => {
                    manifest.description = Some(strip_toml_string(value))
                }
                (Some("package"), "authors") => {
                    // Parse array ["Author One", "Author Two"]
                    let items = parse_toml_array(value);
                    manifest.authors = items.into_iter().map(|s| strip_toml_string(&s)).collect();
                }
                (Some("package"), "license") => manifest.license = Some(strip_toml_string(value)),

                (Some("dependencies"), _) => {
                    let dep_entry = parse_dep_entry(key, value, &current_subsection);
                    manifest.deps.insert(key.to_string(), dep_entry);
                }

                (Some("dev-dependencies"), _) => {
                    let dep_entry = parse_dep_entry(key, value, &current_subsection);
                    manifest.dev_deps.insert(key.to_string(), dep_entry);
                }

                _ => {} // Ignore unknown keys/sections
            }
        }
    }

    if manifest.name.is_empty() {
        return Err("dalin.toml must contain [package] name".into());
    }

    Ok(manifest)
}

fn strip_toml_string(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn parse_toml_array(s: &str) -> Vec<String> {
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|item| item.trim().to_string())
        .collect()
}

fn parse_dep_entry(_key: &str, value: &str, _subsection: &Option<String>) -> DependencyEntry {
    let value = value.trim();

    // Simple form: "version = \"1.0\""
    if value.contains('=') {
        let mut entry = DependencyEntry::default();

        for part in value.split(',') {
            let part = part.trim();
            if let Some(eq_pos) = part.find('=') {
                let k = part[..eq_pos].trim();
                let v = part[eq_pos + 1..].trim();

                match k {
                    "version" => entry.version = strip_toml_string(v),
                    "optional" => entry.optional = v.parse().unwrap_or(false),
                    "default-features" => entry.default_features = v.parse().unwrap_or(true),
                    "source" => entry.source = DependencySource::Registry(strip_toml_string(v)),
                    _ => {}
                }
            }
        }

        entry
    } else {
        // Simple form: "1.0"
        DependencyEntry {
            version: strip_toml_string(value),
            ..DependencyEntry::default()
        }
    }
}

// ═══════════════════════════════
//  依赖解析器
// ═══════════════════════════════

/// 依赖图: 包名 → 包信息
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub packages: HashMap<String, PackageInfo>,
    pub resolved: HashMap<String, SemVer>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            resolved: HashMap::new(),
        }
    }

    /// 添加一个已知包
    pub fn add_package(&mut self, name: String, info: PackageInfo) {
        self.packages.insert(name, info);
    }

    /// 解析所有依赖 (简单的冲突解决)
    pub fn resolve_all(&mut self) -> Result<HashMap<String, SemVer>, String> {
        self.resolved.clear();

        for (name, info) in &self.packages {
            if let Some(existing) = self.resolved.get(name) {
                // 版本冲突检测
                if !info.available_versions.iter().any(|v| v == existing) {
                    return Err(format!(
                        "版本冲突: '{}' 需要 {}, 但已有 {}",
                        name,
                        info.available_versions
                            .first()
                            .cloned()
                            .unwrap_or_else(|| SemVer::new(0, 0, 0)),
                        existing
                    ));
                }
            } else {
                // 取最新版本
                let latest = info
                    .available_versions
                    .iter()
                    .max()
                    .ok_or_else(|| format!("包 '{}' 没有可用版本", name))?
                    .clone();
                self.resolved.insert(name.clone(), latest);
            }
        }

        Ok(self.resolved.clone())
    }
}

/// 包的元数据和可用版本
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub description: Option<String>,
    pub available_versions: Vec<SemVer>,
    pub homepage: Option<String>,
}

// ═══════════════════════════════
//  包缓存
// ═══════════════════════════════

/// 包缓存项
#[derive(Debug, Clone)]
pub struct CachedPackage {
    pub name: String,
    pub version: SemVer,
    pub cache_path: String,
    /// 真实下载时间 (Unix 秒)，用于缓存过期与可重现性
    pub downloaded_at: u64,
    /// SHA-256 校验和 (hex)，下载自 registry 时填充；dev 模式为 None
    pub checksum: Option<String>,
}

/// 包管理器: 管理缓存和真实下载
///
/// 通过可插拔的 [`RegistryClient`] 后端实现下载，支持：
/// - `file://` / 本地目录 → [`LocalRegistryClient`] (离线、私有 registry，真实落盘 + SHA-256)
/// - `http(s)://` → [`HttpRegistryClient`] (真实网络 registry，索引 + 下载 + 校验)
/// - dev 模式 → [`InMemoryRegistryClient`] (mock，不落盘)
#[derive(Debug)]
pub struct PackageManager {
    pub cache_dir: String,
    pub cached_packages: HashMap<String, CachedPackage>,
    pub registry_url: String,
    pub dev_mode: bool,
    client: Box<dyn RegistryClient + Send>,
}

impl PackageManager {
    pub fn new(cache_dir: String, registry_url: String) -> Self {
        let client = make_client(&registry_url);
        Self {
            cache_dir,
            cached_packages: HashMap::new(),
            registry_url,
            dev_mode: false,
            client,
        }
    }

    /// 切换到本地开发模式 (mock 后端，不落盘)
    pub fn enable_dev_mode(&mut self) {
        self.dev_mode = true;
        self.client = Box::new(InMemoryRegistryClient);
    }

    /// 切换回远程仓库模式 (按 registry_url 重建后端)
    pub fn disable_dev_mode(&mut self) {
        self.dev_mode = false;
        self.client = make_client(&self.registry_url);
    }

    /// 获取包: 缓存命中直接返回，否则委托给后端真实下载
    pub fn get_package(&mut self, name: &str, version: &SemVer) -> Result<CachedPackage, String> {
        // 1. 缓存命中
        let cache_key = format!("{}@{}", name, version);
        if let Some(cached) = self.cached_packages.get(&cache_key) {
            return Ok(cached.clone());
        }

        // 2. dev 模式: 返回 mock，不落盘
        if self.dev_mode {
            let pkg = CachedPackage {
                name: name.to_string(),
                version: version.clone(),
                cache_path: format!("./dev/packages/{}", name),
                downloaded_at: now(),
                checksum: None,
            };
            self.cached_packages.insert(cache_key, pkg.clone());
            return Ok(pkg);
        }

        // 3. 真实下载 (经由可插拔后端)
        let pkg = self.client.download(name, version, &self.cache_dir)?;
        self.cached_packages.insert(cache_key, pkg.clone());
        Ok(pkg)
    }

    /// 获取缓存中的包列表
    pub fn list_cached(&self) -> Vec<String> {
        let mut pkgs: Vec<String> = self
            .cached_packages
            .values()
            .map(|p| format!("{} v{}", p.name, p.version))
            .collect();
        pkgs.sort();
        pkgs
    }

    /// 清除过期缓存
    pub fn clean_cache(&mut self, max_age_seconds: u64) -> usize {
        let now_ts = now();
        let original_len = self.cached_packages.len();
        self.cached_packages
            .retain(|_, pkg| (now_ts.saturating_sub(pkg.downloaded_at)) < max_age_seconds);
        original_len - self.cached_packages.len()
    }
}

// ═══════════════════════════════
//  Registry 后端 (可插拔)
// ═══════════════════════════════

/// registry 索引中的单个版本引用 (JSON 可序列化)
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

impl PackageVersionRef {
    pub fn semver(&self) -> Result<SemVer, String> {
        SemVer::parse(&self.version)
    }
}

/// registry 索引 (某包的全部可用版本)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageIndex {
    pub packages: Vec<PackageVersionRef>,
}

/// 可插拔的 registry 客户端抽象
///
/// 所有网络/IO 细节收敛到实现中，[`PackageManager`] 只依赖此 trait，
/// 便于在测试、离线、私有 registry 之间切换。
pub trait RegistryClient: std::fmt::Debug {
    /// 获取某包的可用版本索引
    fn fetch_index(&self, name: &str) -> Result<Vec<PackageVersionRef>, String>;
    /// 下载指定版本到 `cache_dir`，返回落盘路径与 SHA-256 校验和
    fn download(&self, name: &str, version: &SemVer, cache_dir: &str) -> Result<CachedPackage, String>;
}

/// dev 模式后端: 不落盘、不校验，仅返回占位缓存项
#[derive(Debug)]
pub struct InMemoryRegistryClient;

impl RegistryClient for InMemoryRegistryClient {
    fn fetch_index(&self, _name: &str) -> Result<Vec<PackageVersionRef>, String> {
        Ok(Vec::new())
    }

    fn download(&self, name: &str, version: &SemVer, _cache_dir: &str) -> Result<CachedPackage, String> {
        Ok(CachedPackage {
            name: name.to_string(),
            version: version.clone(),
            cache_path: format!("./dev/packages/{}", name),
            downloaded_at: now(),
            checksum: None,
        })
    }
}

/// 本地文件系统 registry 后端 (离线 / 私有 registry)
///
/// 目录布局: `{root}/{name}/index.json` + `{root}/{name}/{version}.dal`
#[derive(Debug)]
pub struct LocalRegistryClient {
    pub root: PathBuf,
}

impl RegistryClient for LocalRegistryClient {
    fn fetch_index(&self, name: &str) -> Result<Vec<PackageVersionRef>, String> {
        let idx_path = self.root.join(name).join("index.json");
        if !idx_path.exists() {
            return Err(format!(
                "local registry: no index for '{}' at {}",
                name,
                idx_path.display()
            ));
        }
        let data = fs::read_to_string(&idx_path)
            .map_err(|e| format!("read index {}: {}", idx_path.display(), e))?;
        let idx: PackageIndex = serde_json::from_str(&data)
            .map_err(|e| format!("parse index {}: {}", idx_path.display(), e))?;
        Ok(idx.packages)
    }

    fn download(&self, name: &str, version: &SemVer, cache_dir: &str) -> Result<CachedPackage, String> {
        let artifact = self.root.join(name).join(format!("{}.dal", version));
        if !artifact.exists() {
            return Err(format!(
                "local registry: artifact missing {}",
                artifact.display()
            ));
        }
        let bytes = fs::read(&artifact)
            .map_err(|e| format!("read artifact {}: {}", artifact.display(), e))?;
        let checksum = sha256_hex(&bytes);

        // 纵深防御: 若索引中声明了 checksum，则校验下载内容完整性
        if let Ok(index) = self.fetch_index(name)
            && let Some(entry) = index
                .iter()
                .find(|p| p.semver().map(|v| &v == version).unwrap_or(false))
            && let Some(expected) = entry.checksum.as_deref()
            && !expected.eq_ignore_ascii_case(&checksum)
        {
            return Err(format!(
                "checksum mismatch for {}/{}: expected {}, got {}",
                name, version, expected, checksum
            ));
        }

        let dest_dir = PathBuf::from(cache_dir).join(name);
        fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("create cache dir {}: {}", dest_dir.display(), e))?;
        let dest = dest_dir.join(format!("{}.dal", version));
        fs::write(&dest, &bytes)
            .map_err(|e| format!("write cache {}: {}", dest.display(), e))?;

        Ok(CachedPackage {
            name: name.to_string(),
            version: version.clone(),
            cache_path: dest.to_string_lossy().to_string(),
            downloaded_at: now(),
            checksum: Some(checksum),
        })
    }
}

/// 真实 HTTP registry 后端 (HTTPS)
///
/// 协议: `GET {base}/index/{name}` → `PackageIndex` (JSON)；
/// `GET {artifact_url}` → 字节流，下载后校验 registry 提供的 SHA-256。
#[derive(Debug)]
pub struct HttpRegistryClient {
    pub base_url: String,
}

impl RegistryClient for HttpRegistryClient {
    fn fetch_index(&self, name: &str) -> Result<Vec<PackageVersionRef>, String> {
        let url = format!("{}/index/{}", self.base_url.trim_end_matches('/'), name);
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("registry fetch_index {} failed: {}", url, e))?;
        let (_, body) = resp.into_parts();
        let mut bytes = Vec::new();
        body.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read body {}: {}", url, e))?;
        let text = String::from_utf8_lossy(&bytes);
        let idx: PackageIndex = serde_json::from_str(&text)
            .map_err(|e| format!("registry index parse {}: {}", url, e))?;
        Ok(idx.packages)
    }

    fn download(&self, name: &str, version: &SemVer, cache_dir: &str) -> Result<CachedPackage, String> {
        let index = self.fetch_index(name)?;
        let entry = index
            .iter()
            .find(|p| p.semver().map(|v| &v == version).unwrap_or(false))
            .ok_or_else(|| format!("registry: version {} of '{}' not found", version, name))?;

        let url = entry.artifact_url.clone();
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("registry download {} failed: {}", url, e))?;
        let (_, body) = resp.into_parts();
        let mut bytes = Vec::new();
        body.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read body {}: {}", url, e))?;

        let checksum = sha256_hex(&bytes);
        if let Some(expected) = entry.checksum.as_deref()
            && !expected.eq_ignore_ascii_case(&checksum)
        {
            return Err(format!(
                "checksum mismatch for {}/{}: expected {}, got {}",
                name, version, expected, checksum
            ));
        }

        let dest_dir = PathBuf::from(cache_dir).join(name);
        fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("create cache dir {}: {}", dest_dir.display(), e))?;
        let dest = dest_dir.join(format!("{}.dal", version));
        fs::write(&dest, &bytes)
            .map_err(|e| format!("write cache {}: {}", dest.display(), e))?;

        Ok(CachedPackage {
            name: name.to_string(),
            version: version.clone(),
            cache_path: dest.to_string_lossy().to_string(),
            downloaded_at: now(),
            checksum: Some(checksum),
        })
    }
}

/// 依据 registry URL 选择后端实现
pub fn make_client(registry_url: &str) -> Box<dyn RegistryClient + Send> {
    let url = registry_url.trim();

    // 显式 file:// 协议
    if let Some(path) = url.strip_prefix("file://") {
        return Box::new(LocalRegistryClient {
            root: PathBuf::from(path),
        });
    }

    // 本地已存在的目录 (绝对路径或相对路径且存在) → 本地 registry
    let as_path = PathBuf::from(url);
    if (as_path.is_absolute() || as_path.exists()) && as_path.is_dir() {
        return Box::new(LocalRegistryClient { root: as_path });
    }

    // 否则按 HTTPS 处理 (无协议前缀时自动补 https://)
    let base = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    Box::new(HttpRegistryClient { base_url: base })
}

// ═══════════════════════════════
//  dalan.lock 锁定文件
// ═══════════════════════════════

/// 锁定文件中的单个依赖条目
#[derive(Debug, Clone)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    pub checksum: Option<String>,
    pub source: String,
}

/// `dalan.lock` — 记录已解析依赖的精确版本与 SHA-256 校验和，保证可重现构建
#[derive(Debug, Clone)]
pub struct Lockfile {
    pub package: String,
    pub version: String,
    pub entries: Vec<LockEntry>,
}

impl Lockfile {
    /// 序列化为带注释的 TOML (手写，零额外依赖)
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# Dalin L Package Lock (auto-generated by `dalib pkg build`)\n");
        out.push_str(&format!("package = \"{}\"\n", self.package));
        out.push_str(&format!("version = \"{}\"\n", self.version));
        if !self.entries.is_empty() {
            out.push('\n');
        }
        for e in &self.entries {
            out.push_str("[[dependencies]]\n");
            out.push_str(&format!("name = \"{}\"\n", e.name));
            out.push_str(&format!("version = \"{}\"\n", e.version));
            let cs = e.checksum.clone().unwrap_or_default();
            out.push_str(&format!("checksum = \"{}\"\n", cs));
            out.push_str(&format!("source = \"{}\"\n", e.source));
            out.push('\n');
        }
        out
    }

    /// 从 TOML 解析 (仅支持本结构约定的子集)
    pub fn from_toml(s: &str) -> Result<Lockfile, String> {
        let mut package = String::new();
        let mut version = String::new();
        let mut entries: Vec<LockEntry> = Vec::new();
        let mut cur: Option<LockEntry> = None;

        for raw in s.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[dependencies]]" {
                if let Some(c) = cur.take() {
                    entries.push(c);
                }
                cur = Some(LockEntry {
                    name: String::new(),
                    version: String::new(),
                    checksum: None,
                    source: String::new(),
                });
                continue;
            }
            if let Some(eq) = line.find('=') {
                let k = line[..eq].trim();
                let v = strip_toml_string(line[eq + 1..].trim());
                match (k, cur.as_mut()) {
                    ("package", None) => package = v,
                    ("version", None) => version = v,
                    ("name", Some(c)) => c.name = v,
                    ("version", Some(c)) => c.version = v,
                    ("checksum", Some(c)) => c.checksum = if v.is_empty() { None } else { Some(v) },
                    ("source", Some(c)) => c.source = v,
                    _ => {}
                }
            }
        }
        if let Some(c) = cur.take() {
            entries.push(c);
        }
        Ok(Lockfile {
            package,
            version,
            entries,
        })
    }
}

// ═══════════════════════════════
//  工具函数
// ═══════════════════════════════

/// 当前 Unix 时间戳 (秒)
pub fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// SHA-256 (hex) — 用于 dalan.lock 校验和与缓存完整性验证
pub fn sha256_hex(bytes: &[u8]) -> String {
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

// ═══════════════════════════════
//  单元测试
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── SemVer 测试 ──

    #[test]
    fn test_semver_parse_valid() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_semver_parse_minor_only() {
        let v = SemVer::parse("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_semver_parse_invalid() {
        assert!(SemVer::parse("invalid").is_err());
        assert!(SemVer::parse("1.2.3.4").is_err());
    }

    #[test]
    fn test_semver_cmp() {
        let v1 = SemVer::new(1, 0, 0);
        let v2 = SemVer::new(2, 0, 0);
        let v3 = SemVer::new(1, 1, 0);
        let v4 = SemVer::new(1, 0, 1);
        let v5 = SemVer::new(1, 0, 1);

        assert!(v1.cmp(&v2) < 0);
        assert!(v1.cmp(&v3) < 0);
        assert!(v2.cmp(&v1) > 0);
        assert!(v3.cmp(&v4) > 0);
        assert_eq!(v4.cmp(&v5), 0);
    }

    #[test]
    fn test_semver_satisfies_exact() {
        let v = SemVer::new(1, 2, 3);
        let req = VersionRequirement::Exact(SemVer::new(1, 2, 3));
        assert!(v.satisfies(&req));

        let req2 = VersionRequirement::Exact(SemVer::new(1, 2, 4));
        assert!(!v.satisfies(&req2));
    }

    #[test]
    fn test_semver_satisfies_caret() {
        let v1 = SemVer::new(1, 3, 0); // ^1.2.3 → yes (same major)
        let v2 = SemVer::new(1, 2, 5); // ^1.2.3 → yes
        let v3 = SemVer::new(2, 0, 0); // ^1.2.3 → no (different major)
        let v4 = SemVer::new(1, 1, 0); // ^1.2.3 → no (below requirement)

        let req = VersionRequirement::Caret(SemVer::new(1, 2, 3));
        assert!(v1.satisfies(&req));
        assert!(v2.satisfies(&req));
        assert!(!v3.satisfies(&req));
        assert!(!v4.satisfies(&req));
    }

    #[test]
    fn test_semver_satisfies_tilde() {
        let v1 = SemVer::new(1, 2, 5); // ~1.2.3 → yes
        let v2 = SemVer::new(1, 2, 3); // ~1.2.3 → yes
        let v3 = SemVer::new(1, 3, 0); // ~1.2.3 → no (different minor)
        let v4 = SemVer::new(2, 2, 3); // ~1.2.3 → no (different major)

        let req = VersionRequirement::Tilde(SemVer::new(1, 2, 3));
        assert!(v1.satisfies(&req));
        assert!(v2.satisfies(&req));
        assert!(!v3.satisfies(&req));
        assert!(!v4.satisfies(&req));
    }

    #[test]
    fn test_semver_satisfies_equal_or_above() {
        let v1 = SemVer::new(1, 2, 5); // >=1.2.3 → yes
        let v2 = SemVer::new(1, 2, 3); // >=1.2.3 → yes
        let v3 = SemVer::new(1, 2, 2); // >=1.2.3 → no

        let req = VersionRequirement::EqualOrAbove(SemVer::new(1, 2, 3));
        assert!(v1.satisfies(&req));
        assert!(v2.satisfies(&req));
        assert!(!v3.satisfies(&req));
    }

    #[test]
    fn test_semver_satisfies_any() {
        let v = SemVer::new(99, 99, 99);
        let req = VersionRequirement::Any;
        assert!(v.satisfies(&req));
    }

    #[test]
    fn test_semver_display() {
        let v = SemVer::new(1, 2, 3);
        assert_eq!(format!("{}", v), "1.2.3");
        assert_eq!(v.display(), "1.2.3");
    }

    // ── dalin.toml 解析测试 ──

    fn parse_toml(content: &str) -> Result<PackageManifest, String> {
        parse_package_manifest(content)
    }

    #[test]
    fn test_parse_minimal_manifest() {
        let toml = r#"
[package]
name = "my-project"
version = "1.0.0"
"#;
        let manifest = parse_toml(toml).expect("parse ok");
        assert_eq!(manifest.name, "my-project");
        assert_eq!(manifest.version, SemVer::new(1, 0, 0));
    }

    #[test]
    fn test_parse_full_manifest() {
        let toml = r#"
[package]
name = "my-project"
version = "2.1.0"
edition = "2024"
description = "A test project"
authors = ["Alice", "Bob"]
license = "MIT"

[dependencies]
serde = { version = "1.0", optional = true, default-features = false }
tokio = "1.0"
rand = "~0.8.4"
"#;
        let manifest = parse_toml(toml).expect("parse ok");
        assert_eq!(manifest.name, "my-project");
        assert_eq!(manifest.version.major, 2);
        assert_eq!(manifest.version.minor, 1);
        assert_eq!(manifest.description, Some("A test project".to_string()));
        assert_eq!(
            manifest.authors,
            vec!["Alice".to_string(), "Bob".to_string()]
        );
        assert_eq!(manifest.license, Some("MIT".to_string()));
        assert_eq!(manifest.deps.len(), 3);
        assert!(manifest.deps.get("serde").unwrap().optional);
        assert!(manifest.deps.get("serde").unwrap().default_features);
    }

    #[test]
    fn test_parse_missing_name() {
        let toml = r#"
[package]
version = "1.0.0"
"#;
        assert!(parse_toml(toml).is_err());
    }

    #[test]
    fn test_parse_invalid_version() {
        let toml = r#"
[package]
name = "bad"
version = "abc"
"#;
        assert!(parse_toml(toml).is_err());
    }

    #[test]
    fn test_parse_with_dev_dependencies() {
        let toml = r#"
[package]
name = "with-dev-deps"
version = "1.0.0"

[dev-dependencies]
mockall = "0.11"
"#;
        let manifest = parse_toml(toml).expect("parse ok");
        assert!(manifest.dev_deps.contains_key("mockall"));
        assert!(!manifest.deps.contains_key("mockall"));
    }

    #[test]
    fn test_parse_skips_unknown_sections() {
        let toml = r#"
[package]
name = "skip-test"
version = "1.0.0"

[build]
rustflags = ["-C", "target-cpu=native"]
"#;
        let manifest = parse_toml(toml).expect("parse ok");
        assert_eq!(manifest.name, "skip-test");
    }

    // ── DependencyGraph 测试 ──

    #[test]
    fn test_dep_graph_resolve_single() {
        let mut graph = DependencyGraph::new();
        graph.add_package(
            "math".to_string(),
            PackageInfo {
                name: "math".to_string(),
                description: Some("Math utilities".to_string()),
                available_versions: vec![
                    SemVer::new(1, 0, 0),
                    SemVer::new(1, 1, 0),
                    SemVer::new(2, 0, 0),
                ],
                homepage: None,
            },
        );

        let resolved = graph.resolve_all().unwrap();
        assert_eq!(resolved.get("math"), Some(&SemVer::new(2, 0, 0)));
    }

    #[test]
    fn test_dep_graph_multiple_packages() {
        let mut graph = DependencyGraph::new();
        graph.add_package(
            "a".to_string(),
            PackageInfo {
                name: "a".to_string(),
                description: None,
                available_versions: vec![SemVer::new(1, 0, 0)],
                homepage: None,
            },
        );
        graph.add_package(
            "b".to_string(),
            PackageInfo {
                name: "b".to_string(),
                description: None,
                available_versions: vec![SemVer::new(0, 5, 0)],
                homepage: None,
            },
        );

        let resolved = graph.resolve_all().unwrap();
        assert_eq!(resolved.len(), 2);
    }

    // ── PackageManager 测试 ──

    #[test]
    fn test_package_manager_dev_mode() {
        let mut pm =
            PackageManager::new("./cache".to_string(), "https://registry.dal.in".to_string());
        pm.enable_dev_mode();

        let pkg = pm
            .get_package("my-lib", &SemVer::new(1, 0, 0))
            .expect("dev get ok");
        assert_eq!(pkg.name, "my-lib");
        assert_eq!(pkg.cache_path, "./dev/packages/my-lib");
    }

    #[test]
    fn test_package_manager_cache_lookup() {
        let mut pm =
            PackageManager::new("./cache".to_string(), "https://registry.dal.in".to_string());

        // First lookup: not found (falls back to dev mode for mock)
        pm.enable_dev_mode();
        let pkg1 = pm
            .get_package("cached-pkg", &SemVer::new(1, 0, 0))
            .expect("ok");

        // Check that it's in the cache now (but dev mode doesn't actually cache)
        // With dev mode, each call returns fresh
        let pkg2 = pm
            .get_package("cached-pkg", &SemVer::new(1, 0, 0))
            .expect("ok");
        assert_eq!(pkg1.name, pkg2.name);
    }

    #[test]
    fn test_package_manager_list_cached() {
        let pm = PackageManager::new("./cache".to_string(), "https://registry.dal.in".to_string());
        assert!(pm.list_cached().is_empty());
    }

    #[test]
    fn test_package_manager_clean_cache_noop() {
        let mut pm =
            PackageManager::new("./cache".to_string(), "https://registry.dal.in".to_string());
        // dev mode adds mock entries but they don't have timestamps, so clean should be safe
        pm.clean_cache(3600);
        // No packages to clean
    }

    // ── Visibility / ImportItem 测试 ──

    #[test]
    fn test_dependency_source_default() {
        let dep = DependencyEntry::default();
        match dep.source {
            DependencySource::Registry(url) => assert_eq!(url, "crates.dal.in"),
            _ => panic!("expected registry source"),
        }
    }

    #[test]
    fn test_cached_package_clone() {
        let pkg = CachedPackage {
            name: "clone-test".to_string(),
            version: SemVer::new(1, 0, 0),
            cache_path: "/tmp/test".to_string(),
            downloaded_at: 12345,
            checksum: Some("abc123".to_string()),
        };
        let cloned = pkg.clone();
        assert_eq!(cloned.name, pkg.name);
        assert_eq!(cloned.version, pkg.version);
        assert_eq!(cloned.checksum, pkg.checksum);
    }

    // ── SHA-256 测试 (NIST 已知向量) ──

    #[test]
    fn test_sha256_nist_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"hello, dalin"),
            "7a04eec34adc170330551790ffa3f4dc972c38991f7b98860ee99761d65f3c47"
        );
    }

    // ── Lockfile 往返测试 ──

    #[test]
    fn test_lockfile_roundtrip() {
        let lock = Lockfile {
            package: "my-proj".to_string(),
            version: "0.1.0".to_string(),
            entries: vec![
                LockEntry {
                    name: "serde".to_string(),
                    version: "1.0.0".to_string(),
                    checksum: Some(
                        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                            .to_string(),
                    ),
                    source: "https://registry.dal.in".to_string(),
                },
                LockEntry {
                    name: "tokio".to_string(),
                    version: "1.2.0".to_string(),
                    checksum: None,
                    source: "file://./local-reg".to_string(),
                },
            ],
        };
        let toml = lock.to_toml();
        let parsed = Lockfile::from_toml(&toml).expect("parse ok");
        assert_eq!(parsed.package, "my-proj");
        assert_eq!(parsed.version, "0.1.0");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].name, "serde");
        assert_eq!(parsed.entries[0].checksum, lock.entries[0].checksum);
        assert_eq!(parsed.entries[1].name, "tokio");
        assert_eq!(parsed.entries[1].checksum, None);
    }

    // ── 本地 registry 端到端测试 (离线) ──

    #[test]
    fn test_local_registry_client() {
        let base = std::env::temp_dir().join(format!("dalin_test_reg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // 构造本地 registry: mylib/1.0.0.dal + index.json
        let artifact = b"fn main() { println(\"hi\"); }";
        let client = LocalRegistryClient {
            root: base.clone(),
        };
        // 手工写入 index + artifact (模拟 registry 服务端已发布)
        let name_dir = base.join("mylib");
        fs::create_dir_all(&name_dir).unwrap();
        fs::write(name_dir.join("1.0.0.dal"), artifact).unwrap();
        let idx = PackageIndex {
            packages: vec![PackageVersionRef {
                name: "mylib".to_string(),
                version: "1.0.0".to_string(),
                artifact_url: "1.0.0.dal".to_string(),
                checksum: Some(sha256_hex(artifact)),
                capability: None,
                effect_level: None,
            }],
        };
        fs::write(
            name_dir.join("index.json"),
            serde_json::to_string_pretty(&idx).unwrap(),
        )
        .unwrap();

        // fetch_index
        let index = client.fetch_index("mylib").expect("fetch index");
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].version, "1.0.0");

        // download → 真实落盘 + SHA-256
        let cache = std::env::temp_dir().join(format!("dalin_test_cache_{}", std::process::id()));
        let _ = fs::remove_dir_all(&cache);
        let pkg = client
            .download("mylib", &SemVer::new(1, 0, 0), &cache.to_string_lossy())
            .expect("download");
        assert!(pkg.checksum.is_some());
        assert_eq!(pkg.checksum.as_deref(), Some(sha256_hex(artifact).as_str()));
        assert!(PathBuf::from(&pkg.cache_path).exists());

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&cache);
    }

    // ── PackageManager 真实下载 (file:// 后端) 测试 ──

    #[test]
    fn test_package_manager_local_download() {
        let base = std::env::temp_dir().join(format!("dalin_pm_reg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let artifact = b"@lib\nfn add(a: Int, b: Int) -> Int { return a + b; }";
        let name_dir = base.join("math");
        fs::create_dir_all(&name_dir).unwrap();
        fs::write(name_dir.join("2.1.0.dal"), artifact).unwrap();
        let idx = PackageIndex {
            packages: vec![PackageVersionRef {
                name: "math".to_string(),
                version: "2.1.0".to_string(),
                artifact_url: "2.1.0.dal".to_string(),
                checksum: Some(sha256_hex(artifact)),
                capability: None,
                effect_level: None,
            }],
        };
        fs::write(
            name_dir.join("index.json"),
            serde_json::to_string_pretty(&idx).unwrap(),
        )
        .unwrap();

        let cache = std::env::temp_dir().join(format!("dalin_pm_cache_{}", std::process::id()));
        let _ = fs::remove_dir_all(&cache);
        let url = format!("file://{}", base.to_string_lossy());
        let mut pm = PackageManager::new(cache.to_string_lossy().to_string(), url);
        let pkg = pm
            .get_package("math", &SemVer::new(2, 1, 0))
            .expect("get_package");
        assert_eq!(pkg.checksum.as_deref(), Some(sha256_hex(artifact).as_str()));
        // 二次获取应命中缓存
        let cached = pm
            .get_package("math", &SemVer::new(2, 1, 0))
            .expect("cached get");
        assert_eq!(cached.cache_path, pkg.cache_path);

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&cache);
    }
}
