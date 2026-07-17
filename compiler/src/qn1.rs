/// Dalin L 2.0 — QN1/SFA 深度集成
///
/// Phase D 核心组件：将 @llm 编译指令从"模板匹配"升级为"认知架构实时代码生成"。
///
/// 架构：
/// ```text
/// @llm("sort data by timestamp")
///   → LlmEngine
///     → Qn1Backend::generate()      ← 可插拔后端（mock / 真实 QN1）
///       → QN1 认知循环：Perceive → Reason → Decide → Act
///         → 返回 代码 + 置信度 + 延迟 profile
///       → 编译器集成：类型检查 + 延迟验证 + 置信度断言
/// ```
///
/// 接口设计原则：
/// - Qn1Backend trait 是纯异步/同步皆可的接口
/// - MockQn1Backend 用于无 QN1 环境下的开发/测试
/// - 真实 QN1 后端只需实现该 trait 即可接入
use crate::ast::{Expr, Stmt};
use std::collections::HashMap;

/// QN1 代码生成结果
#[derive(Debug, Clone)]
pub struct Qn1GeneratedCode {
    /// 生成的函数体（AST 语句列表）
    pub statements: Vec<Stmt>,
    /// 置信度评分 0.0..1.0
    pub confidence_score: f64,
    /// 估计延迟（毫秒）
    pub estimated_latency_ms: u64,
    /// 认知循环阶段：代码生成器内部走过的认知路径
    pub cognitive_path: Vec<String>,
    /// 原始 prompt
    pub prompt: String,
}

/// QN1 后端 trait — 可插拔接口
///
/// 实现此 trait 即可让 @llm 通过真实 QN1 认知架构进行代码生成。
/// 目前提供 Mock 实现；真实 QN1 后端需要实现 generate() 方法。
pub trait Qn1Backend: std::fmt::Debug {
    /// 根据自然语言描述生成代码
    /// - prompt: @llm("...") 中的自然语言指令
    /// - context: 编译上下文（函数名、参数、已有类型信息）
    fn generate(&self, prompt: &str, context: &GenerationContext) -> Qn1GeneratedCode;

    /// 后端名称（用于调试/审计）
    fn name(&self) -> &str;
}

/// 代码生成上下文
#[derive(Debug, Clone)]
pub struct GenerationContext {
    /// 目标函数名
    pub fn_name: Option<String>,
    /// 函数参数列表
    pub params: Vec<String>,
    /// 已有注解（效应、能力等）
    pub annotations: HashMap<String, String>,
}

impl Default for GenerationContext {
    fn default() -> Self {
        Self::new()
    }
}

impl GenerationContext {
    pub fn new() -> Self {
        Self { fn_name: None, params: Vec::new(), annotations: HashMap::new() }
    }
}

/// QN1 后端配置 — 控制真实后端的连接参数
#[derive(Debug, Clone)]
pub struct Qn1BackendConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
}

impl Default for Qn1BackendConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key: std::env::var("QN1_API_KEY").unwrap_or_default(),
        }
    }
}

/// 真实 QN1 后端 — 通过 OpenAI 兼容 API 调用 LLM 生成代码
#[derive(Debug)]
pub struct RealQn1Backend {
    api_key: String,
    endpoint: String,
    model: String,
    estimated_latency_ms: u64,
}

impl RealQn1Backend {
    pub fn new(config: Qn1BackendConfig) -> Self {
        Self {
            api_key: config.api_key,
            endpoint: config.endpoint,
            model: config.model,
            estimated_latency_ms: 2000,
        }
    }

    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.estimated_latency_ms = latency_ms;
        self
    }
}

