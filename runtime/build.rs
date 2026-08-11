//! build.rs — 为 FFI 集成测试构建 cdylib 固件。
//!
//! 在编译 dalin-runtime 时自动编译 `ffi_fixture` 子 crate，把产物
//! `libffi_fixture.{dylib,so,dll}` 拷到本 crate 的 OUT_DIR，供单元测试
//! 经 `env!("OUT_DIR")` 定位并 `ffi_load` 加载。仅依赖本地 cargo，无需联网。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let fixture = manifest_dir.join("ffi_fixture");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .current_dir(&fixture)
        .args(["build", "--release"])
        .status()
        .expect("failed to spawn cargo for ffi_fixture build");
    if !status.success() {
        panic!("ffi_fixture build failed");
    }

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "linux") {
        "so"
    } else {
        "dll"
    };
    let lib_name = format!("libffi_fixture.{ext}");

    let release = fixture.join("target").join("release").join(&lib_name);
    let debug = fixture.join("target").join("debug").join(&lib_name);

    let src = if release.exists() {
        release
    } else if debug.exists() {
        debug
    } else {
        panic!("ffi_fixture artifact not found (tried {release:?} and {debug:?})");
    };

    fs::copy(&src, out_dir.join(&lib_name)).expect("failed to copy ffi_fixture into OUT_DIR");
}
