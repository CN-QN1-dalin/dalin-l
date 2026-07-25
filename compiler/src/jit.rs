/// Dalin L 3.0 — JIT 编译器骨架
///
/// 将 Dalin L AST 直接编译为原生机器码 (LLVM ORC JIT)。
/// 替换当前 AST 树遍历解释器，将性能提升 10-100x。
///
/// ## 设计目标
///
/// - 零开销抽象：运行时无 GC pause，无引用计数
/// - 增量编译：只重新编译变动的函数
/// - 七通道感知：根据 `@cpu`/`@io`/`@net` 等注解选择不同的优化策略
///
/// ## 未来集成路径
///
/// 1. `inkwell` crate 提供 LLVM IR 绑定
/// 2. 编译器将 AST 翻译为 LLVM IR
/// 3. ORC JIT 在运行时加载并执行机器码
/// 4. 回退到解释器（当前 `dalin-runtime`）对于无法 JIT 的函数
use crate::ast::Program;

/// LLVM ORC JIT 编译器
///
/// 将 Dalin L 的 `Program` (AST) 编译为原生机器码。
/// 初始为桩实现，所有调用返回 `Err` 提示未实现。
pub struct JitCompiler {
    /// 是否启用 JIT
    enabled: bool,
    /// 编译统计
    compiled_count: usize,
    /// 缓存命中统计
    cache_hits: usize,
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl JitCompiler {
    /// 创建一个新的 JIT 编译器实例（默认关闭）
    pub fn new() -> Self {
        Self {
            enabled: false,
            compiled_count: 0,
            cache_hits: 0,
        }
    }

    /// 启用 JIT 编译
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// 禁用 JIT 编译（回退到解释器）
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// 检查 JIT 是否已启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 编译整个 Program（AST）为机器码
    ///
    /// 遍历 AST，为每个函数生成 LLVM IR → 优化 → 机器码
    pub fn compile(&self, _program: &Program) -> Result<(), String> {
        if !self.enabled {
            return Err("JIT compiler is disabled".to_string());
        }
        Err("JIT compiler: stub — not yet implemented".to_string())
    }

    /// 编译单个函数（增量编译入口）
    ///
    /// 当用户修改或添加单个函数时调用，避免全量重编译
    pub fn compile_function(&self, _name: &str, _body: &str) -> Result<(), String> {
        Err("JIT compiler: compile_function stub — not yet implemented".to_string())
    }

    /// 获取已编译的函数数量
    pub fn compiled_count(&self) -> usize {
        self.compiled_count
    }

    /// 获取缓存命中数
    pub fn cache_hits(&self) -> usize {
        self.cache_hits
    }

    /// 重置统计信息
    pub fn reset_stats(&mut self) {
        self.compiled_count = 0;
        self.cache_hits = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    #[test]
    fn test_jit_disabled_by_default() {
        let jit = JitCompiler::new();
        assert!(!jit.is_enabled(), "JIT should be disabled by default");
        assert_eq!(jit.compiled_count(), 0);
        assert_eq!(jit.cache_hits(), 0);
    }

    #[test]
    fn test_jit_enable_disable() {
        let mut jit = JitCompiler::new();
        jit.enable();
        assert!(jit.is_enabled());
        jit.disable();
        assert!(!jit.is_enabled());
    }

    #[test]
    fn test_compile_returns_stub_error_when_enabled() {
        let mut jit = JitCompiler::new();
        jit.enable();
        let prog = Program::new();
        let result = jit.compile(&prog);
        assert!(result.is_err(), "compile() should return error (stub)");
        assert!(
            result.unwrap_err().contains("not yet implemented"),
            "error message should indicate stub"
        );
    }

    #[test]
    fn test_compile_returns_disabled_error() {
        let jit = JitCompiler::new();
        let prog = Program::new();
        let result = jit.compile(&prog);
        assert!(result.is_err(), "compile() should fail when disabled");
        assert!(
            result.unwrap_err().contains("disabled"),
            "error message should indicate disabled"
        );
    }

    #[test]
    fn test_compile_function_stub() {
        let mut jit = JitCompiler::new();
        jit.enable();
        let result = jit.compile_function("test_fn", "return 42");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("not yet implemented"),
            "compile_function should return stub error"
        );
    }

    #[test]
    fn test_reset_stats() {
        let mut jit = JitCompiler::new();
        jit.reset_stats();
        assert_eq!(jit.compiled_count(), 0);
        assert_eq!(jit.cache_hits(), 0);
    }
}