impl Qn1Backend for RealQn1Backend {
    fn generate(&self, prompt: &str, _context: &GenerationContext) -> Qn1GeneratedCode {
        let system_prompt = "You are a code generation assistant for the Dalin L programming language. Generate only code, no explanations. Return the code as plain text.";

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.1,
        });

        let result = ureq::post(&self.endpoint)
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(&body);

        match result {
            Ok(response) => {
                let body_str = match response.into_body().read_to_string() {
                    Ok(s) => s,
                    Err(_) => {
                        return Qn1GeneratedCode {
                            statements: vec![
                                Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: failed to read LLM response body".into()))),
                            ],
                            confidence_score: 0.1,
                            estimated_latency_ms: self.estimated_latency_ms,
                            cognitive_path: vec!["error(read)".into()],
                            prompt: prompt.to_string(),
                        };
                    }
                };
                match serde_json::from_str::<serde_json::Value>(&body_str) {
                    Ok(json) => {
                        let code = json["choices"][0]["message"]["content"]
                            .as_str()
                            .unwrap_or("// LLM returned no content")
                            .to_string();

                        let statements = vec![
                            Stmt::Expr(Box::new(Expr::StringLiteral(format!("// LLM generated: {}", code)))),
                        ];

                        Qn1GeneratedCode {
                            statements,
                            confidence_score: 0.85,
                            estimated_latency_ms: self.estimated_latency_ms,
                            cognitive_path: vec![
                                "perceive".into(),
                                "reason".into(),
                                "decide(llm)".into(),
                                "act(generate)".into(),
                            ],
                            prompt: prompt.to_string(),
                        }
                    }
                    Err(_) => {
                        Qn1GeneratedCode {
                            statements: vec![
                                Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: failed to parse LLM response".into()))),
                            ],
                            confidence_score: 0.1,
                            estimated_latency_ms: self.estimated_latency_ms,
                            cognitive_path: vec!["error(parse)".into()],
                            prompt: prompt.to_string(),
                        }
                    }
                }
            }
            Err(_) => {
                Qn1GeneratedCode {
                    statements: vec![
                        Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: LLM request failed".into()))),
                    ],
                    confidence_score: 0.1,
                    estimated_latency_ms: self.estimated_latency_ms,
                    cognitive_path: vec!["error(http)".into()],
                    prompt: prompt.to_string(),
                }
            }
        }
    }

    fn name(&self) -> &str {
        "real-qn1-openai"
    }
}

/// Mock QN1 后端 — 用于开发测试
///
/// 模拟 QN1 认知架构的行为：
/// - 对已知模式做"认知匹配"（对应 SFA 的 PatternMatch 阶段）
/// - 对未知模式做"推理生成"（对应 Act 阶段）
/// - 返回合理的置信度和延迟估值
#[derive(Debug)]
pub struct MockQn1Backend;

impl Default for MockQn1Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockQn1Backend {
    pub fn new() -> Self {
        Self
    }
}

impl Qn1Backend for MockQn1Backend {
    fn generate(&self, prompt: &str, _context: &GenerationContext) -> Qn1GeneratedCode {
        let p = prompt.trim().to_lowercase();

        // 感知阶段：模式匹配（对应 CognitiveLoop::Perceive + Reason）
        let (statements, confidence, path) = if p.contains("sort") && (p.contains("asc") || p.contains("ascending")) {
            // 认知匹配：模式已知 → 高置信度
            (vec![
                Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: sorted ascending".into()))),
            ], 0.95, vec!["perceive".into(), "reason".into(), "decide(pattern_match)".into(), "act".into()])
        } else if p.contains("filter") && (p.contains(">") || p.contains("greater")) {
            (vec![
                Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: filtered > threshold".into()))),
            ], 0.93, vec!["perceive".into(), "reason".into(), "decide(pattern_match)".into(), "act".into()])
        } else if p.contains("sum") || p.contains("total") || p.contains("average") {
            (vec![
                Stmt::Expr(Box::new(Expr::StringLiteral("// QN1: aggregate computation".into()))),
            ], 0.90, vec!["perceive".into(), "reason".into(), "decide(pattern_match)".into(), "act".into()])
        } else {
            // 未知模式：QN1 推理生成（置信度较低，但比纯模板降级高）
            (vec![
                Stmt::Expr(Box::new(Expr::StringLiteral(format!("// QN1 generated: {}", prompt)))),
            ], 0.75, vec!["perceive".into(), "reason".into(), "decide(reasoning)".into(), "act(generate)".into(), "loop".into()])
        };

        Qn1GeneratedCode {
            statements,
            confidence_score: confidence,
            estimated_latency_ms: 15, // 模拟 QN1 推理延迟 15ms
            cognitive_path: path,
            prompt: prompt.to_string(),
        }
    }

    fn name(&self) -> &str {
        "mock-qn1"
    }
}

/// QN1 代码生成器 — 高级封装
///
/// 将 QN1 后端的生成结果映射到编译器消费的格式。
pub struct Qn1CodeGenerator {
    backend: Box<dyn Qn1Backend>,
}

impl Qn1CodeGenerator {
    /// 创建 QN1 代码生成器，指定后端实现
    pub fn new(backend: Box<dyn Qn1Backend>) -> Self {
        Self { backend }
    }

    /// 创建使用 Mock 后端的 QN1 代码生成器
    pub fn new_mock() -> Self {
        Self { backend: Box::new(MockQn1Backend::new()) }
    }

    /// 创建使用真实 LLM 后端的 QN1 代码生成器
    pub fn new_real(config: Qn1BackendConfig) -> Self {
        Self { backend: Box::new(RealQn1Backend::new(config)) }
    }

