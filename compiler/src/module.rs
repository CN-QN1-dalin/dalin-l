/// Dalin L 3.0 — Phase H: 模块系统 (Module System)
///
/// 支持 `mod foo;` / `mod foo { ... }` / `use foo::bar;` / `pub use` 等语法。
/// 构建模块树、解析路径、冲突检测、拓扑排序。
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ═══════════════════════════════
//  模块节点
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Pub,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone)]
pub struct ModuleDecl {
    pub name: String,
    pub visibility: Visibility,
    pub is_inline: bool,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone)]
pub enum ModuleItem {
    Function(String),
    Struct(String),
    Enum(String),
    Trait(String),
    ImplBlock(Vec<String>),
    Module(ModuleDecl),
    Use(ImportItem),
    Export(String),
    Const(String),
    TypeAlias(String),
}

// ═══════════════════════════════
//  模块树
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct ModuleTree {
    pub root: ModuleNode,
    pub path_index: HashMap<String, Vec<String>>,
}

impl ModuleTree {
    pub fn new(root_name: &str) -> Self {
        Self {
            root: ModuleNode::module(root_name),
            path_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, module: ModuleDecl) {
        match module.visibility {
            Visibility::Pub => {
                self.root.items.push(ModuleItem::Module(ModuleDecl {
                    visibility: Visibility::Pub,
                    ..module.clone()
                }));
            }
            Visibility::Private => {
                self.root.items.push(ModuleItem::Module(module.clone()));
            }
        }
        let module_name = module.name.clone();
        self.path_index.insert(module_name.clone(), vec![module_name]);
    }

    pub fn insert_use(&mut self, imp: ImportItem) {
        if matches!(imp.visibility, Visibility::Pub) {
            self.root.items.push(ModuleItem::Use(imp));
        }
    }

    pub fn register_fn(&mut self, name: &str, _module_path: &[String]) {
        let idx = self.root.items.iter().position(|item| {
            matches!(item, ModuleItem::Function(n) if n == name)
        });
        if idx.is_none() {
            self.root.items.push(ModuleItem::Function(name.to_string()));
        }
    }

    pub fn resolve_path(&self, path: &[String]) -> ResolveResult {
        if path.is_empty() {
            return ResolveResult::NotFound;
        }
        let head = &path[0];

        // Check if it's a known module
        if let Some(full_path) = self.path_index.get(head) {
            if path.len() == 1 {
                return ResolveResult::Resolved(full_path.clone());
            }
            if full_path.len() >= path.len() && &full_path[..path.len()] == path {
                return ResolveResult::Resolved(full_path.clone());
            }
        }

        // Check if it's a symbol in current module
        match self.lookup_symbol(head) {
            Some(symbol) => ResolveResult::Resolved(symbol),
            None => ResolveResult::NotFound,
        }
    }

    fn lookup_symbol(&self, name: &str) -> Option<Vec<String>> {
        for item in &self.root.items {
            match item {
                ModuleItem::Function(fn_name) if fn_name == name => {
                    return Some(vec![self.root.name.clone(), name.to_string()]);
                }
                ModuleItem::Struct(s_name) if s_name == name => {
                    return Some(vec![self.root.name.clone(), name.to_string()]);
                }
                ModuleItem::Enum(e_name) if e_name == name => {
                    return Some(vec![self.root.name.clone(), name.to_string()]);
                }
                ModuleItem::Trait(t_name) if t_name == name => {
                    return Some(vec![self.root.name.clone(), name.to_string()]);
                }
                _ => {}
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolveResult {
    Resolved(Vec<String>),
    NotFound,
    Ambiguous(Vec<Vec<String>>),
}

// ═══════════════════════════════
//  模块节点
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub name: String,
    pub items: Vec<ModuleItem>,
    pub visibility: Visibility,
}

impl ModuleNode {
    pub fn module(name: &str) -> Self {
        Self {
            name: name.to_string(),
            items: Vec::new(),
            visibility: Visibility::Private,
        }
    }

    pub fn exported_items(&self) -> Vec<&str> {
        self.items.iter()
            .filter_map(|item| match item {
                ModuleItem::Function(name) => Some(name.as_str()),
                ModuleItem::Struct(name) => Some(name.as_str()),
                ModuleItem::Enum(name) => Some(name.as_str()),
                ModuleItem::Trait(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

// ═══════════════════════════════
//  依赖图与拓扑排序
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub modules: HashMap<String, Vec<String>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self { modules: HashMap::new() }
    }

    pub fn add_module(&mut self, name: &str, deps: Vec<String>) {
        self.modules.insert(name.to_string(), deps);
    }

    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        for mod_name in self.modules.keys() {
            if self.has_cycle_util(mod_name, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn has_cycle_util(&self, name: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>) -> bool {
        visited.insert(name.to_string());
        rec_stack.insert(name.to_string());

        if let Some(deps) = self.modules.get(name) {
            for dep in deps {
                if !visited.contains(dep) {
                    if self.has_cycle_util(dep, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }

        rec_stack.remove(name);
        false
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        // Initialize all module nodes with in_degree 0
        for mod_name in self.modules.keys() {
            in_degree.entry(mod_name.clone()).or_insert(0);
        }
        // For each module, its deps have an edge FROM the module TO the dep
        // So the dep's in_degree should increase (dep must be loaded before module)
        for (mod_name, deps) in &self.modules {
            for dep in deps {
                // If dep is also a known module, it depends on mod_name being available after it
                // Actually, mod_name REQUIRES dep, so dep must come first
                // In_degree of mod_name increases (it needs its deps loaded first)
                *in_degree.entry(mod_name.clone()).or_insert(0) += 1;
                // Make sure dep has an entry
                in_degree.entry(dep.clone()).or_insert(0);
            }
        }

        let mut queue: Vec<String> = in_degree.iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(k, _)| k.clone())
            .collect();
        queue.sort();

        let mut result = Vec::new();
        while let Some(node) = queue.pop() {
            result.push(node.clone());
            // Remove this node: find all modules that depend on 'node' and decrease their in_degree
            for (mod_name, deps) in &self.modules {
                if deps.contains(&node)
                    && let Some(deg) = in_degree.get_mut(mod_name) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(mod_name.clone());
                            queue.sort();
                        }
                    }
            }
        }

        if result.len() != self.modules.len() {
            Err("Circular dependency detected!".into())
        } else {
            Ok(result)
        }
    }
}

// ═══════════════════════════════
//  命名空间与冲突检测
// ═══════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    pub module: String,
    pub item_type: String,
    pub visibility: Visibility,
}

impl std::fmt::Display for SymbolLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{} ({})", self.module, self.item_type,
            match &self.visibility {
                Visibility::Private => "private",
                Visibility::Pub => "pub",
            })
    }
}

#[derive(Debug, Clone)]
pub struct Namespace {
    pub names: HashMap<String, SymbolLocation>,
}

impl Default for Namespace {
    fn default() -> Self {
        Self::new()
    }
}

impl Namespace {
    pub fn new() -> Self {
        Self { names: HashMap::new() }
    }

    pub fn register(&mut self, name: &str, location: SymbolLocation) -> Result<(), String> {
        if let Some(existing) = self.names.get(name) {
            return Err(format!(
                "命名冲突: '{}' 已经在 {} 中定义，当前位置为 {}",
                name, existing, location
            ));
        }
        self.names.insert(name.to_string(), location);
        Ok(())
    }

    pub fn lookup(&self, name: &str) -> Option<&SymbolLocation> {
        self.names.get(name)
    }

    pub fn merge(&mut self, other: &Namespace) -> Vec<String> {
        let mut conflicts = Vec::new();
        for (name, location) in &other.names {
            if let Some(existing) = self.names.get(name) {
                if existing != location {
                    conflicts.push(name.clone());
                }
            } else {
                self.names.insert(name.clone(), location.clone());
            }
        }
        conflicts
    }

    pub fn check_import_conflicts(&self, imports: &[String], source_module: &str) -> Vec<String> {
        let mut conflicts = Vec::new();
        for import_name in imports {
            if let Some(loc) = self.names.get(import_name)
                && loc.module != source_module {
                    conflicts.push(format!(
                        "{}: 导入 '{}' 与来自 {} 的定义冲突",
                        source_module, import_name, loc.module
                    ));
                }
        }
        conflicts
    }
}

// ═══════════════════════════════
//  模块解析器 (从 .dalin 文件解析)
// ═══════════════════════════════

pub fn parse_module_from_source(source: &str, module_name: &str) -> ModuleDecl {
    let mut items = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") { continue; }

        if let Some(body) = line.strip_prefix("pub mod ") {
            let name = body.trim_end_matches(';').trim_end_matches('{');
            items.push(ModuleItem::Module(ModuleDecl {
                name: name.to_string(), visibility: Visibility::Pub,
                is_inline: line.contains('{'), items: Vec::new(),
            }));
        } else if let Some(body) = line.strip_prefix("mod ") {
            let name = body.trim_end_matches(';').trim_end_matches('{');
            items.push(ModuleItem::Module(ModuleDecl {
                name: name.to_string(), visibility: Visibility::Private,
                is_inline: line.contains('{'), items: Vec::new(),
            }));
        } else if let Some(body) = line.strip_prefix("pub use ") {
            let path: Vec<String> = body.trim_end_matches(';').split("::").map(|s| s.to_string()).collect();
            items.push(ModuleItem::Use(ImportItem {
                path, alias: None, visibility: Visibility::Pub,
            }));
        } else if let Some(body) = line.strip_prefix("use ") {
            let path: Vec<String> = body.trim_end_matches(';').split("::").map(|s| s.to_string()).collect();
            items.push(ModuleItem::Use(ImportItem {
                path, alias: None, visibility: Visibility::Private,
            }));
        } else if let Some(body) = line.strip_prefix("pub fn ").or_else(|| line.strip_prefix("fn ")) {
            let name = extract_name(body);
            items.push(ModuleItem::Function(name));
        } else if let Some(body) = line.strip_prefix("pub struct ").or_else(|| line.strip_prefix("struct ")) {
            let name = extract_name(body);
            items.push(ModuleItem::Struct(name));
        } else if let Some(body) = line.strip_prefix("pub enum ").or_else(|| line.strip_prefix("enum ")) {
            let name = extract_name(body);
            items.push(ModuleItem::Enum(name));
        } else if let Some(body) = line.strip_prefix("pub trait ").or_else(|| line.strip_prefix("trait ")) {
            let name = extract_name(body);
            items.push(ModuleItem::Trait(name));
        } else if let Some(body) = line.strip_prefix("pub const ").or_else(|| line.strip_prefix("const ")) {
            let name = extract_name(body);
            items.push(ModuleItem::Const(name));
        } else if line.starts_with("type ") {
            let name = extract_name(line.strip_prefix("type ").unwrap_or_default());
            items.push(ModuleItem::TypeAlias(name));
        }
    }
    ModuleDecl {
        name: module_name.to_string(), visibility: Visibility::Private,
        is_inline: false, items,
    }
}

fn extract_name(s: &str) -> String {
    let s = s.trim();
    s.find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|pos| s[..pos].to_string())
        .unwrap_or_else(|| s.to_string())
}

// ═══════════════════════════════
//  ModuleResolver: 文件系统模块加载
// ═══════════════════════════════

#[derive(Debug, Clone)]
pub struct ModuleResolver {
    pub base_dir: PathBuf,
    pub loaded_modules: HashMap<String, ModuleDecl>,
    pub module_paths: HashMap<String, PathBuf>,
}

impl ModuleResolver {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir, loaded_modules: HashMap::new(), module_paths: HashMap::new() }
    }

    pub fn resolve(&mut self, module_name: &str) -> Result<&ModuleDecl, String> {
        if self.loaded_modules.contains_key(module_name) {
            return Ok(self.loaded_modules.get(module_name).ok_or_else(|| format!("module '{}' not found in cache", module_name))?);
        }
        // Mock: create empty module declaration
        let decl = ModuleDecl {
            name: module_name.to_string(),
            visibility: Visibility::Private,
            is_inline: false,
            items: Vec::new(),
        };
        let path = self.base_dir.join(format!("{}.dalin", module_name));
        self.loaded_modules.insert(module_name.to_string(), decl);
        self.module_paths.insert(module_name.to_string(), path);
        Ok(self.loaded_modules.get(module_name).ok_or_else(|| format!("module '{}' not found after insert", module_name))?)
    }

    pub fn resolve_all(&mut self, module_name: &str) -> Result<Vec<String>, String> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        self.resolve_recursive(module_name, &mut order, &mut visited)?;
        Ok(order)
    }

