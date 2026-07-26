//! Dalin L 3.0 — GPU 计算目标
//!
//! 将 Dalin L 源码编译为 GPU 着色器语言（Metal Shading Language / CUDA）。
//!
//! ## 设计目标
//!
//! - GPU 加速：将 `@cpu` + 纯数据并行的函数编译为 GPU 内核
//! - Metal 优先：macOS/iOS 平台优先 Metal，回退到 CUDA
//! - 自动并行化：识别可向量化的循环和张量操作
//!
//! ## 编译管线
//!
//! 1. 源码解析：识别函数签名、参数类型、控制流
//! 2. 并行性分析：检测 for 循环和纯数学表达式 → 可 GPU 加速
//! 3. MSL/CUDA 代码生成：输出 .metal / .cu 文本文件
//! 4. 后续由 xcode/metallib 或 nvcc 进行实际编译

use std::fmt;
use std::fmt::Write;

// ═══════════════════════════════
//  GPU Backend Enum
// ═══════════════════════════════

/// GPU 后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// Apple Metal (macOS/iOS)
    Metal,
    /// NVIDIA CUDA
    Cuda,
    /// 自动检测
    Auto,
}

impl fmt::Display for GpuBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metal => write!(f, "Metal"),
            Self::Cuda => write!(f, "CUDA"),
            Self::Auto => write!(f, "Auto"),
        }
    }
}

// ═══════════════════════════════
//  Type Mapping
// ═══════════════════════════════

/// Dalin L 类型 → GPU 语言类型
fn dal_type_to_gpu(base: &str, _gpu_backend: &str) -> String {
    match base {
        "Int" | "int" => "int".to_string(),
        "Float" | "float" => "float".to_string(),
        "Bool" | "bool" => "bool".to_string(),
        "String" | "string" => "string".to_string(),
        _ => "int".to_string(),
    }
}

/// Dalin L 类型 → GPU 设备指针类型（用于数组参数）
fn dal_array_type_to_gpu(base: &str, gpu_backend: &str) -> String {
    let elem = dal_type_to_gpu(base, "");
    match gpu_backend {
        "cuda" => format!("const {elem}*"),
        _ => format!("const device {elem}*"), // Metal
    }
}

// ═══════════════════════════════
//  AST Analysis
// ═══════════════════════════════

/// GPU 可加速函数分析结果
pub struct GpuAnalysis {
    pub fn_name: String,
    pub params: Vec<GpuParam>,
    pub return_type: Option<String>,
    pub is_parallel: bool,
    pub loop_count: usize,
    pub has_nested_loops: bool,
}

#[derive(Clone)]
pub struct GpuParam {
    pub name: String,
    pub base_type: String,
    pub is_array: bool,
}