    /// 生成代码 + 返回置信度和延迟
    pub fn generate(&self, prompt: &str, context: &GenerationContext) -> Qn1GeneratedCode {
        self.backend.generate(prompt, context)
    }

    /// 后端名称
    pub fn backend_name(&self) -> &str {
        self.backend.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_creates_code() {
        let g = Qn1CodeGenerator::new_mock();
        let ctx = GenerationContext::new();
        let result = g.generate("sort ascending", &ctx);
        assert!(!result.statements.is_empty());
        assert_eq!(g.backend_name(), "mock-qn1");
    }

    #[test]
    fn test_mock_backend_confidence_high_for_known_pattern() {
        let g = Qn1CodeGenerator::new_mock();
        let ctx = GenerationContext::new();
        let result = g.generate("filter data where x > 5", &ctx);
        assert!(result.confidence_score >= 0.9);
    }

    #[test]
    fn test_mock_backend_confidence_lower_for_unknown() {
        let g = Qn1CodeGenerator::new_mock();
        let ctx = GenerationContext::new();
        let result = g.generate("complex custom transformation pipeline", &ctx);
        assert!(result.confidence_score < 0.9);
        assert!(result.confidence_score >= 0.7);
    }

    #[test]
    fn test_mock_backend_estimated_latency() {
        let g = Qn1CodeGenerator::new_mock();
        let ctx = GenerationContext::new();
        let result = g.generate("sort ascending", &ctx);
        assert_eq!(result.estimated_latency_ms, 15); // Mock 固定 15ms
    }

    #[test]
    fn test_mock_backend_cognitive_path() {
        let g = Qn1CodeGenerator::new_mock();
        let ctx = GenerationContext::new();
        let result = g.generate("sort ascending", &ctx);
        assert!(!result.cognitive_path.is_empty());
        assert!(result.cognitive_path.contains(&"act".to_string()));
    }

    #[test]
    fn test_context_with_fn_name() {
        let g = Qn1CodeGenerator::new_mock();
        let mut ctx = GenerationContext::new();
        ctx.fn_name = Some("sort_data".to_string());
        ctx.params = vec!["data".to_string(), "asc".to_string()];
        let result = g.generate("sort ascending", &ctx);
        assert!(!result.statements.is_empty());
    }

    #[test]
    fn test_qn1_backend_is_debug() {
        let g = Qn1CodeGenerator::new_mock();
        let _ = format!("{:?}", g.backend_name());
    }

    #[test]
    fn test_real_backend_config_default() {
        let config = Qn1BackendConfig::default();
        assert_eq!(config.endpoint, "https://api.openai.com/v1/chat/completions");
        assert_eq!(config.model, "gpt-4o-mini");
    }

    #[test]
    fn test_real_backend_config_custom() {
        let config = Qn1BackendConfig {
            endpoint: "https://custom.endpoint/v1/chat/completions".to_string(),
            model: "gpt-4".to_string(),
            api_key: "test-key".to_string(),
        };
        assert_eq!(config.endpoint, "https://custom.endpoint/v1/chat/completions");
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.api_key, "test-key");
    }

    #[test]
    fn test_real_backend_construction() {
        let config = Qn1BackendConfig {
            endpoint: "https://test.endpoint/v1/chat/completions".to_string(),
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
        };
        let backend = RealQn1Backend::new(config);
        assert_eq!(backend.name(), "real-qn1-openai");
    }

    #[test]
    fn test_real_backend_with_latency() {
        let config = Qn1BackendConfig {
            endpoint: "https://test.endpoint/v1/chat/completions".to_string(),
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
        };
        let backend = RealQn1Backend::new(config).with_latency(5000);
        let g = Qn1CodeGenerator::new(Box::new(backend));
        let ctx = GenerationContext::new();
        let result = g.generate("test prompt", &ctx);
        assert_eq!(result.estimated_latency_ms, 5000);
    }

    #[test]
    fn test_real_generator_construction() {
        let config = Qn1BackendConfig {
            endpoint: "https://test.endpoint/v1/chat/completions".to_string(),
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
        };
        let g = Qn1CodeGenerator::new_real(config);
        assert_eq!(g.backend_name(), "real-qn1-openai");
    }

    #[test]
    fn test_real_backend_name() {
        let config = Qn1BackendConfig {
            endpoint: "https://test.endpoint/v1/chat/completions".to_string(),
            model: "test-model".to_string(),
            api_key: "test-key".to_string(),
        };
        let backend = RealQn1Backend::new(config);
        assert_eq!(backend.name(), "real-qn1-openai");
    }
}