    fn resolve_recursive(
        &mut self,
        module_name: &str,
        order: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if !visited.insert(module_name.to_string()) {
            return Ok(());
        }
        let decl = self.resolve(module_name)?;
        // Collect sub-module names to resolve
        let sub_names: Vec<String> = decl.items.iter()
            .filter_map(|item| {
                if let ModuleItem::Module(sub) = item {
                    Some(sub.name.clone())
                } else {
                    None
                }
            }).collect();
        for sub_name in sub_names {
            self.resolve_recursive(&sub_name, order, visited)?;
        }
        order.push(module_name.to_string());
        Ok(())
    }

    pub fn list_modules(&self) -> Vec<String> {
        let mut mods: Vec<String> = self.loaded_modules.keys().cloned().collect();
        mods.sort();
        mods
    }
}

// ═══════════════════════════════
//  单元测试
// ═══════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_tree_creation() {
        let tree = ModuleTree::new("main");
        assert_eq!(tree.root.name, "main");
        assert!(tree.root.items.is_empty());
    }

    #[test]
    fn test_insert_module() {
        let mut tree = ModuleTree::new("main");
        tree.insert(ModuleDecl {
            name: "utils".to_string(), visibility: Visibility::Private,
            is_inline: false, items: Vec::new(),
        });
        assert_eq!(tree.root.items.len(), 1);
        assert!(tree.path_index.contains_key("utils"));
    }

    #[test]
    fn test_register_function() {
        let mut tree = ModuleTree::new("main");
        tree.register_fn("add", &["main".to_string()]);
        assert!(tree.root.items.iter().any(|item| {
            matches!(item, ModuleItem::Function(name) if name == "add")
        }));
    }

    #[test]
    fn test_resolve_known_module() {
        let mut tree = ModuleTree::new("main");
        tree.insert(ModuleDecl {
            name: "std".to_string(), visibility: Visibility::Pub,
            is_inline: false, items: Vec::new(),
        });
        let result = tree.resolve_path(&["std".to_string()]);
        assert!(matches!(result, ResolveResult::Resolved(_)));
    }

    #[test]
    fn test_resolve_unknown_path() {
        let tree = ModuleTree::new("main");
        let result = tree.resolve_path(&["unknown_module".to_string()]);
        assert!(matches!(result, ResolveResult::NotFound));
    }

    #[test]
    fn test_dependency_graph_simple() {
        let mut graph = DependencyGraph::new();
        graph.add_module("main", vec!["core".to_string(), "utils".to_string()]);
        graph.add_module("core", vec![]);
        graph.add_module("utils", vec!["core".to_string()]);
        assert!(!graph.has_cycle());
        let sorted = graph.topological_sort().unwrap();
        let core_idx = sorted.iter().position(|m| m == "core").unwrap();
        let main_idx = sorted.iter().position(|m| m == "main").unwrap();
        assert!(core_idx < main_idx);
    }

    #[test]
    fn test_dependency_graph_cycle_detection() {
        let mut graph = DependencyGraph::new();
        graph.add_module("a", vec!["b".to_string()]);
        graph.add_module("b", vec!["c".to_string()]);
        graph.add_module("c", vec!["a".to_string()]);
        assert!(graph.has_cycle());
        assert!(graph.topological_sort().is_err());
    }

    #[test]
    fn test_namespace_register_and_lookup() {
        let mut ns = Namespace::new();
        ns.register("add", SymbolLocation {
            module: "math".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let loc = ns.lookup("add");
        assert!(loc.is_some());
        assert_eq!(loc.unwrap().module, "math");
    }

    #[test]
    fn test_namespace_conflict_detection() {
        let mut ns = Namespace::new();
        ns.register("foo", SymbolLocation {
            module: "a".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let result = ns.register("foo", SymbolLocation {
            module: "b".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("命名冲突"));
    }

    #[test]
    fn test_namespace_merge() {
        let mut ns1 = Namespace::new();
        ns1.register("bar", SymbolLocation {
            module: "module_x".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let mut ns2 = Namespace::new();
        ns2.register("baz", SymbolLocation {
            module: "module_y".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let conflicts = ns1.merge(&ns2);
        assert!(conflicts.is_empty());
        assert!(ns1.lookup("bar").is_some());
        assert!(ns1.lookup("baz").is_some());
    }

    #[test]
    fn test_namespace_merge_with_conflict() {
        let mut ns1 = Namespace::new();
        ns1.register("shared", SymbolLocation {
            module: "alpha".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let mut ns2 = Namespace::new();
        ns2.register("shared", SymbolLocation {
            module: "beta".to_string(), item_type: "fn".to_string(),
            visibility: Visibility::Private,
        }).unwrap();
        let conflicts = ns1.merge(&ns2);
        assert!(conflicts.contains(&"shared".to_string()));
    }

    #[test]
    fn test_parse_simple_module() {
        let source = "\
mod utils;\npub mod core;\nfn helper() { }\npub fn main() -> int { return 0 }\nstruct Point { x: int, y: int }\nenum Color { Red, Green, Blue }\n";
        let decl = parse_module_from_source(source, "my_mod");
        assert_eq!(decl.name, "my_mod");
        assert!(decl.items.iter().any(|item| {
            matches!(item, ModuleItem::Module(m) if m.name == "utils")
        }));
        assert!(decl.items.iter().any(|item| {
            matches!(item, ModuleItem::Module(m) if m.name == "core" && matches!(m.visibility, Visibility::Pub))
        }));
        assert!(decl.items.iter().any(|item| {
            matches!(item, ModuleItem::Function(name) if name == "helper")
        }));
    }

    #[test]
    fn test_parse_use_statement() {
        let source = "use std::vec::Vec;\npub use std::option::Option;";
        let decl = parse_module_from_source(source, "test_mod");
        assert!(decl.items.iter().any(|item| {
            matches!(item, ModuleItem::Use(imp) if imp.path.last() == Some(&"Vec".to_string()))
        }));
    }

    #[test]
    fn test_resolver_list_modules_empty() {
        let resolver = ModuleResolver::new(PathBuf::from("/tmp/test"));
        assert!(resolver.list_modules().is_empty());
    }

    #[test]
    fn test_resolver_cached_module() {
        let mut resolver = ModuleResolver::new(PathBuf::from("/tmp/test"));
        let mock_decl = ModuleDecl {
            name: "cached".to_string(), visibility: Visibility::Private,
            is_inline: false, items: Vec::new(),
        };
        resolver.loaded_modules.insert("cached".to_string(), mock_decl);
        let mods = resolver.list_modules();
        assert_eq!(mods, vec!["cached".to_string()]);
    }

    #[test]
    fn test_visibility_equality() {
        assert_eq!(Visibility::Private, Visibility::Private);
        assert_eq!(Visibility::Pub, Visibility::Pub);
        assert_ne!(Visibility::Private, Visibility::Pub);
    }

    #[test]
    fn test_module_node_exported_items() {
        let node = ModuleNode {
            name: "math".to_string(),
            items: vec![
                ModuleItem::Function("add".to_string()),
                ModuleItem::Function("sub".to_string()),
                ModuleItem::Struct("Point".to_string()),
                ModuleItem::Module(ModuleDecl {
                    name: "internal".to_string(), visibility: Visibility::Private,
                    is_inline: false, items: vec![],
                }),
            ],
            visibility: Visibility::Private,
        };
        let exported = node.exported_items();
        assert_eq!(exported.len(), 3);
        assert!(exported.contains(&"add"));
        assert!(exported.contains(&"sub"));
        assert!(exported.contains(&"Point"));
    }

    #[test]
    fn test_resolve_result_cloning() {
        let resolved = ResolveResult::Resolved(vec!["std".to_string(), "vec".to_string()]);
        let cloned = resolved.clone();
        assert_eq!(resolved, cloned);
        assert_eq!(ResolveResult::NotFound, ResolveResult::NotFound);
    }

    #[test]
    fn test_skip_comments_and_empty_lines() {
        let source = "// comment\nfn foo() { }\n\nfn bar() { }";
        let decl = parse_module_from_source(source, "comment_test");
        let fn_count = decl.items.iter()
            .filter(|item| matches!(item, ModuleItem::Function(_)))
            .count();
        assert_eq!(fn_count, 2);
    }

    #[test]
    fn test_symbol_location_display() {
        let loc = SymbolLocation {
            module: "math".to_string(),
            item_type: "fn".to_string(),
            visibility: Visibility::Pub,
        };
        let display = format!("{}", loc);
        assert!(display.contains("math"));
        assert!(display.contains("pub"));
    }
}