/// 从 Dalin L 源码字符串中分析 GPU 加速潜力
///
/// 检测逻辑：
/// 1. 提取函数名 `fn name(...)`
/// 2. 提取参数列表，识别类型和数组标注 `[[Type]]`
/// 3. 提取返回类型 `-> RetType`
/// 4. 统计 for 循环数量（嵌套深度 > 1 时为嵌套循环）
#[must_use]
pub fn analyze_for_gpu(source: &str) -> GpuAnalysis {
    let mut params = Vec::new();
    let mut fn_name = "unknown".to_string();
    let mut return_type = None;
    let mut loop_count = 0usize;
    let mut has_nested_loops = false;

    // 1. Extract function name
    if let Some(pos) = source.find("fn ") {
        let rest = &source[pos + 3..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(rest.len());
        fn_name = rest[..name_end].trim().to_string();
    }

    // 2. Extract parameter types from signature
    if let Some(paren_start) = source.find('(') {
        let paren_end_rel = match source[paren_start..].find(')') {
            Some(n) => n,
            None => {
                return GpuAnalysis {
                    fn_name,
                    params,
                    return_type,
                    is_parallel: false,
                    loop_count: 0,
                    has_nested_loops: false,
                };
            }
        };
        let paren_end = paren_start + paren_end_rel;
        let param_str = source[paren_start + 1..paren_end].trim();
        if !param_str.is_empty() {
            for param in param_str.split(',') {
                let param = param.trim();
                if let Some(colon) = param.find(':') {
                    let p_name = param[..colon].trim().to_string();
                    let p_type = param[colon + 1..].trim().to_string();
                    let is_array = p_type.contains("[[") && p_type.contains("]]");
                    let base_type = if is_array {
                        p_type
                            .replace("[[", "")
                            .replace("]]", "")
                            .trim()
                            .to_string()
                    } else {
                        p_type.clone()
                    };
                    params.push(GpuParam {
                        name: p_name,
                        base_type,
                        is_array,
                    });
                } else if !param.is_empty() {
                    params.push(GpuParam {
                        name: param.to_string(),
                        base_type: "int".to_string(),
                        is_array: false,
                    });
                }
            }
        }
    }

    // 3. Extract return type
    if let Some(arrow_pos) = source.find("->") {
        let after_arrow = source[arrow_pos + 2..].trim();
        let ret_end = after_arrow
            .find(|c: char| c.is_whitespace() || c == '{' || c == ';')
            .unwrap_or(after_arrow.len());
        let ret_type = after_arrow[..ret_end].trim().to_string();
        if !ret_type.is_empty() {
            return_type = Some(ret_type);
        }
    }

    // 4. Count for loops using brace-depth tracking per-line with position-based depth tracking
    //    Depth represents current nesting level within the source string, resetting at newlines.
    let mut depth = 0isize;
    for line in source.lines() {
        let trimmed = line.trim();
        // Track opening braces BEFORE checking for "for " keyword
        if let Some(for_pos) = trimmed.find("for ") {
            let before_for = &trimmed[..for_pos];
            depth += before_for.matches('{').count() as isize;
            depth -= before_for.matches('}').count() as isize;

            if depth >= 1 {
                loop_count += 1;
                if depth >= 2 {
                    has_nested_loops = true;
                }
            }

            // Count remaining braces after the keyword on this line
            let after_for = &trimmed[for_pos + 4..];
            depth += after_for.matches('{').count() as isize;
            depth -= after_for.matches('}').count() as isize;
        } else {
            // No "for " keyword on this line — count all braces normally
            for ch in trimmed.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
        }
    }

    GpuAnalysis {
        fn_name,
        params,
        return_type,
        is_parallel: loop_count > 0,
        loop_count,
        has_nested_loops,
    }
}

// ═══════════════════════════════
//  MSL Generator
// ═══════════════════════════════

/// 编译为 Metal Shading Language
pub fn compile_to_msl(source: &str) -> Result<String, String> {
    let analysis = analyze_for_gpu(source);
    if analysis.params.is_empty() {
        return Err("至少一个函数参数用于 GPU 计算".to_string());
    }

    let mut out = String::from("#include <metal_stdlib>\nusing namespace metal;\n\n");

    // Function signature
    write!(out, "kernel void {}(", analysis.fn_name).unwrap();
    for (i, p) in analysis.params.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        if p.is_array {
            let gt = dal_array_type_to_gpu(&p.base_type, "metal");
            out.push_str(&format!(
                "    const device {}* {}_data [[buffer({})]],\n    int {}_len",
                gt, p.name, i, p.name
            ));
        } else {
            let gt = dal_type_to_gpu(&p.base_type, "metal");
            write!(out, "    {} {}", gt, p.name).unwrap();
        }
    }
    out.push_str(",\n    uint tid [[thread_position_in_threadgroup]])\n{\n");

    if analysis.is_parallel && analysis.loop_count > 0 {
        out.push_str("    // Parallel GPU execution\n");
        out.push_str("    int idx = tid;\n");
        out.push_str("    int stride = 256;\n");
        out.push_str("    int max_size = 1024;\n");
        out.push_str("    for (int i = idx; i < max_size; i += stride) {\n");
        for ap in analysis.params.iter().filter(|p| p.is_array) {
            let et = dal_type_to_gpu(&ap.base_type, "metal");
            out.push_str(&format!(
                "        device {}& {}_val = {}_data[i];\n",
                et, ap.name, ap.name
            ));
        }
        for sp in analysis.params.iter().filter(|p| !p.is_array) {
            writeln!(out, "        // scalar: {}", sp.name).unwrap();
        }
        out.push_str("    }\n");
    } else {
        out.push_str("    // Serial fallback\n");
        // Placeholder: 后续集成 compiler/parser AST → MSL statement translation
        out.push_str("    // Serial path — no parallel loop detected, executing sequentially\n");
    }

    out.push_str("}\n");
    Ok(out)
}

// ═══════════════════════════════
//  CUDA Generator
// ═══════════════════════════════

