//! Dalin L 3.0 — Phase I 标准库实证测试 (Empirical Stdlib Gate)
//!
//! 目标：为标准库 `stdlib/` 下所有 `.dal` 模块建立**解析门禁**，
//! 确保 (1) 每个模块都能独立通过 Lexer + Parser；(2) 整棵标准库
//! 能通过 StdLibLoader 递归解析 `use` 依赖并合并（真实加载路径）。
//!
//! 这是 Phase I "实证测试开发中" 的回归基线：任何词法/语法 drift
//! 或加载器崩溃都会让对应测试 fail，迫使在合并前修复。

use std::fs;
use std::path::{Path, PathBuf};

use dalin_compiler::{lexer, parser, stdlib_loader::StdLibLoader};

/// 工作区根目录下的 stdlib 目录
fn stdlib_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <workspace>/compiler
    let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    compiler_dir
        .parent()
        .expect("compiler crate has a parent (workspace root)")
        .join("stdlib")
}

/// 递归收集所有 `.dal` 文件（含子目录）
fn collect_dal_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("dal") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 解析单个 .dal 文件，返回 Ok(()) 或错误描述
fn parse_dal_file(path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("读取失败: {}", e))?;

    let tokens = lexer::Lexer::new(&src)
        .tokenize()
        .map_err(|e| format!("词法错误 [{}:{}]: {}", e.line, e.column, e.message))?;

    parser::Parser::new(tokens).parse();

    Ok(())
}

#[test]
fn stdlib_all_modules_parse() {
    let root = stdlib_dir();
    assert!(root.exists(), "stdlib 目录应存在: {}", root.display());

    let files = collect_dal_files(&root);
    assert!(!files.is_empty(), "应至少发现一个 .dal 模块");

    let mut failures: Vec<(String, String)> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        match parse_dal_file(path) {
            Ok(()) => {}
            Err(msg) => failures.push((rel, msg)),
        }
    }

    assert!(
        failures.is_empty(),
        "有 {} 个 stdlib 模块解析失败 (共 {} 个):\n{}",
        failures.len(),
        files.len(),
        failures
            .iter()
            .map(|(name, msg)| format!("  ❌ {}: {}", name, msg))
            .collect::<Vec<_>>()
            .join("\n")
    );

    eprintln!(
        "✅ stdlib 解析门禁通过: {} 个模块全部通过 Lexer + Parser",
        files.len()
    );
}

#[test]
fn stdlib_loader_load_all_ok() {
    // 走真实加载路径：递归解析 use 依赖 + 预置合并
    // StdLibLoader::new 接收「项目根目录」，会自行拼接 stdlib 后缀
    let compiler_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = compiler_dir
        .parent()
        .expect("compiler crate has a parent (workspace root)");
    let mut loader = StdLibLoader::new(workspace_root.to_path_buf())
        .expect("StdLibLoader 应能初始化 (无 dalin.toml 时使用默认配置)");

    let loaded = loader
        .load_all()
        .expect("整棵标准库应能零错误加载 (词法/语法/依赖解析)");

    assert!(
        !loaded.is_empty(),
        "load_all 应至少加载一个模块"
    );

    // 防静默空加载：缓存的模块数应反映真实文件数（不应被静默吞成空 Program）
    let stats = loader.stats();
    assert!(
        stats.cached_modules >= 100,
        "加载的模块数应 >= 100 (实际 {}), 疑似加载器静默空加载",
        stats.cached_modules
    );

    // 预置模块必须存在且可加载
    let prelude = loader
        .load_prelude()
        .expect("prelude + core_types 应能加载");
    assert!(
        prelude.contains(&"prelude".to_string()),
        "prelude 应在预置模块列表中"
    );

    eprintln!(
        "✅ stdlib 加载门禁通过: load_all 加载 {} 个模块, 预置={:?}",
        loaded.len(),
        prelude
    );
}
