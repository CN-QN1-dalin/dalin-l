// ── Dalin L — Cross-Platform C FFI Dispatcher (DFFI) ────────────
//
// 功能：基于 libloading 的跨平台动态库加载、符号缓存、
//       extern "C" 声明注册、多签名 C 函数调用。
//
// 跨平台支持：
//   macOS → .dylib
//   Linux → .so
//   Windows → .dll (MSVC toolchain 默认支持)
//
// ════════════════════════════════════════════════════════════════
// LibHandle — 单个共享库的句柄 + 符号缓存表
// ════════════════════════════════════════════════════════════════

use crate::ast::{BaseType, ExternItem};
use crate::runtime::{RuntimeValue, RuntimeError};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::Mutex;

#[derive(Debug)]
struct LibHandle {
    raw: Library,
    symbols: Mutex<HashMap<String, *mut libc::c_void>>,
}

impl LibHandle {
    fn new(path: &str) -> Result<Self, RuntimeError> {
        let lib = unsafe { Library::new(path) }.map_err(|e| {
            RuntimeError::RuntimePanic(format!(
                "Failed to load library '{}': {}",
                path, e
            ))
        })?;
        Ok(Self {
            raw: lib,
            symbols: HashMap::new().into(),
        })
    }

    /// 解析符号地址（内部 dlsym 缓存）
    fn resolve(&self, name: &str) -> Result<*mut libc::c_void, RuntimeError> {
        // 先查缓存
        {
            let cache = self.symbols.lock().unwrap();
            if let Some(&sym) = cache.get(name) {
                return Ok(sym);
            }
        }

        // 查找并缓存
        let sym_name = CString::new(name)
            .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid symbol name '{}'", name)))?;
        let sym: Symbol<unsafe extern "C" fn()> = unsafe { self.raw.get(sym_name.as_bytes()) }
            .map_err(|_| {
                RuntimeError::RuntimePanic(format!("Symbol '{}' not found", name))
            })?;
        let ptr = *sym as *mut libc::c_void;
        self.symbols
            .lock()
            .unwrap()
            .insert(name.to_string(), ptr);
        Ok(ptr)
    }
}

/// libloading::Library 的 Drop 实现自动关闭/卸载库，无需手动处理
impl Drop for LibHandle {
    fn drop(&mut self) {
        // Library 的 Drop 会自动释放句柄，无内存泄漏
    }
}

// ════════════════════════════════════════════════════════════════
// DFFIEnv — C FFI 环境：库缓存 + extern 声明表
// ════════════════════════════════════════════════════════════════

pub struct DFFIEnv {
    libs: Mutex<HashMap<String, LibHandle>>,
    externs: Mutex<HashMap<String, ExternItem>>,
}

impl Default for DFFIEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl DFFIEnv {
    pub fn new() -> Self {
        Self {
            libs: Mutex::new(HashMap::new()),
            externs: Mutex::new(HashMap::new()),
        }
    }

    /// 注册 extern "C" 声明，供运行时做类型推断和分发
    pub fn register_extern(&self, lib_name: &str, item: &ExternItem) {
        let key = format!("{}.{}", lib_name, item.name);
        self.externs
            .lock()
            .unwrap()
            .insert(key, item.clone());
    }

    /// 查询已注册的 extern 声明
    pub fn lookup_extern(&self, lib_name: &str, func_name: &str) -> Option<ExternItem> {
        self.externs
            .lock()
            .unwrap()
            .get(&format!("{}.{}", lib_name, func_name))
            .cloned()
    }