/// 编译为 CUDA C++
pub fn compile_to_cuda(source: &str) -> Result<String, String> {
    let analysis = analyze_for_gpu(source);
    if analysis.params.is_empty() {
        return Err("至少一个函数参数用于 GPU 计算".to_string());
    }

    let mut out = String::from("#include <cuda_runtime.h>\n\n");

    // __global__ function
    write!(out, "__global__ void {}(", analysis.fn_name).unwrap();
    let mut first = true;
    for p in &analysis.params {
        if !first {
            out.push_str(", ");
        }
        first = false;
        if p.is_array {
            let gt = dal_array_type_to_gpu(&p.base_type, "cuda");
            out.push_str(&format!("{} {}_data, int {}_len", gt, p.name, p.name));
        } else {
            let gt = dal_type_to_gpu(&p.base_type, "cuda");
            write!(out, "{} {}", gt, p.name).unwrap();
        }
    }
    if let Some(ref ret) = analysis.return_type {
        let base = ret.replace("[[", "").replace("]]", "");
        let ret_t = if base.is_empty() {
            "int".to_string()
        } else {
            dal_type_to_gpu(&base, "cuda")
        };
        if !first {
            out.push_str(", ");
        }
        write!(out, "{ret_t}* result_data").unwrap();
    }
    out.push_str(")\n{\n");

    if analysis.is_parallel && analysis.loop_count > 0 {
        out.push_str("    // Parallel GPU execution\n");
        out.push_str("    int idx = blockIdx.x * blockDim.x + threadIdx.x;\n");
        out.push_str("    int stride = blockDim.x;\n");
        out.push_str("    for (int i = idx; i < 1024; i += stride) {\n");
        for ap in analysis.params.iter().filter(|p| p.is_array) {
            let et = dal_type_to_gpu(&ap.base_type, "cuda");
            out.push_str(&format!(
                "        {} {}_val = {}_data[i];\n",
                et, ap.name, ap.name
            ));
        }
        out.push_str("    }\n");
    } else {
        out.push_str("    // Serial fallback\n");
        out.push_str("    int tid = blockIdx.x * blockDim.x + threadIdx.x;\n");
        // Placeholder: 后续集成 compiler/parser AST → CUDA statement translation
        out.push_str("    // Serial path — no parallel loop detected, executing sequentially\n");
    }

    out.push_str("}\n\n");

    // Launch helper
    writeln!(out, "// Launch helper for {}", analysis.fn_name).unwrap();
    out.push_str(&format!(
        "static inline __host__ void launch_{}\n",
        analysis.fn_name
    ));
    out.push_str("{\n");
    for p in &analysis.params {
        if p.is_array {
            let gt = dal_array_type_to_gpu(&p.base_type, "cuda");
            out.push_str(&format!(
                "    {} {}_data, int {}_len,\n",
                gt, p.name, p.name
            ));
        } else {
            let gt = dal_type_to_gpu(&p.base_type, "cuda");
            writeln!(out, "    {} {},", gt, p.name).unwrap();
        }
    }
    out.push_str("    int blocks = (n + 255) / 256;\n");
    out.push_str("    int threads = 256;\n");
    out.push_str(&format!(
        "    {}<<<blocks, threads>>>(...);\n",
        analysis.fn_name
    ));
    out.push_str("}\n");

    Ok(out)
}

// ═══════════════════════════════
//  GpuCompiler (Wrapper)
// ═══════════════════════════════

/// GPU 编译器包装器
pub struct GpuCompiler {
    backend: GpuBackend,
    workgroup_size: (u32, u32, u32),
    auto_parallelize: bool,
}

impl Default for GpuCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuCompiler {
    /// 创建新的 GPU 编译器
    #[must_use]
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

    /// 获取当前后端
    #[must_use]
    pub fn get_backend(&self) -> GpuBackend {
        self.backend
    }

    /// 设置工作组大小
    pub fn set_workgroup_size(&mut self, x: u32, y: u32, z: u32) {
        self.workgroup_size = (x, y, z);
    }

    /// 获取工作组大小
    #[must_use]
    pub fn get_workgroup_size(&self) -> (u32, u32, u32) {
        self.workgroup_size
    }

    /// 启用/禁用自动并行化
    pub fn set_auto_parallelize(&mut self, enable: bool) {
        self.auto_parallelize = enable;
    }

    /// 是否启用自动并行化
    #[must_use]
    pub fn get_auto_parallelize(&self) -> bool {
        self.auto_parallelize
    }

    /// 分析函数是否可以 GPU 加速
    #[must_use]
    pub fn analyze(&self, source: &str) -> GpuAnalysis {
        analyze_for_gpu(source)
    }

    /// 编译 Dalin L 源码为 Metal Shading Language
    pub fn compile_to_metal(&self, source: &str) -> Result<String, String> {
        compile_to_msl(source)
    }

    /// 编译 Dalin L 源码为 CUDA C++
    pub fn compile_to_cuda(&self, source: &str) -> Result<String, String> {
        compile_to_cuda(source)
    }

