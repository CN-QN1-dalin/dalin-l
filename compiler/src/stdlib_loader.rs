/// Dalin L 3.0 — 标准库加载器 (Standard Library Loader)
///
/// 负责从 dalin.toml 配置中读取 stdlib_path，按模块名查找对应的 .dal 文件，
/// 将内容解析为 AST 片段并注入当前作用域。
///
/// 使用方式：
///   let loader = StdLibLoader::new("/path/to/project")?;
///   let injected = loader.load_all()?;          // 一次性加载全部
///   let core_ast = loader.load_module("core_types")?;  // 按需加载单个模块
use crate::ast::*;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::task_spec::TaskSpec;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// ═══════════════════════════════
//  配置解析
// ═══════════════════════════════

/// dalin.toml 中与标准库相关的配置
#[derive(Debug, Clone)]
pub struct StdLibConfig {
    /// 标准库根目录路径（绝对路径或相对于项目根目录）
    pub stdlib_path: PathBuf,
    /// 自动导入的预置模块列表（来自 prelude.dal 声明）
    pub prelude_modules: Vec<String>,
}

impl StdLibConfig {
    /// 从 PackageManifest 构建配置
    pub fn from_manifest(manifest: &PackageManifest) -> Self {
        let stdlib_path = PathBuf::from(
            manifest
                .stdlib_modules
                .first()
                .map(|s| s.as_str())
                .unwrap_or("stdlib"),
        );

        Self {
            stdlib_path,
            prelude_modules: vec!["prelude".to_string(), "core_types".to_string()],
        }
    }

    /// 从 dalin.toml 文件内容解析（如果包含 `[stdlib]` 段落）
    pub fn from_toml(content: &str, project_root: &Path) -> Result<Self, String> {
        let mut stdlib_path = PathBuf::from("stdlib");
        let mut prelude = vec!["prelude".to_string(), "core_types".to_string()];

        let mut in_stdlib_section = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[stdlib]" {
                in_stdlib_section = true;
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_stdlib_section = false;
                continue;
            }
            if !in_stdlib_section {
                continue;
            }

            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim();
                let value = trimmed[eq_pos + 1..].trim().trim_matches('"');
                match key {
                    "path" => stdlib_path = PathBuf::from(value),
                    "prelude" => {
                        let mods: Vec<String> =
                            value.split(',').map(|s| s.trim().to_string()).collect();
                        prelude = mods;
                    }
                    _ => {}
                }
            }
        }

        // 将路径解析为相对于项目根目录的绝对路径
        let stdlib_path = if stdlib_path.is_absolute() {
            stdlib_path
        } else {
            project_root.join(stdlib_path)
        };

        Ok(Self {
            stdlib_path,
            prelude_modules: prelude,
        })
    }
}

// ═══════════════════════════════
//  标准库加载器
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct StdLibLoader {
    /// 项目根目录
    pub project_root: PathBuf,
    /// 标准库配置
    pub config: StdLibConfig,
    /// 已缓存的 AST（模块名 → 已解析的程序片）
    cache: HashMap<String, Program>,
    /// 已加载的模块名集合（避免重复加载 + 循环依赖检测）
    loaded: HashSet<String>,
}

impl StdLibLoader {
    /// 从项目根目录初始化加载器
    pub fn new(project_root: PathBuf) -> Result<Self, String> {
        let manifest_file = project_root.join("dalin.toml");

        let config = if manifest_file.exists() {
            let content = fs::read_to_string(&manifest_file)
                .map_err(|e| format!("读取 dalin.toml 失败: {}", e))?;
            // 先尝试从 [stdlib] 段解析
            if content.contains("[stdlib]") {
                StdLibConfig::from_toml(&content, &project_root)?
            } else {
                // 回退：使用默认 stdlib_path
                StdLibConfig {
                    stdlib_path: project_root.join("stdlib"),
                    prelude_modules: vec!["prelude".to_string(), "core_types".to_string()],
                }
            }
        } else {
            // 没有 dalin.toml，使用默认配置
            StdLibConfig {
                stdlib_path: project_root.join("stdlib"),
                prelude_modules: vec!["prelude".to_string(), "core_types".to_string()],
            }
        };

        Ok(Self {
            project_root,
            config,
            cache: HashMap::new(),
            loaded: HashSet::new(),
        })
    }

    /// 显式设置配置
    pub fn with_config(mut self, config: StdLibConfig) -> Self {
        self.config = config;
        self
    }

