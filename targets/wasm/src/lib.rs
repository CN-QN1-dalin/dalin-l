/// Dalin L 3.0 — WASM 编译目标
///
/// 将 Dalin L AST 编译为 WebAssembly 二进制 (.wasm)。
///
/// ## 设计目标
///
/// - 浏览器端执行：编译后的 .wasm 可在浏览器中直接运行
/// - 与 runtime 共享 AST：复用 `dalin-compiler` 的解析和类型检查
/// - 轻量输出：只包含实际使用的函数，无运行时依赖
///
/// ## 编译管线
///
/// 1. 解析 + 类型检查（复用 dalin-compiler）
/// 2. 七通道降级到线性 IR
/// 3. 生成 WASM 指令序列
/// 4. 输出 .wasm 二进制（或 .wat 文本格式调试用）
///
/// WASM 编译后端
pub struct WasmBackend {
    /// 是否启用优化
    optimize: bool,
    /// 导出的函数名列表
    exports: Vec<String>,
}

impl Default for WasmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmBackend {
    /// 创建新的 WASM 后端
    pub fn new() -> Self {
        Self {
            optimize: true,
            exports: Vec::new(),
        }
    }

    /// 设置是否启用优化
    pub fn set_optimize(&mut self, optimize: bool) {
        self.optimize = optimize;
    }

    /// 添加导出函数
    pub fn add_export(&mut self, name: &str) {
        self.exports.push(name.to_string());
    }

    /// 编译 Dalin L AST 为 WASM 二进制
    ///
    /// 当前为桩实现，返回错误提示未实现
    pub fn compile(&self, _source: &str) -> Result<Vec<u8>, String> {
        Err("WASM backend: stub — not yet implemented".to_string())
    }

    /// 生成 WASM 文本格式（.wat）用于调试
    pub fn compile_to_wat(&self, _source: &str) -> Result<String, String> {
        Err("WASM backend: compile_to_wat stub — not yet implemented".to_string())
    }

    /// 获取导出函数列表
    pub fn exports(&self) -> &[String] {
        &self.exports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_backend_new() {
        let backend = WasmBackend::new();
        assert!(
            backend.optimize,
            "optimization should be enabled by default"
        );
        assert!(backend.exports().is_empty(), "no exports initially");
    }

    #[test]
    fn test_add_export() {
        let mut backend = WasmBackend::new();
        backend.add_export("main");
        assert_eq!(backend.exports().len(), 1);
        assert_eq!(backend.exports()[0], "main");
    }

    #[test]
    fn test_compile_stub() {
        let backend = WasmBackend::new();
        let result = backend.compile("fn main() { return 0 }");
        assert!(result.is_err(), "compile() should return stub error");
        assert!(
            result.unwrap_err().contains("not yet implemented"),
            "error should mention stub"
        );
    }

    #[test]
    fn test_compile_to_wat_stub() {
        let backend = WasmBackend::new();
        let result = backend.compile_to_wat("fn main() { return 0 }");
        assert!(result.is_err());
    }

    #[test]
    fn test_set_optimize() {
        let mut backend = WasmBackend::new();
        backend.set_optimize(false);
        assert!(!backend.optimize);
        backend.set_optimize(true);
        assert!(backend.optimize);
    }
}
