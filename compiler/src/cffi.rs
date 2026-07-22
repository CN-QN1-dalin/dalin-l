// ── Dalin L — C FFI Dispatcher (DFFI) ────────────────────────────
//
// 功能：dlopen/dlsym 缓存、extern "C" 声明注册、多签名 C 函数调用。

use crate::ast::{BaseType, ExternItem};
use crate::runtime::{RuntimeValue, RuntimeError};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::Mutex;

// ════════════════════════════════════════════════════════════════
// LibHandle — 单个共享库的句柄 + 符号缓存表
// ════════════════════════════════════════════════════════════════

#[derive(Debug)]
struct LibHandle {
    raw: *mut libc::c_void,
    symbols: HashMap<String, *mut libc::c_void>,
}

impl LibHandle {
    fn new(path: &str) -> Result<Self, RuntimeError> {
        let c_path = CString::new(path)
            .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid library path: '{}'", path)))?;
        let raw = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if raw.is_null() {
            return Err(RuntimeError::RuntimePanic(
                format!("Failed to open C library '{}'", path),
            ));
        }
        Ok(Self { raw, symbols: HashMap::new() })
    }

    /// 解析符号地址（内部 dlsym 缓存）
    fn resolve(&mut self, name: &str) -> Result<*mut libc::c_void, RuntimeError> {
        let c_name = CString::new(name)
            .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid symbol name '{}'", name)))?;
        let sym = unsafe { libc::dlsym(self.raw, c_name.as_ptr()) };
        if sym.is_null() {
            return Err(RuntimeError::RuntimePanic(format!(
                "Symbol '{}' not found", name
            )));
        }
        self.symbols.insert(name.to_string(), sym);
        Ok(sym)
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
    fn default() -> Self { Self::new() }
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
        self.externs.lock().unwrap().insert(key, item.clone());
    }

    /// 查询已注册的 extern 声明
    pub fn lookup_extern(&self, lib_name: &str, func_name: &str) -> Option<ExternItem> {
        self.externs
            .lock()
            .unwrap()
            .get(&format!("{}.{}", lib_name, func_name))
            .cloned()
    }

    /// 解析符号地址：先查 RTLD_DEFAULT（系统 C 库已在进程空间），再 fallback dlopen
    pub fn resolve_symbol(
        &self,
        lib_name: &str,
        func_name: &str,
    ) -> Result<*mut libc::c_void, RuntimeError> {
        // 1. RTLD_DEFAULT 查找
        let c_func = CString::new(func_name.as_bytes())
            .map_err(|_| RuntimeError::RuntimePanic(format!("Invalid function name: {}", func_name)))?;
        let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c_func.as_ptr()) };
        if !sym.is_null() {
            return Ok(sym);
        }
        // 2. dlopen fallback
        let mut libs = self.libs.lock().unwrap();
        let lib = libs
            .entry(lib_name.to_string())
            .or_insert_with(|| match LibHandle::new(lib_name) {
                Ok(h) => h,
                Err(e) => panic!("{}", e),
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
            call_c_impl(sym, sig, args.len(), args)
        })
        .map_err(|e| RuntimeError::RuntimePanic(format!(
            "C FFI panic in '{}.{}': {:?}", lib_name, func_name, e
        )))?
        .map_err(|e| RuntimeError::RuntimePanic(format!(
            "C FFI error in '{}.{}': {}", lib_name, func_name, e
        )))?;

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
// 核心 C FFI 分发器 — 按返回类型 + 参数数量匹配 C ABI 签名
// ════════════════════════════════════════════════════════════════

fn call_c_impl(
    sym: *mut libc::c_void,
    sig: CallSig,
    param_count: usize,
    args: &[RuntimeValue],
) -> Result<RuntimeValue, String> {
    match (sig, param_count) {
        // === int ret, 0 args ===
        (CallSig::Int, 0) => {
            let f: extern "C" fn() -> libc::c_int = unsafe { std::mem::transmute(sym) };
            Ok(RuntimeValue::Int(f() as i64))
        }
        // === int ret, 1 arg ===
        (CallSig::Int, 1) => match &args[0] {
            RuntimeValue::Int(n) => {
                let f: extern "C" fn(libc::c_int) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Int(f(*n as libc::c_int) as i64))
            }
            RuntimeValue::Float(fl) => {
                let f: extern "C" fn(libc::c_double) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Int(f(*fl) as i64))
            }
            RuntimeValue::Bool(b) => {
                let f: extern "C" fn(libc::c_int) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Int(f(if *b { 1i32 } else { 0i32 }) as i64))
            }
            other => Err(format!("Cannot convert {} to C int argument", other)),
        },
        // === int ret, 2 args ===
        (CallSig::Int, 2) => match (&args[0], &args[1]) {
            (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                let f: extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Int(
                    f(*a as libc::c_int, *b as libc::c_int) as i64,
                ))
            }
            _ => Err("Unsupported arg types for int(int,int)".into()),
        },
        // === float ret, 0 args ===
        (CallSig::Float, 0) => {
            let f: extern "C" fn() -> libc::c_double = unsafe { std::mem::transmute(sym) };
            Ok(RuntimeValue::Float(f() as f64))
        }
        // === float ret, 1 arg ===
        (CallSig::Float, 1) => match &args[0] {
            RuntimeValue::Float(fl) => {
                let f: extern "C" fn(libc::c_double) -> libc::c_double = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Float(f(*fl) as f64))
            }
            RuntimeValue::Int(n) => {
                let f: extern "C" fn(libc::c_double) -> libc::c_double = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Float(f(*n as f64) as f64))
            }
            other => Err(format!("Cannot convert {} to C float argument", other)),
        },
        // === bool ret, 0 args ===
        (CallSig::Bool, 0) => {
            let f: extern "C" fn() -> libc::c_int = unsafe { std::mem::transmute(sym) };
            Ok(RuntimeValue::Bool(f() != 0))
        }
        // === bool ret, 1 arg ===
        (CallSig::Bool, 1) => match &args[0] {
            RuntimeValue::Bool(b) => {
                let f: extern "C" fn(libc::c_int) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                Ok(RuntimeValue::Bool(f(if *b { 1i32 } else { 0i32 }) != 0))
            }
            other => Err(format!("Unsupported bool argument type: {}", other)),
        },
        // === void ret, 0 args ===
        (CallSig::Void, 0) => {
            let f: extern "C" fn() = unsafe { std::mem::transmute(sym) };
            f();
            Ok(RuntimeValue::None)
        }
        // === void ret, 1 arg (string-based: printf / puts style) ===
        (CallSig::Void, 1) => match &args[0] {
            RuntimeValue::String(s) => {
                let c_s = CString::new(s.as_bytes())
                    .map_err(|e| format!("Invalid C string: {}", e))?;
                let f: extern "C" fn(*const c_char) -> libc::c_int = unsafe { std::mem::transmute(sym) };
                let ret = f(c_s.as_ptr());
                Ok(RuntimeValue::Int(ret as i64))
            }
            other => Err(format!(
                "Unsupported single arg type for void call: {}",
                other
            )),
        },
        _ => Err(format!(
            "Unsupported C FFI signature: return={}, params={}",
            sig.label(),
            param_count
        )),
    }
}