    /// 加载指定模块的 AST
    ///
    /// 查找逻辑：
    ///   1. 检查缓存命中
    ///   2. 在 stdlib_path/<module_name>.dal 查找
    ///   3. 在 stdlib_path/core/<module_name>.dal 查找（子模块）
    ///   4. 如果文件不存在，创建一个空的 Program 占位符
    pub fn load_module(&mut self, module_name: &str) -> Result<(), String> {
        // 缓存命中
        if self.cache.contains_key(module_name) {
            return Ok(());
        }

        // 循环依赖检测
        if self.loaded.contains(module_name) {
            return Err(format!(
                "循环依赖: 正在加载 '{}' 时检测到重复引用",
                module_name
            ));
        }

        // 构建可能的文件路径
        let candidate_paths = vec![
            self.config.stdlib_path.join(format!("{}.dal", module_name)),
            self.config
                .stdlib_path
                .join("core")
                .join(format!("{}.dal", module_name)),
            self.config
                .stdlib_path
                .join(format!("{}.dalin", module_name)),
        ];

        let mut content: Option<String> = None;
        for path in &candidate_paths {
            if path.exists() {
                content = Some(
                    fs::read_to_string(path)
                        .map_err(|e| format!("读取 {}: {}", path.display(), e))?,
                );
                break;
            }
        }

        if content.is_none() {
            let empty_prog = Program::new();
            self.cache.insert(module_name.to_string(), empty_prog);
            self.loaded.insert(module_name.to_string());
            return Ok(());
        }

        self.loaded.insert(module_name.to_string());

        let tokens = {
            let content_str = content.as_ref().ok_or("No content")?;
            let mut lex = Lexer::new(content_str);
            match lex.tokenize() {
                Ok(t) => t,
                Err(e) => {
                    return Err(format!(
                        "{} 词法错误 [{}:{}]: {}",
                        module_name, e.line, e.column, e.message
                    ));
                }
            }
        };

        let mut parser = Parser::new(tokens);
        let prog = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                return Err(format!(
                    "{} 语法错误 [{}:{}]: {}",
                    module_name, e.line, e.column, e.message
                ));
            }
        };

        // 缓存结果（clone 后让 cache 持有所有权）
        let _stmts_count = prog.statements.len();
        let _uses_count = prog.uses.len();
        let _mods_count = prog.modules.len();

        self.cache.insert(module_name.to_string(), prog.clone());

        // resolve_uses 需要 &mut self，但此时已持有 prog 的 clone，没有借住冲突
        drop(content);
        self.resolve_uses(&prog)?;

        Ok(())
    }

    /// 一次性加载所有预置模块（prelude + core_types + 其他标记为 auto 的）
    pub fn load_prelude(&mut self) -> Result<Vec<String>, String> {
        let mut loaded = Vec::new();
        // Clone to avoid borrow conflict with mutable self.load_module()
        let prelude_mods: Vec<String> = self.config.prelude_modules.clone();
        for mod_name in &prelude_mods {
            self.load_module(mod_name)?;
            loaded.push(mod_name.clone());
        }
        Ok(loaded)
    }

    /// 加载全部标准库模块（扫描 stdlib_path 下的所有 .dal 文件）
    pub fn load_all(&mut self) -> Result<Vec<String>, String> {
        if !self.config.stdlib_path.exists() {
            return Err(format!(
                "标准库目录不存在: {}",
                self.config.stdlib_path.display()
            ));
        }

        let mut loaded = Vec::new();
        for entry in fs::read_dir(&self.config.stdlib_path)
            .map_err(|e| format!("读取标准库目录失败: {}", e))?
        {
            let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
            let path = entry.path();
            if let Some(ext) = path.extension()
                && ext == "dal"
                && let Some(file_name) = path.file_stem().and_then(|f| f.to_str())
            {
                self.load_module(file_name)?;
                loaded.push(file_name.to_string());
            }
        }

        loaded.sort();
        Ok(loaded)
    }

    /// 将加载的所有标准库模块合并到当前程序中
    pub fn merge_into_program(&mut self, target: &mut Program) -> Result<usize, String> {
        // 确保预置模块已加载
        self.load_prelude()?;

        let mut merged_count = 0;

        for (mod_name, prog) in &self.cache {
            for stmt in &prog.statements {
                target.add(stmt.clone());
                merged_count += 1;
            }
            // 也注入模块声明
            for module_decl in &prog.modules {
                // 在 AST 层面记录来源模块
                let imported_stmt = Stmt::Export(format!(
                    "{}_imported_from:{}",
                    mod_name,
                    match module_decl {
                        ModuleDecl::External(n) => n.clone(),
                        ModuleDecl::Inline(n, _) => n.clone(),
                    }
                ));
                target.add(imported_stmt);
            }
        }

        Ok(merged_count)
    }

    /// 解析程序中的 use 语句，递归加载所需的标准库模块
    fn resolve_uses(&mut self, prog: &Program) -> Result<(), String> {
        for stmt in &prog.statements {
            if let Stmt::Use(path) = stmt {
                // 解析 use 路径：可能是 "core_types" 或 "std::vec" 或 "core_types::Option"
                let parts: Vec<&str> = path.split("::").collect();
                let top_level_mod = parts[0];

                // 跳过已在预置中的模块（避免死循环）
                if self
                    .config
                    .prelude_modules
                    .iter()
                    .any(|p| p == top_level_mod)
                {
                    continue;
                }

                // 如果尚未缓存，递归加载
                if !self.cache.contains_key(top_level_mod) {
                    self.load_module(top_level_mod)?;
                }
            }
        }

        // 也检查 program 中直接声明的 modules
        for module_decl in &prog.modules {
            if let ModuleDecl::External(name) = module_decl
                && !self.cache.contains_key(name)
            {
                self.load_module(name)?;
            }
        }

        Ok(())
    }

    /// 获取某个模块的 TaskSpec 列表（用于控制面调度）
    pub fn get_task_specs(&self, module_name: &str) -> Option<Vec<TaskSpec>> {
        let prog = self.cache.get(module_name)?;
        Some(crate::task_spec::from_program(prog))
    }

    /// 获取所有已加载模块的统计信息
    pub fn stats(&self) -> StdLibStats {
        StdLibStats {
            total_loaded: self.loaded.len(),
            cached_modules: self.cache.len(),
            prelude_modules: self.config.prelude_modules.clone(),
            stdlib_root: self.config.stdlib_path.clone(),
        }
    }
}

