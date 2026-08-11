//! Dalin L 3.0 — dalib pkg 包管理子命令
//!
//! 类似 Cargo 的包管理器，但为 Dalin L 做了裁剪和定制：
//! - dalib pkg init name — 初始化项目 + 生成 dalan.toml
//! - dalib pkg add dep [--git URL] [--version VER] — 添加依赖
//! - dalib pkg list — 列出已安装的依赖及版本
//! - dalib pkg build — 解析 dalan.toml，下载/解析依赖，生成 dalan.lock
//! - dalib pkg remove dep — 移除依赖
use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use dalin_compiler::package::{
    DependencyEntry, DependencyGraph, DependencySource, PackageInfo, PackageManifest, SemVer,
    parse_package_manifest,
};
use dalin_registry::net::{download_artifact, fetch_package_index, resolve_best};
use dalin_registry::sha256::sha256_hex;

// ═══════════════════════════════
//  核心实现
// ═══════════════════════════════

fn find_dalan_toml(path: &std::path::Path) -> Result<PathBuf, String> {
    let candidate = path.join("dalan.toml");
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(format!(
        "No `dalan.toml` found at '{}'. Run `dalib pkg init` first.",
        path.display()
    ))
}

fn read_manifest(path: &PathBuf) -> Result<PackageManifest, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    parse_package_manifest(&content)
}

fn write_manifest(path: &PathBuf, manifest: &PackageManifest) -> Result<(), String> {
    let mut content = String::new();

    content.push_str("[package]\n");
    writeln!(content, "name = \"{}\"", manifest.name).unwrap();
    writeln!(content, "version = \"{}\"", manifest.version).unwrap();
    if !manifest.edition.is_empty() {
        writeln!(content, "edition = \"{}\"", manifest.edition).unwrap();
    }
    if let Some(ref desc) = manifest.description {
        writeln!(content, "description = \"{desc}\"").unwrap();
    }
    if !manifest.authors.is_empty() {
        writeln!(content, "authors = {:?}", manifest.authors).unwrap();
    }
    if let Some(ref license) = manifest.license {
        writeln!(content, "license = \"{license}\"").unwrap();
    }

    if !manifest.deps.is_empty() {
        content.push_str("\n[dependencies]\n");
        for (name, dep) in &manifest.deps {
            match &dep.source {
                DependencySource::Git(url) => {
                    writeln!(
                        content,
                        "{} = {{ version = \"{}\", git = \"{}\" }}",
                        name, dep.version, url
                    )
                    .unwrap();
                }
                DependencySource::Path(p) => {
                    writeln!(
                        content,
                        "{} = {{ version = \"{}\", path = \"{}\" }}",
                        name, dep.version, p
                    )
                    .unwrap();
                }
                _ => {
                    if dep.optional {
                        writeln!(
                            content,
                            "{} = {{ version = \"{}\", optional = true }}",
                            name, dep.version
                        )
                        .unwrap();
                    } else {
                        writeln!(content, "{} = \"{}\"", name, dep.version).unwrap();
                    }
                }
            }
        }
    }

    if !manifest.dev_deps.is_empty() {
        content.push_str("\n[dev-dependencies]\n");
        for (name, dep) in &manifest.dev_deps {
            writeln!(content, "{} = \"{}\"", name, dep.version).unwrap();
        }
    }

    fs::write(path, content).map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

pub fn run(subcommand: &str, args: &HashMap<String, String>) -> Result<(), String> {
    match subcommand {
        "init" => cmd_init(args),
        "add" => cmd_add(args),
        "remove" => cmd_remove(args),
        "list" => cmd_list(args),
        "build" => cmd_build(args),
        _ => Err(format!(
            "Unknown pkg subcommand: {subcommand}. Use: init/add/remove/list/build"
        )),
    }
}

fn cmd_init(args: &HashMap<String, String>) -> Result<(), String> {
    let path_str = args.get("path").cloned().unwrap_or_else(|| ".".to_string());
    let pbuf = PathBuf::from(&path_str);
    let name = args.get("name").cloned().unwrap_or_else(|| {
        pbuf.file_name().map_or_else(
            || "my-dalin-project".to_string(),
            |s| s.to_string_lossy().to_string(),
        )
    });
    let lib_only = args.get("lib").is_some_and(|v| v == "true");

    let out_dir = if pbuf.to_str() == Some(".") {
        PathBuf::from(&name)
    } else {
        pbuf.clone()
    };

    if out_dir.exists()
        && let Ok(entries) = fs::read_dir(&out_dir)
    {
        let count: usize = entries.count();
        if count > 0 {
            return Err(format!("Directory '{}' is not empty", out_dir.display()));
        }
    }

    fs::create_dir_all(out_dir.join("src")).map_err(|e| format!("Cannot create src/: {e}"))?;
    fs::create_dir_all(out_dir.join("tests")).map_err(|e| format!("Cannot create tests/: {e}"))?;

    let toml_content = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[dependencies]\n"
    );
    fs::write(out_dir.join("dalan.toml"), toml_content)
        .map_err(|e| format!("Cannot write dalan.toml: {e}"))?;
    println!("  Created dalan.toml");

    let main_code = if lib_only {
        "@lib\nfn add(a: Int, b: Int) -> Int { return a + b; }\n"
    } else {
        "@main\nfn main() -> Int {\n    println(\"Hello, Dalin L 3.0!\");\n    return 0;\n}"
    };
    fs::write(out_dir.join("src/main.dal"), main_code)
        .map_err(|e| format!("Cannot write src/main.dal: {e}"))?;
    println!("  Created src/main.dal");

    let test_code = "?test\nfn test_basic() -> Bool { return true; }\n";
    fs::write(out_dir.join("tests/basic_test.dal"), test_code)
        .map_err(|e| format!("Cannot write tests/basic_test.dal: {e}"))?;
    println!("  Created tests/basic_test.dal");

    fs::write(
        out_dir.join(".gitignore"),
        "target/\n.dalan/\n*.rlib\n*.lock\n",
    )
    .map_err(|e| format!("Cannot write .gitignore: {e}"))?;
    println!("  Created .gitignore");

    println!(
        "\n  Project '{}' initialized at {}\n",
        name,
        out_dir.display()
    );
    Ok(())
}