    /// 自动检测并编译到合适的 GPU 后端
    pub fn compile(&self, source: &str) -> Result<String, String> {
        match self.backend {
            GpuBackend::Metal => self.compile_to_metal(source),
            GpuBackend::Cuda => self.compile_to_cuda(source),
            GpuBackend::Auto => {
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
    #[must_use]
    pub fn backend_name(&self) -> &str {
        match self.backend {
            GpuBackend::Metal => "Metal",
            GpuBackend::Cuda => "CUDA",
            GpuBackend::Auto => "Auto",
        }
    }

    /// 编译为指定后端的 GPU 代码
    pub fn compile_to_backend(&self, source: &str, target: GpuBackend) -> Result<String, String> {
        match target {
            GpuBackend::Metal => self.compile_to_metal(source),
            GpuBackend::Cuda => self.compile_to_cuda(source),
            GpuBackend::Auto => self.compile(source),
        }
    }
}

// ═══════════════════════════════
//  Tests
// ═══════════════════════════════

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

    #[test]
    fn test_analyze_simple_scalar_fn() {
        let source = "fn add(a: Int, b: Int) -> Int { return a + b }";
        let analysis = analyze_for_gpu(source);
        assert_eq!(analysis.fn_name, "add");
        assert_eq!(analysis.params.len(), 2);
        assert_eq!(analysis.params[0].name, "a");
        assert_eq!(analysis.params[1].name, "b");
        assert_eq!(analysis.params[0].base_type, "Int");
        assert_eq!(analysis.params[1].base_type, "Int");
        assert!(!analysis.params[0].is_array);
        assert!(!analysis.params[1].is_array);
        assert!(!analysis.is_parallel);
        assert_eq!(analysis.loop_count, 0);
    }

    #[test]
    fn test_analyze_array_fn() {
        let source = r#"fn add_arrays(a: [[Int]], b: [[Int]], n: Int) -> [[Int]] {
  let result = empty(n)
  for i in 0 .. n {
    result[i] = a[i] + b[i]
  }
  return result
}"#;
        let analysis = analyze_for_gpu(source);
        assert_eq!(analysis.fn_name, "add_arrays");
        assert_eq!(analysis.params.len(), 3);
        assert!(analysis.params[0].is_array);
        assert_eq!(analysis.params[0].base_type, "Int");
        assert!(analysis.params[1].is_array);
        assert_eq!(analysis.params[1].base_type, "Int");
        assert!(!analysis.params[2].is_array);
        assert_eq!(analysis.params[2].name, "n");
        assert!(analysis.is_parallel, "expected is_parallel=true, got false");
        assert_eq!(analysis.loop_count, 1);
        assert!(!analysis.has_nested_loops);
    }

    #[test]
    fn test_analyze_nested_loop_fn() {
        let source = r#"fn matrix_sum(a: [[Float]], n: Int) -> Float {
  let total = 0.0
  for i in 0 .. n {
    for j in 0 .. n {
      total = total + a[i][j]
    }
  }
  return total
}"#;
        let analysis = analyze_for_gpu(source);
        assert_eq!(analysis.fn_name, "matrix_sum");
        assert!(analysis.is_parallel);
        assert!(analysis.has_nested_loops);
        assert_eq!(analysis.params[0].base_type, "Float");
    }