// ═══════════════════════════════
//  工具函数：从 dalin.toml 读取 stdlib_path
// ═══════════════════════════════

/// 从 dalin.toml 中提取 stdlib_path 配置项
pub fn read_stdlib_path(project_root: &Path) -> Option<PathBuf> {
    let manifest = project_root.join("dalin.toml");
    if !manifest.exists() {
        return None;
    }

    let content = fs::read_to_string(&manifest).ok()?;
    let mut stdlib_path = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if (trimmed.starts_with("stdlib_path") || trimmed.starts_with("stdlib"))
            && let Some(eq_pos) = trimmed.find('=')
        {
            let value = trimmed[eq_pos + 1..].trim().trim_matches('"');
            stdlib_path = Some(PathBuf::from(value));
            break;
        }
    }

    stdlib_path
}

// ═══════════════════════════════
//  标准库统计信息
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct StdLibStats {
    pub total_loaded: usize,
    pub cached_modules: usize,
    pub prelude_modules: Vec<String>,
    pub stdlib_root: PathBuf,
}

impl std::fmt::Display for StdLibStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== 标准库状态 ===")?;
        writeln!(f, "  标准库根目录: {}", self.stdlib_root.display())?;
        writeln!(f, "  已加载模块数: {}", self.total_loaded)?;
        writeln!(f, "  缓存 AST 数: {}", self.cached_modules)?;
        writeln!(f, "  预置模块: {:?}", self.prelude_modules)?;
        Ok(())
    }
}

// ═══════════════════════════════
//  单元测试
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_module_returns_empty() {
        let mut loader = StdLibLoader::new(PathBuf::from("/tmp")).expect("should create loader");
        let result = loader.load_module("nonexistent_module_xyz");
        // 应返回成功（空 Program）而非错误
        assert!(result.is_ok());
        assert!(
            loader
                .cache
                .get("nonexistent_module_xyz")
                .map(|p| p.is_empty())
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_stats_display() {
        let mut loader = StdLibLoader::new(PathBuf::from("/tmp")).expect("should create loader");
        let _ = loader.load_module("dummy_for_stats");
        let stats = loader.stats();
        let display = format!("{}", stats);
        assert!(display.contains("标准库"));
    }

    #[test]
    fn test_cache_prevents_duplicate_load() {
        let mut loader = StdLibLoader::new(PathBuf::from("/tmp")).expect("should create loader");

        // 两次加载同一模块不应出错
        let _ = loader.load_module("dummy_test2");
        let _ = loader.load_module("dummy_test2");
        assert_eq!(loader.cache.len(), 1);
    }

    #[test]
    fn test_load_module_invalidates_with_cache() {
        let mut loader = StdLibLoader::new(PathBuf::from("/tmp")).expect("should create loader");
        let _ = loader.load_module("cache_test");
        assert!(loader.cache.contains_key("cache_test"));
        assert!(loader.loaded.contains("cache_test"));
    }
}