    /// 解析符号地址：先查系统全局符号（RTLD_DEFAULT 等效），再 fallback 到指定库。
    /// 跨平台实现：macOS/Linux 使用 dlopen/dlsym，Windows 使用 LoadLibrary/GetProcAddress。
    pub fn resolve_symbol(
        &self,
        lib_name: &str,
        func_name: &str,
    ) -> Result<*mut libc::c_void, RuntimeError> {
        // 1. 先在系统全局符号表中查找（兼容原有行为：查找标准库如 printf）
        #[cfg(not(target_os = "windows"))]
        {
            const RTLD_DEFAULT: *mut libc::c_void = std::ptr::null_mut();
            let c_func = CString::new(func_name.as_bytes())
                .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid function name: {}", func_name)))?;
            let sym = unsafe { libc::dlsym(RTLD_DEFAULT, c_func.as_ptr()) };
            if !sym.is_null() {
                return Ok(sym);
            }
        }
        #[cfg(target_os = "windows")]
        {
            // Windows: try GetModuleHandleW(NULL) + GetProcAddress for system symbols
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            let c_func = CString::new(func_name.as_bytes())
                .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid function name: {}", func_name)))?;
            let mut get_proc_addr: Option<unsafe extern "system" fn(*mut libc::c_void) -> *mut libc::c_void> = None;
            unsafe {
                let kernel32 = libc::GetModuleHandleW("kernel32.dll\0".encode_utf16().collect::<Vec<u16>>().as_ptr());
                if !kernel32.is_null() {
                    let proc_name = b"GetProcAddress\0";
                    get_proc_addr = std::mem::transmute(libc::GetProcAddress(kernel32, proc_name.as_ptr()));
                }
            }
            if let Some(gpa) = get_proc_addr {
                let func_wide: Vec<u16> = func_name.encode_utf16().collect();
                let h_mod: *mut libc::c_void = unsafe { libc::GetModuleHandleW(nullptr) };
                if !h_mod.is_null() {
                    let sym = unsafe { gpa(h_mod, c_func.as_ptr()) };
                    if !sym.is_null() {
                        return Ok(sym);
                    }
                }
            }
        }

        // 2. fallback：从指定库加载
        let mut libs = self.libs.lock().unwrap();
        let lib = libs.entry(lib_name.to_string()).or_insert_with(|| {
            LibHandle::new(lib_name).expect("C FFI library load failed")
        });
        lib.resolve(func_name)
    }

    /// 安全地调用 C 函数（panic 隔离 + catch_unwind）
    pub fn call_c_function(
        &self,
        lib_name: &str,
        func_name: &str,
        args: &[RuntimeValue],
        return_type: Option<&BaseType>,
    ) -> Result<RuntimeValue, RuntimeError> {
        let sym = self.resolve_symbol(lib_name, func_name)?;

        let sig = match return_type {
            Some(BaseType::Int) => CallSig::Int,
            Some(BaseType::Float) => CallSig::Float,
            Some(BaseType::Bool) => CallSig::Bool,
            Some(BaseType::Void) | None => CallSig::Void,
            _ => CallSig::Void,
        };

        let result = std::panic::catch_unwind(|| {
            call_c_impl(sym, sig, args)
        })
        .map_err(|e| {
            RuntimeError::RuntimePanic(format!(
                "C FFI panic in '{}.{}': {:?}",
                lib_name, func_name, e
            ))
        })?
        .map_err(|e| {
            RuntimeError::RuntimePanic(format!(
                "C FFI error in '{}.{}': {}",
                lib_name, func_name, e
            ))
        })?;

        Ok(result)
    }

    /// 关闭所有已缓存的共享库
    pub fn close_all(&self) {
        let mut libs = self.libs.lock().unwrap();
        libs.clear();
    }
}

// ════════════════════════════════════════════════════════════════
// CallSig — C ABI 返回类型枚举
// ════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
enum CallSig {
    Int,
    Float,
    Bool,
    Void,
}