    #[test]
    fn test_analyze_for_loop_only_first_level_detected() {
        let source = r#"fn triple_sum(n: Int) -> Int {
  let total = 0
  for i in 0 .. n {
    total = total + i
  }
  return total
}"#;
        let analysis = analyze_for_gpu(source);
        assert!(analysis.is_parallel);
        assert_eq!(analysis.loop_count, 1);
        assert!(!analysis.has_nested_loops);
    }

    #[test]
    fn test_dal_type_to_gpu_int() {
        assert_eq!(dal_type_to_gpu("Int", "metal"), "int");
        assert_eq!(dal_type_to_gpu("int", "cuda"), "int");
    }

    #[test]
    fn test_dal_type_to_gpu_float() {
        assert_eq!(dal_type_to_gpu("Float", "metal"), "float");
        assert_eq!(dal_type_to_gpu("float", "cuda"), "float");
    }

    #[test]
    fn test_dal_type_to_gpu_bool() {
        assert_eq!(dal_type_to_gpu("Bool", "metal"), "bool");
        assert_eq!(dal_type_to_gpu("bool", "cuda"), "bool");
    }

    #[test]
    fn test_dal_type_to_gpu_unknown_defaults_to_int() {
        assert_eq!(dal_type_to_gpu("CustomType", "metal"), "int");
    }

    #[test]
    fn test_dal_array_type_to_gpu_metal() {
        assert_eq!(dal_array_type_to_gpu("Int", "metal"), "const device int*");
        assert_eq!(
            dal_array_type_to_gpu("Float", "metal"),
            "const device float*"
        );
    }

    #[test]
    fn test_dal_array_type_to_gpu_cuda() {
        assert_eq!(dal_array_type_to_gpu("Int", "cuda"), "const int*");
        assert_eq!(dal_array_type_to_gpu("Float", "cuda"), "const float*");
    }

    #[test]
    fn test_compile_to_metal_scalar_fn() {
        let source = "fn add(a: Int, b: Int) -> Int { return a + b }";
        let result = compile_to_msl(source);
        assert!(result.is_ok());
        let metal = result.unwrap();
        assert!(metal.contains("#include <metal_stdlib>"));
        assert!(metal.contains("using namespace metal;"));
        assert!(metal.contains("kernel void add"));
        assert!(metal.contains("thread_position_in_threadgroup"));
    }

    #[test]
    fn test_compile_to_metal_array_fn() {
        let source = "fn vec_add(a: [[Float]], b: [[Float]], n: Int) -> [[Float]]";
        let result = compile_to_msl(source);
        assert!(result.is_ok());
        let metal = result.unwrap();
        assert!(metal.contains("device float*"));
        assert!(metal.contains("kernel void vec_add"));
    }

    #[test]
    fn test_compile_to_cuda_scalar_fn() {
        let source = "fn multiply(a: Int, b: Int) -> Int { return a * b }";
        let result = compile_to_cuda(source);
        assert!(result.is_ok());
        let cuda = result.unwrap();
        assert!(cuda.contains("#include <cuda_runtime.h>"));
        assert!(cuda.contains("__global__ void multiply"));
        assert!(cuda.contains("blockIdx.x"));
    }

    #[test]
    fn test_compile_to_cuda_array_fn() {
        let source = "fn vec_scale(data: [[Float]], scale: Int, n: Int) -> [[Float]]";
        let result = compile_to_cuda(source);
        assert!(result.is_ok());
        let cuda = result.unwrap();
        assert!(cuda.contains("const float*"));
    }

    #[test]
    fn test_gpu_compiler_wrapper_new() {
        let compiler = GpuCompiler::new();
        let analysis = compiler.analyze("fn hello(a: Int, b: Int) -> Int { return a + b }");
        assert_eq!(analysis.fn_name, "hello");
    }

    #[test]
    fn test_gpu_compiler_wrapper_compile_metal() {
        let compiler = GpuCompiler::new();
        let result = compiler.compile_to_metal("fn foo(x: Float) -> Float { return x }");
        assert!(result.is_ok());
    }

    #[test]
    fn test_gpu_compiler_wrapper_compile_cuda() {
        let compiler = GpuCompiler::new();
        let result = compiler.compile_to_cuda("fn bar(y: Int) -> Int { return y }");
        assert!(result.is_ok());
    }

    #[test]
    fn test_gpu_analysis_no_params_returns_error() {
        let result = compile_to_msl("fn no_args() -> Int { return 42 }");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("至少一个函数参数"),
            "error should mention required params"
        );
    }

    #[test]
    fn test_analyze_no_loop_fn_not_parallel() {
        let source =
            "fn max_of_two(a: Int, b: Int) -> Int { if a > b { return a } else { return b } }";
        let analysis = analyze_for_gpu(source);
        assert!(!analysis.is_parallel);
        assert_eq!(analysis.loop_count, 0);
    }

    #[test]
    fn test_analyze_multiple_scalar_params() {
        let source = "fn calculate(a: Int, b: Float, c: Bool) -> Int { return a }";
        let analysis = analyze_for_gpu(source);
        assert_eq!(analysis.params.len(), 3);
        assert_eq!(analysis.params[0].name, "a");
        assert_eq!(analysis.params[0].base_type, "Int");
        assert_eq!(analysis.params[1].name, "b");
        assert_eq!(analysis.params[1].base_type, "Float");
        assert_eq!(analysis.params[2].name, "c");
        assert_eq!(analysis.params[2].base_type, "Bool");
    }

    #[test]
    fn test_analyze_simple_function_no_arrays() {
        let source = "fn square(x: Int) -> Int { return x * x }";
        let analysis = analyze_for_gpu(source);
        assert_eq!(analysis.fn_name, "square");
        assert_eq!(analysis.params.len(), 1);
        assert_eq!(analysis.params[0].name, "x");
        assert_eq!(analysis.params[0].base_type, "Int");
        assert!(!analysis.params[0].is_array);
        assert!(!analysis.is_parallel);
        assert_eq!(analysis.loop_count, 0);
    }
}
