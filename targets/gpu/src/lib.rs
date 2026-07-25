/// Dalin L 3.0 — GPU 计算目标
///
/// 将 Dalin L AST 编译为 GPU 着色器语言（Metal Shading Language / CUDA）。
///
/// ## 设计目标
///
/// - GPU 加速：将 `@cpu` + 纯数据并行的函数编译为 GPU 内核
/// - Metal 优先：macOS/iOS 平台优先 Metal，回退到 CUDA
/// - 自动并行化：识别可向量化的循环和张量操作
///
/// ## 编译管线
///
/// 1. AST 分析：识别数据并行模式和七通道注解
/// 2. IR 降级：将 Dalin L IR 线性化为 GPU 内核 IR
/// 3. Metal/CUDA 代码生成：输出 MSL 或 CUDA C++
/// 4. 运行时加载：通过 Metal API / CUDA Driver API 加载和启动内核
///
/// ## 支持的 GPU 后端
pub enum GpuBackend {
    /// Apple Metal (macOS/iOS)
    Metal,
    /// NVIDIA CUDA
    Cuda,
    /// 自动检测
    Auto,
}

/// GPU 计算编译器
pub struct GpuCompiler {
    /// 目标后端
    backend: GpuBackend,
    /// 工作组大小
    workgroup_size: (u32, u32, u32),
    /// 是否启用自动并行化
    auto_parallelize: bool,
}

impl Default for GpuCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuCompiler {
    /// 创建新的 GPU 编译器（自动检测后端）
    pub fn new() -> Self {
        Self {
            backend: GpuBackend::Auto,
            workgroup_size: (256, 1, 1),
            auto_parallelize: true,
        }
    }

    /// 设置 GPU 后端
    pub fn set_backend(&mut self, backend: GpuBackend) {
        self.backend = backend;
    }

    /// 设置工作组大小
    pub fn set_workgroup_size(&mut self, x: u32, y: u32, z: u32) {
        self.workgroup_size = (x, y, z);
    }

    /// 启用/禁用自动并行化
    pub fn set_auto_parallelize(&mut self, enable: bool) {
        self.auto_parallelize = enable;
    }

    /// 编译 Dalin L 源码为 Metal Shading Language
    pub fn compile_to_metal(&self, _source: &str) -> Result<String, String> {
        Err("GPU backend: Metal stub — not yet implemented".to_string())
    }

    /// 编译 Dalin L 源码为 CUDA C++
    pub fn compile_to_cuda(&self, _source: &str) -> Result<String, String> {
        Err("GPU backend: CUDA stub — not yet implemented".to_string())
    }

    /// 自动检测并编译到合适的 GPU 后端
    pub fn compile(&self, source: &str) -> Result<String, String> {
        match self.backend {
            GpuBackend::Metal => self.compile_to_metal(source),
            GpuBackend::Cuda => self.compile_to_cuda(source),
            GpuBackend::Auto => {
                // macOS 优先 Metal，否则 CUDA
                #[cfg(target_os = "macos")]
                {
                    self.compile_to_metal(source)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    self.compile_to_cuda(source)
                }
            }
        }
    }

    /// 获取后端名称
    pub fn backend_name(&self) -> &str {
        match self.backend {
            GpuBackend::Metal => "Metal",
            GpuBackend::Cuda => "CUDA",
            GpuBackend::Auto => "Auto",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_compiler_new() {
        let compiler = GpuCompiler::new();
        assert_eq!(compiler.workgroup_size, (256, 1, 1));
        assert!(compiler.auto_parallelize);
    }

    #[test]
    fn test_set_backend() {
        let mut compiler = GpuCompiler::new();
        compiler.set_backend(GpuBackend::Metal);
        assert_eq!(compiler.backend_name(), "Metal");
        compiler.set_backend(GpuBackend::Cuda);
        assert_eq!(compiler.backend_name(), "CUDA");
    }

    #[test]
    fn test_compile_to_metal_stub() {
        let mut compiler = GpuCompiler::new();
        compiler.set_backend(GpuBackend::Metal);
        let result = compiler.compile("fn add(a, b) { return a + b }");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("not yet implemented"),
            "error should mention stub"
        );
    }

    #[test]
    fn test_compile_to_cuda_stub() {
        let mut compiler = GpuCompiler::new();
        compiler.set_backend(GpuBackend::Cuda);
        let result = compiler.compile("fn add(a, b) { return a + b }");
        assert!(result.is_err());
    }

    #[test]
    fn test_workgroup_size() {
        let mut compiler = GpuCompiler::new();
        compiler.set_workgroup_size(128, 2, 1);
        assert_eq!(compiler.workgroup_size, (128, 2, 1));
    }

    #[test]
    fn test_auto_parallelize() {
        let mut compiler = GpuCompiler::new();
        compiler.set_auto_parallelize(false);
        assert!(!compiler.auto_parallelize);
    }
}