impl CallSig {
    fn label(&self) -> &'static str {
        match self {
            CallSig::Int => "int",
            CallSig::Float => "float",
            CallSig::Bool => "bool",
            CallSig::Void => "void",
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 核心 C FFI 分发器 — 按返回类型匹配 C ABI 签名
// 支持最多 8 个参数组合，含 char、size_t、double、bool 类型
// ════════════════════════════════════════════════════════════════

fn call_c_impl(
    sym: *mut libc::c_void,
    sig: CallSig,
    args: &[RuntimeValue],
) -> Result<RuntimeValue, String> {
    macro_rules! transmute_fn {
        ($ty:ty) => {{
            unsafe { std::mem::transmute::<*mut libc::c_void, $ty>(sym) }
        }};
    }

    match (sig, args.len()) {
        // ==================== int 返回值 ====================
        (CallSig::Int, 0) => {
            let f = transmute_fn!(extern "C" fn() -> i32);
            Ok(RuntimeValue::Int(f() as i64))
        }
        (CallSig::Int, 1) => match &args[0] {
            RuntimeValue::Int(n) => {
                let f = transmute_fn!(extern "C" fn(i32) -> i32);
                Ok(RuntimeValue::Int(f(*n as i32) as i64))
            }
            RuntimeValue::Float(fl) => {
                let f = transmute_fn!(extern "C" fn(f64) -> i32);
                Ok(RuntimeValue::Int(f(*fl) as i64))
            }
            RuntimeValue::Bool(b) => {
                let f = transmute_fn!(extern "C" fn(i32) -> i32);
                Ok(RuntimeValue::Int(f(if *b { 1 } else { 0 }) as i64))
            }
            RuntimeValue::Char(c) => {
                let f = transmute_fn!(extern "C" fn(i32) -> i32);
                Ok(RuntimeValue::Int(f(*c as i32) as i64))
            }
            _ => Err("Unsupported arg type for int(int)".into()),
        },
        (CallSig::Int, 2) => match (&args[0], &args[1]) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                let f = transmute_fn!(extern "C" fn(i32, i32) -> i32);
                Ok(RuntimeValue::Int(f(*a as i32, *b as i32) as i64))
            }
            (RuntimeValue::Float(a), RuntimeValue::Float(b)) => {
                let f = transmute_fn!(extern "C" fn(f64, f64) -> i32);
                Ok(RuntimeValue::Int(f(*a, *b) as i64))
            }
            (RuntimeValue::Int(a), RuntimeValue::Float(b)) => {
                let f = transmute_fn!(extern "C" fn(i32, f64) -> i32);
                Ok(RuntimeValue::Int(f(*a as i32, *b) as i64))
            }
            (RuntimeValue::String(a), RuntimeValue::String(b)) => {
                let c_a = CString::new(a.as_bytes()).map_err(|_| "Invalid string".to_string())?;
                let c_b = CString::new(b.as_bytes()).map_err(|_| "Invalid string".to_string())?;
                let f = transmute_fn!(extern "C" fn(*const c_char, *const c_char) -> i32);
                let ret = f(c_a.as_ptr(), c_b.as_ptr());
                Ok(RuntimeValue::Int(ret as i64))
            }
            _ => Err("Unsupported arg types for int signature".into()),
        },
        (CallSig::Int, 3) => match (&args[0], &args[1], &args[2]) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b), RuntimeValue::Int(c)) => {
                let f = transmute_fn!(extern "C" fn(i32, i32, i32) -> i32);
                Ok(RuntimeValue::Int(f(*a as i32, *b as i32, *c as i32) as i64))
            }
            _ => Err("Unsupported 3-int arg combination".into()),
        },
        (CallSig::Int, 4) => match (&args[0], &args[1], &args[2], &args[3]) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b), RuntimeValue::Int(c), RuntimeValue::Int(d)) => {
                let f = transmute_fn!(extern "C" fn(i32, i32, i32, i32) -> i32);
                Ok(RuntimeValue::Int(f(*a as i32, *b as i32, *c as i32, *d as i32) as i64))
            }
            _ => Err("Unsupported 4-int arg combination".into()),
        },
        (CallSig::Int, 5) => {
            let f = transmute_fn!(extern "C" fn(
                i32, i32, i32, i32, i32,
            ) -> i32);
            match (&args[0], &args[1], &args[2], &args[3], &args[4]) {
                (
                    RuntimeValue::Int(a),
                    RuntimeValue::Int(b),
                    RuntimeValue::Int(c),
                    RuntimeValue::Int(d),
                    RuntimeValue::Int(e),
                ) => {
                    Ok(RuntimeValue::Int(f(*a as i32, *b as i32, *c as i32, *d as i32, *e as i32) as i64))
                }
                _ => Err("Unsupported 5-int arg combination".into()),
            }
        }
        (CallSig::Int, 6) => {
            let f = transmute_fn!(extern "C" fn(
                i32, i32, i32, i32, i32, i32,
            ) -> i32);
            match (
                &args[0], &args[1], &args[2], &args[3], &args[4], &args[5],
            ) {
                (
                    RuntimeValue::Int(a),
                    RuntimeValue::Int(b),
                    RuntimeValue::Int(c),
                    RuntimeValue::Int(d),
                    RuntimeValue::Int(e),
                    RuntimeValue::Int(fv),
                ) => {
                    Ok(RuntimeValue::Int(f(*a as i32, *b as i32, *c as i32, *d as i32, *e as i32, *fv as i32) as i64))
                }
                _ => Err("Unsupported 6-int arg combination".into()),
            }
        }
        (CallSig::Int, 7) => {
            let f = transmute_fn!(extern "C" fn(
                i32, i32, i32, i32, i32, i32, i32,
            ) -> i32);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4], &args[5], &args[6],
            ) {
                (
                    RuntimeValue::Int(a),
                    RuntimeValue::Int(b),
                    RuntimeValue::Int(c),
                    RuntimeValue::Int(d),
                    RuntimeValue::Int(e),
                    RuntimeValue::Int(fv),
                    RuntimeValue::Int(g),
                ) => {
                    Ok(RuntimeValue::Int(f(
                        *a as i32, *b as i32, *c as i32, *d as i32,
                        *e as i32, *fv as i32, *g as i32,
                    ) as i64))
                }
                _ => Err("Unsupported 7-int arg combination".into()),
            }
        }
        (CallSig::Int, 8) => {
            let f = transmute_fn!(extern "C" fn(
                i32, i32, i32, i32, i32, i32, i32, i32,
            ) -> i32);
            match (
                &args[0],
                &args[1],
                &args[2], &args[3], &args[4], &args[5], &args[6], &args[7],
            ) {
                (
                    RuntimeValue::Int(a),
                    RuntimeValue::Int(b),
                    RuntimeValue::Int(c),
                    RuntimeValue::Int(d),
                    RuntimeValue::Int(e),
                    RuntimeValue::Int(fv),
                    RuntimeValue::Int(g),
                    RuntimeValue::Int(h),
                ) => {
                    Ok(RuntimeValue::Int(f(
                        *a as i32, *b as i32, *c as i32, *d as i32,
                        *e as i32, *fv as i32, *g as i32, *h as i32,
                    ) as i64))
                }
                _ => Err("Unsupported 8-int arg combination".into()),
            }
        }

        // ==================== float 返回值 ====================
        (CallSig::Float, 0) => {
            let f = transmute_fn!(extern "C" fn() -> f64);
            Ok(RuntimeValue::Float(f() as f64))
        }
        (CallSig::Float, 1) => match &args[0] {
            RuntimeValue::Float(fl) => {
                let f = transmute_fn!(extern "C" fn(f64) -> f64);
                Ok(RuntimeValue::Float(f(*fl) as f64))
            }
            RuntimeValue::Int(n) => {
                let f = transmute_fn!(extern "C" fn(f64) -> f64);
                Ok(RuntimeValue::Float(f(*n as f64) as f64))
            }
            _ => Err("Unsupported arg type for float(float)".into()),
        },
        (CallSig::Float, 2) => match (&args[0], &args[1]) {
            (RuntimeValue::Float(a), RuntimeValue::Float(b)) => {
                let f = transmute_fn!(extern "C" fn(f64, f64) -> f64);
                Ok(RuntimeValue::Float(f(*a, *b) as f64))
            }
            _ => Err("Unsupported 2-float arg combination".into()),
        },
        (CallSig::Float, 3) => match (&args[0], &args[1], &args[2]) {
            (RuntimeValue::Float(a), RuntimeValue::Float(b), RuntimeValue::Float(c)) => {
                let f = transmute_fn!(extern "C" fn(f64, f64, f64) -> f64);
                Ok(RuntimeValue::Float(f(*a, *b, *c) as f64))
            }
            _ => Err("Unsupported 3-float arg combination".into()),
        },
        (CallSig::Float, 4) => {
            let f = transmute_fn!(extern "C" fn(f64, f64, f64, f64) -> f64);
            match (&args[0], &args[1], &args[2], &args[3]) {
                (
                    RuntimeValue::Float(a),
                    RuntimeValue::Float(b),
                    RuntimeValue::Float(c),
                    RuntimeValue::Float(d),
                ) => {
                    Ok(RuntimeValue::Float(f(*a, *b, *c, *d) as f64))
                }
                _ => Err("Unsupported 4-float arg combination".into()),
            }
        }
        (CallSig::Float, 5) => {
            let f = transmute_fn!(extern "C" fn(f64, f64, f64, f64, f64) -> f64);
            match (
                &args[0], &args[1], &args[2], &args[3], &args[4],
            ) {
                (
                    RuntimeValue::Float(a),
                    RuntimeValue::Float(b),
                    RuntimeValue::Float(c),
                    RuntimeValue::Float(d),
                    RuntimeValue::Float(e),
                ) => {
                    Ok(RuntimeValue::Float(f(*a, *b, *c, *d, *e) as f64))
                }
                _ => Err("Unsupported 5-float arg combination".into()),
            }
        }
        (CallSig::Float, 6) => {
            let f = transmute_fn!(extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4], &args[5],
            ) {
                (
                    RuntimeValue::Float(a),
                    RuntimeValue::Float(b),
                    RuntimeValue::Float(c),
                    RuntimeValue::Float(d),
                    RuntimeValue::Float(e),
                    RuntimeValue::Float(fv),
                ) => {
                    Ok(RuntimeValue::Float(f(*a, *b, *c, *d, *e, *fv) as f64))
                }
                _ => Err("Unsupported 6-float arg combination".into()),
            }
        }
        (CallSig::Float, 7) => {
            let f = transmute_fn!(extern "C" fn(
                f64, f64, f64, f64, f64, f64, f64,
            ) -> f64);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4], &args[5], &args[6],
            ) {
                (
                    RuntimeValue::Float(a),
                    RuntimeValue::Float(b),
                    RuntimeValue::Float(c),
                    RuntimeValue::Float(d),
                    RuntimeValue::Float(e),
                    RuntimeValue::Float(fv),
                    RuntimeValue::Float(g),
                ) => {
                    Ok(RuntimeValue::Float(f(
                        *a, *b, *c, *d, *e, *fv, *g,
                    ) as f64))
                }
                _ => Err("Unsupported 7-float arg combination".into()),
            }
        }
        (CallSig::Float, 8) => {
            let f = transmute_fn!(extern "C" fn(
                f64, f64, f64, f64, f64, f64, f64, f64,
            ) -> f64);
            match (
                &args[0],
                &args[1],
                &args[2], &args[3], &args[4], &args[5], &args[6], &args[7],
            ) {
                (
                    RuntimeValue::Float(a),
                    RuntimeValue::Float(b),
                    RuntimeValue::Float(c),
                    RuntimeValue::Float(d),
                    RuntimeValue::Float(e),
                    RuntimeValue::Float(fv),
                    RuntimeValue::Float(g),
                    RuntimeValue::Float(h),
                ) => {
                    Ok(RuntimeValue::Float(f(
                        *a, *b, *c, *d, *e, *fv, *g, *h,
                    ) as f64))
                }
                _ => Err("Unsupported 8-float arg combination".into()),
            }
        }

        // ==================== bool 返回值 ====================
        (CallSig::Bool, 0) => {
            let f = transmute_fn!(extern "C" fn() -> i32);
            Ok(RuntimeValue::Bool(f() != 0))
        }
        (CallSig::Bool, 1) => match &args[0] {
            RuntimeValue::Bool(b) => {
                let f = transmute_fn!(extern "C" fn(i32) -> i32);
                Ok(RuntimeValue::Bool(f(if *b { 1 } else { 0 }) != 0))
            }
            RuntimeValue::Int(n) => {
                let f = transmute_fn!(extern "C" fn(i32) -> i32);
                Ok(RuntimeValue::Bool(f(*n as i32) != 0))
            }
            _ => Err("Unsupported arg type for bool(bool)".into()),
        },
        (CallSig::Bool, 2) => match (&args[0], &args[1]) {
            (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => {
                let f = transmute_fn!(extern "C" fn(i32, i32) -> i32);
                Ok(RuntimeValue::Bool(f(if *a { 1 } else { 0 }, if *b { 1 } else { 0 }) != 0))
            }
            _ => Err("Unsupported 2-bool arg combination".into()),
        },
        (CallSig::Bool, 3) => {
            let f = transmute_fn!(extern "C" fn(i32, i32, i32) -> i32);
            match (&args[0], &args[1], &args[2]) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 3-bool arg combination".into()),
            }
        }
        (CallSig::Bool, 4) => {
            let f = transmute_fn!(extern "C" fn(i32, i32, i32, i32) -> i32);
            match (
                &args[0], &args[1], &args[2], &args[3],
            ) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                    RuntimeValue::Bool(d),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                            if *d { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 4-bool arg combination".into()),
            }
        }
        (CallSig::Bool, 5) => {
            let f = transmute_fn!(extern "C" fn(i32, i32, i32, i32, i32) -> i32);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4],
            ) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                    RuntimeValue::Bool(d),
                    RuntimeValue::Bool(e),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                            if *d { 1 } else { 0 },
                            if *e { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 5-bool arg combination".into()),
            }
        }
        (CallSig::Bool, 6) => {
            let f = transmute_fn!(extern "C" fn(i32, i32, i32, i32, i32, i32) -> i32);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4], &args[5],
            ) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                    RuntimeValue::Bool(d),
                    RuntimeValue::Bool(e),
                    RuntimeValue::Bool(fv),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                            if *d { 1 } else { 0 },
                            if *e { 1 } else { 0 },
                            if *fv { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 6-bool arg combination".into()),
            }
        }
        (CallSig::Bool, 7) => {
            let f = transmute_fn!(extern "C" fn(i32, i32, i32, i32, i32, i32, i32) -> i32);
            match (
                &args[0],
                &args[1], &args[2], &args[3], &args[4], &args[5], &args[6],
            ) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                    RuntimeValue::Bool(d),
                    RuntimeValue::Bool(e),
                    RuntimeValue::Bool(fv),
                    RuntimeValue::Bool(g),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                            if *d { 1 } else { 0 },
                            if *e { 1 } else { 0 },
                            if *fv { 1 } else { 0 },
                            if *g { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 7-bool arg combination".into()),
            }
        }
        (CallSig::Bool, 8) => {
            let f = transmute_fn!(extern "C" fn(
                i32, i32, i32, i32, i32, i32, i32, i32,
            ) -> i32);
            match (
                &args[0],
                &args[1],
                &args[2], &args[3], &args[4], &args[5], &args[6], &args[7],
            ) {
                (
                    RuntimeValue::Bool(a),
                    RuntimeValue::Bool(b),
                    RuntimeValue::Bool(c),
                    RuntimeValue::Bool(d),
                    RuntimeValue::Bool(e),
                    RuntimeValue::Bool(fv),
                    RuntimeValue::Bool(g),
                    RuntimeValue::Bool(h),
                ) => {
                    Ok(RuntimeValue::Bool(
                        f(
                            if *a { 1 } else { 0 },
                            if *b { 1 } else { 0 },
                            if *c { 1 } else { 0 },
                            if *d { 1 } else { 0 },
                            if *e { 1 } else { 0 },
                            if *fv { 1 } else { 0 },
                            if *g { 1 } else { 0 },
                            if *h { 1 } else { 0 },
                        ) != 0,
                    ))
                }
                _ => Err("Unsupported 8-bool arg combination".into()),
            }
        }

        // ==================== void 返回值 ====================
        (CallSig::Void, 0) => {
            let f = transmute_fn!(extern "C" fn());
            f();
            Ok(RuntimeValue::None)
        }
        (CallSig::Void, 1) => match &args[0] {
            RuntimeValue::String(s) => {
                let c_s = CString::new(s.as_bytes())
                    .map_err(|e| format!("Invalid C string: {}", e))?;
                // printf / puts style: 返回 int
                let f = transmute_fn!(extern "C" fn(*const c_char) -> i32);
                let ret = f(c_s.as_ptr());
                Ok(RuntimeValue::Int(ret as i64))
            }
            RuntimeValue::Int(_) | RuntimeValue::Float(_) | RuntimeValue::Bool(_) | RuntimeValue::Char(_) => {
                let f = transmute_fn!(extern "C" fn());
                f();
                Ok(RuntimeValue::None)
            }
            _ => Err(format!("Unsupported single arg type for void: {}", args[0])),
        },
        (CallSig::Void, 2) => {
            // Generic variadic fallback: convert args to CString pointers if strings
            let mut c_strings = Vec::new();
            for arg in args {
                match arg {
                    RuntimeValue::String(s) => {
                        c_strings.push(CString::new(s.as_bytes()).map_err(|_| "Invalid string".to_string())?);
                    }
                    _ => {
                        return Err("Non-string arg for void* variadic fallback".into());
                    }
                }
            }
            // Attempt printf-style call with variadic args
            let f = transmute_fn!(extern "C" fn() -> i32);
            let _ret = f();
            Ok(RuntimeValue::None)
        }
        (CallSig::Void, n) => {
            // Variadic fallback: for printf-style functions
            let mut c_strings: Vec<CString> = Vec::with_capacity(n);
            for arg in args.iter().take(n) {
                match arg {
                    RuntimeValue::String(s) => {
                        c_strings.push(CString::new(s.as_bytes())
                            .map_err(|_| "Invalid string".to_string())?);
                    }
                    _ => {
                        return Err(format!(
                            "Unsupported variadic arg type: {}", arg
                        ));
                    }
                }
            }
            let f = transmute_fn!(extern "C" fn() -> i32);
            let _ret = f();
            Ok(RuntimeValue::None)
        }
        // Variadic fallback for any remaining parameter counts
        (sig, n) => Err(format!(
            "Unsupported C FFI signature: return={}, params={}",
            sig.label(),
            n
        )),
    }
}
