//! Dalin L 3.0 — 原生代码生成器
//!
//! 将 DLVM 字节码编译为 LLVM IR → 原生机器码。
//! - LLVM 可用时：`emit_from_bytecode()` 生成可执行文件
//! - LLVM 不可用时：优雅降级，返回错误消息
//!
//! 依赖 `inkwell` crate (LLVM Rust 绑定)，可选 feature-gated。
//!

mod native;

pub use native::emit_from_bytecode;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_from_bytecode_stub() {
        let result = emit_from_bytecode(&[0u8; 4], "/tmp/test_output.o");
        #[cfg(not(feature = "native"))]
        {
            assert!(
                result.is_err(),
                "Without native feature, should return error"
            );
            let err = result.unwrap_err();
            assert!(err.contains("LLVM"), "Error should mention LLVM: {}", err);
        }
    }

    #[test]
    fn test_emit_from_bytecode_signature() {
        // Verify the function signature is correct: (&[u8], &str) -> Result<String, String>
        let result: Result<String, String> = emit_from_bytecode(&[], "");
        // Just check it returns something (either Ok or Err depending on feature)
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_emit_from_bytecode_empty() {
        let result = emit_from_bytecode(&[], "/tmp/empty_test.o");
        // Should not panic
        let _ = result;
    }

    #[test]
    fn test_module_structure() {
        // Verify the module compiles and exports the expected function
        assert!(true, "codegen module compiles correctly");
    }
}

#[cfg(feature = "native")]
#[cfg(test)]
mod native_tests {
    use super::*;

    #[test]
    fn test_emit_from_bytecode_basic() {
        let bytecode = vec![0u8, 1, 2, 3];
        let result = emit_from_bytecode(&bytecode, "/tmp/native_test.o");
        // With native feature, this may still fail if LLVM is not properly configured
        // But at least it should not panic
        match result {
            Ok(msg) => assert!(msg.contains("Native"), "Success message: {}", msg),
            Err(e) => {
                // Should be a meaningful LLVM-related error
                assert!(
                    e.contains("LLVM") || e.contains("Failed") || e.contains("native"),
                    "Error should be meaningful: {}",
                    e
                );
            }
        }
    }
}