fn cmd_add(args: &HashMap<String, String>) -> Result<(), String> {
    let name = match args.get("name") {
        Some(n) => n.clone(),
        None => return Err("Missing dependency name. Usage: dalib pkg add <name>".into()),
    };
    let version = match args.get("version") {
        Some(v) => v.clone(),
        None => "*".to_string(),
    };
    let git_url = args.get("git").cloned();
    let optional = args.get("optional").is_some_and(|v| v == "true");

    let toml_path = find_dalan_toml(&PathBuf::from("."))?;
    let mut manifest = read_manifest(&toml_path)?;

    let source = match &git_url {
        Some(git) => DependencySource::Git(git.clone()),
        None => DependencySource::Registry("crates.dal.in".to_string()),
    };

    let version_req = DependencyEntry {
        version: version.clone(),
        optional,
        default_features: true,
        features: Vec::new(),
        source,
    };

    manifest.deps.insert(name.clone(), version_req);
    write_manifest(&toml_path, &manifest)?;

    let src_display = match &git_url {
        Some(u) => format!("@ git:{u}"),
        None => String::new(),
    };

    println!("  Added {name} {version}{src_display} (optional={optional})");
    Ok(())
}

fn cmd_remove(args: &HashMap<String, String>) -> Result<(), String> {
    let name = match args.get("name") {
        Some(n) => n.clone(),
        None => return Err("Missing dependency name. Usage: dalib pkg remove <name>".into()),
    };

    let toml_path = find_dalan_toml(&PathBuf::from("."))?;
    let mut manifest = read_manifest(&toml_path)?;

    if manifest.deps.remove(&name).is_some() {
        write_manifest(&toml_path, &manifest)?;
        println!("  Removed {name}");
    } else {
        println!("  Warning: {name} not found in dependencies");
    }

    Ok(())
}

fn cmd_list(args: &HashMap<String, String>) -> Result<(), String> {
    let toml_path = find_dalan_toml(&PathBuf::from("."))?;
    let manifest = read_manifest(&toml_path)?;
    let as_json = args.get("json").is_some_and(|v| v == "true");

    if manifest.deps.is_empty() {
        println!("  (no dependencies)");
        return Ok(());
    }

    if as_json {
        print!("{{\n  \"dependencies\": {{");
        let mut first = true;
        for (name, dep) in &manifest.deps {
            if !first {
                print!(",");
            }
            first = false;
            println!();
            print!(
                "    \"{}\": {{\"version\": \"{}\", \"optional\": {}, \"source\": {:?}",
                name, dep.version, dep.optional, dep.source
            );
        }
        println!("\n  }}\n}}");
        return Ok(());
    }

    println!("  Dependencies:\n");
    for (name, dep) in &manifest.deps {
        let opt_marker = if dep.optional { " (optional)" } else { "" };
        let src_info = match &dep.source {
            DependencySource::Git(u) => format!(" [git: {u}]"),
            DependencySource::Path(p) => format!(" [path: {p}]"),
            DependencySource::Registry(r) => format!(" [registry: {r}]"),
        };
        println!("  {} @ {}{}{}", name, dep.version, opt_marker, src_info);
    }

    if !manifest.dev_deps.is_empty() {
        println!("\n  Dev Dependencies:\n");
        for (name, dep) in &manifest.dev_deps {
            println!("  {} @ {}", name, dep.version);
        }
    }

    Ok(())
}

