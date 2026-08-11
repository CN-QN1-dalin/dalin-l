//! FFI 集成测试固件。
//!
//! 纯 Rust 编译为 cdylib，由 dalin-runtime 的 build.rs 自动构建并拷到 OUT_DIR，
//! 供运行时 `ffi_load` / `ffi_call` 集成测试加载。覆盖常见 C ABI 签名：
//! 全 f64、全 i64、混合、字符串指针、void 返回、4 参数。

use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn df_add(a: f64, b: f64) -> f64 {
    a + b
}

#[no_mangle]
pub extern "C" fn df_mul(a: f64, b: f64) -> f64 {
    a * b
}

#[no_mangle]
pub extern "C" fn df_dsqrt(x: f64) -> f64 {
    x.sqrt()
}

#[no_mangle]
pub extern "C" fn df_iabs(x: i64) -> i64 {
    x.abs()
}

#[no_mangle]
pub extern "C" fn df_sum4(a: f64, b: f64, c: f64, d: f64) -> f64 {
    a + b + c + d
}

#[no_mangle]
pub extern "C" fn df_strlen(s: *const c_char) -> i64 {
    if s.is_null() {
        return -1;
    }
    let mut len = 0i64;
    let mut p = s;
    unsafe {
        while *p != 0 {
            len += 1;
            p = p.add(1);
        }
    }
    len
}

#[no_mangle]
pub extern "C" fn df_void_print(x: i64) {
    // 无副作用，仅供 void 返回签名测试
    let _ = x;
}