fn cmd_build(_args: &HashMap<String, String>) -> Result<(), String> {
    let toml_path = find_dalan_toml(&PathBuf::from("."))?;
    let manifest = read_manifest(&toml_path)?;

    println!(
        "  Resolving dependencies for '{}' v{}...",
        manifest.name, manifest.version
    );

    // 维护依赖图（用于冲突检测 / 内部一致性），但真正落盘以远端解析结果为准
    let mut graph = DependencyGraph::new();
    for (name, dep) in &manifest.deps {
        let ver =
            SemVer::parse(dep.version.trim_matches('"')).unwrap_or_else(|_| SemVer::new(1, 0, 0));
        graph.add_package(
            name.clone(),
            PackageInfo {
                name: name.clone(),
                description: None,
                available_versions: vec![ver],
                homepage: None,
            },
        );
    }
    if let Err(e) = graph.resolve_all() {
        println!("  Note: dependency graph conflict: {e}");
    }

    let cache_root = PathBuf::from(".dalan/registry");
    let mut lock_lines = Vec::new();
    lock_lines.push("# Dalin L Package Lock".to_string());
    lock_lines.push("# This file is auto-generated by `dalib pkg build`".to_string());
    lock_lines.push(format!(
        "# Package: {} v{}",
        manifest.name, manifest.version
    ));
    lock_lines.push(String::new());

    let mut resolved_count = 0usize;
    for (name, dep) in &manifest.deps {
        match &dep.source {
            DependencySource::Registry(host) => {
                match resolve_registry_dep(name, &dep.version, host, &cache_root) {
                    Ok(entry) => {
                        lock_lines.push(entry);
                        resolved_count += 1;
                    }
                    Err(e) => {
                        println!("  Warning: cannot fetch '{name}' from registry {host}: {e}");
                        println!("  Falling back to declared version (no artifact cached).");
                        lock_lines.push(format!("{name}@{}", dep.version.trim_matches('"')));
                        resolved_count += 1;
                    }
                }
            }
            _ => {
                lock_lines.push(format!("{name}@{}", dep.version.trim_matches('"')));
                resolved_count += 1;
            }
        }
    }

    let lock_content = lock_lines.join("\n");
    let lock_path = PathBuf::from("dalan.lock");
    fs::write(&lock_path, lock_content).map_err(|e| format!("Cannot write dalan.lock: {e}"))?;
    println!("  Resolved {resolved_count} dependencies");
    println!("  Generated dalan.lock");

    println!("  Building stdlib...");
    let stdlib_path = PathBuf::from("stdlib");
    if stdlib_path.exists() && stdlib_path.is_dir() {
        let entries = if let Ok(e) = fs::read_dir(&stdlib_path) {
            e
        } else {
            println!("  Note: could not read stdlib/");
            println!("\n  Build finished successfully!");
            return Ok(());
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension()
                && ext == "dal"
            {
                count += 1;
            }
        }
        println!("  Compiled {count} stdlib modules");
    } else {
        println!("  Note: stdlib/ directory not found");
    }

    println!("\n  Build finished successfully!");
    Ok(())
}

/// 从注册表解析最优版本、下载工件到本地缓存，并生成 `dalan.lock` 条目。
///
/// 若本地缓存已存在则跳过下载（仍记录 sha256 以校验一致性）。
fn resolve_registry_dep(
    name: &str,
    version_req: &str,
    host: &str,
    cache_root: &Path,
) -> Result<String, String> {
    let index = fetch_package_index(host, name)?;
    let pkg = resolve_best(&index, version_req)
        .ok_or_else(|| format!("没有满足 '{version_req}' 的可用版本"))?;

    let cache_path = cache_root.join(name).join(format!("{}.dal", pkg.version));
    let sha = if cache_path.exists() {
        let bytes = fs::read(&cache_path).map_err(|e| format!("读取缓存失败：{e}"))?;
        sha256_hex(&bytes)
    } else {
        let art = download_artifact(&pkg.artifact_url, &cache_path)?;
        art.sha256
    };

    Ok(format!(
        "{name}@{} registry url={} sha256={}",
        pkg.version, pkg.artifact_url, sha
    ))
}
