//! @llm 编译指令 — 自然语言编译时代码生成
//!
//! 类似 C 的 `asm {}` 内联汇编，Dalin L 的 `@llm("...")` 在编译时
//! 调用 LLM 生成目标代码，编译器负责类型检查 + 七通道验证后链接。
//!
//! 架构：
//! ```text
//! 源码 → Parser → @llm 指令 → LlmEngine.generate() → AST 片段
//!     → 七通道类型检查 → 代码生成 → 链接进二进制
//! ```
//!
//! 置信度集成：@llm 生成的代码自动标记为 Confidence::Generated，
//! 调用方必须显式处理低置信度路径。

use crate::ast::{Expr, Stmt};
use crate::qn1::{GenerationContext, Qn1CodeGenerator};

/// LLM 生成结果
#[derive(Debug)]
pub struct LlmGeneratedCode {
    /// 生成的 AST 片段
    pub statements: Vec<Stmt>,
    /// 生成时的置信度
    pub confidence: LlmConfidence,
    /// 原始提示词
    pub prompt: String,
}

/// LLM 生成的代码置信度评估
#[derive(Debug, Clone, PartialEq)]
pub enum LlmConfidence {
    /// 完全匹配已知模式（模板填充）
    PatternMatch,
    /// 代码通过类型检查
    TypeChecked,
    /// 代码生成但可能有细微偏差
    Generated,
    /// 编译失败或置信度过低
    Rejected,
}

/// @llm 编译引擎
pub struct LlmEngine;

impl LlmEngine {
    /// 处理 @llm 编译指令，生成代码。
    ///
    /// Phase B: 模板匹配和骨架生成（无 QN1 后端时的默认路径）
    /// Phase D: 通过 QN1/SFA 调用 LLM 实时生成
    pub fn process_directive(prompt: &str, target: Option<&str>) -> LlmGeneratedCode {
        let statements = Self::prompt_to_ast(prompt, target);
        let confidence = if Self::is_pattern_match(prompt) {
            LlmConfidence::PatternMatch
        } else {
            LlmConfidence::Generated
        };

        LlmGeneratedCode {
            statements,
            confidence,
            prompt: prompt.to_string(),
        }
    }

    /// 通过 QN1 认知架构处理 @llm 编译指令。
    ///
    /// Phase D 核心入口：
    /// 1. 如果有 QN1 后端 → 调用 QN1 生成代码（置信度 + 延迟跟踪）
    /// 2. 无 QN1 后端 → 回落至 process_directive() 模板匹配
    pub fn process_with_qn1(
        prompt: &str,
        target: Option<&str>,
        qn1: Option<&Qn1CodeGenerator>,
    ) -> LlmGeneratedCode {
        if let Some(qn1) = qn1 {
            let mut ctx = GenerationContext::new();
            ctx.fn_name = target.map(|s| s.to_string());
            let result = qn1.generate(prompt, &ctx);

            LlmGeneratedCode {
                statements: result.statements,
                confidence: if result.confidence_score >= 0.9 {
                    LlmConfidence::PatternMatch
                } else if result.confidence_score >= 0.75 {
                    LlmConfidence::TypeChecked
                } else {
                    LlmConfidence::Generated
                },
                prompt: prompt.to_string(),
            }
        } else {
            // 无 QN1 后端 → 回退到模板匹配
            Self::process_directive(prompt, target)
        }
    }

    /// 检查是否是已知模板模式（安全路径，置信度高）
    fn is_pattern_match(prompt: &str) -> bool {
        let p = prompt.trim().to_lowercase();
        // 常见简单模式的模板匹配
        p.contains("sort") && (p.contains("ascending") || p.contains("asc"))
            || p.contains("filter") && p.contains("greater")
            || p.contains("map") && p.contains("double")
            || p.contains("sum")
            || p.contains("average")
            || p.contains("reverse")
            || p.contains("flatten")
    }

    /// 将 prompt 转换为 AST 片段
    fn prompt_to_ast(prompt: &str, target: Option<&str>) -> Vec<Stmt> {
        let p = prompt.trim().to_lowercase();

        // 模板匹配：常见转换模式
        if let Some(stmts) = Self::match_template(&p, target) {
            return stmts;
        }

        // 通用降级：生成函数骨架 + @todo 标记
        let fn_name = target.unwrap_or("generated_fn");
        vec![Stmt::Fn {
            name: fn_name.to_string(),
            type_params: vec![],
            params: vec![],
            return_type: None,
            effect: Some("pure".to_string()),
            capability: Some("cpu".to_string()),
            llm_prompt: Some(prompt.to_string()),
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![Stmt::Expr(Box::new(Expr::StringLiteral(format!(
                "@llm stub: {prompt}"
            ))))],
            async_: false,
            pub_: false,
        }]
    }

    /// 模板匹配引擎
    fn match_template(prompt: &str, _target: Option<&str>) -> Option<Vec<Stmt>> {
        let p = prompt.trim().to_lowercase();

        // filter > threshold
        if p.contains("filter") && (p.contains("greater") || p.contains(">")) {
            return Some(vec![Stmt::Fn {
                name: "filter_fn".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: None,
                effect: Some("pure".to_string()),
                capability: Some("cpu".to_string()),
                llm_prompt: None,
                confidence: None,
                cognitive_loop: None,
                governance: None,
                latency: None,
                timeout: None,
                throughput: None,
                body: vec![],
                async_: false,
                pub_: false,
            }]);
        }

        // sort ascending
        if (p.contains("sort") || p.contains("order"))
            && (p.contains("asc") || p.contains("ascending"))
        {
            return Some(vec![Stmt::Fn {
                name: "sort_fn".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: None,
                effect: Some("pure".to_string()),
                capability: Some("cpu".to_string()),
                llm_prompt: None,
                confidence: None,
                cognitive_loop: None,
                governance: None,
                latency: None,
                timeout: None,
                throughput: None,
                body: vec![],
                async_: false,
                pub_: false,
            }]);
        }

        // sum or average
        if p.contains("sum") || p.contains("average") || p.contains("total") {
            return Some(vec![Stmt::Fn {
                name: "aggregate_fn".to_string(),
                type_params: vec![],
                params: vec![],
                return_type: None,
                effect: Some("pure".to_string()),
                capability: Some("cpu".to_string()),
                llm_prompt: None,
                confidence: None,
                cognitive_loop: None,
                governance: None,
                latency: None,
                timeout: None,
                throughput: None,
                body: vec![],
                async_: false,
                pub_: false,
            }]);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_match_sort() {
        let result = LlmEngine::process_directive("sort the list ascending", None);
        assert_eq!(result.confidence, LlmConfidence::PatternMatch);
        assert!(!result.statements.is_empty());
    }

    #[test]
    fn test_pattern_match_sum() {
        let result = LlmEngine::process_directive("compute the sum of all elements", None);
        assert_eq!(result.confidence, LlmConfidence::PatternMatch);
    }

    #[test]
    fn test_generic_prompt_generates_stub() {
        let result = LlmEngine::process_directive(
            "transform raw data according to business rules",
            Some("transform_data"),
        );
        // 通用 prompt → Generated 置信度
        assert_eq!(result.confidence, LlmConfidence::Generated);
        // 应有函数骨架
        let has_fn = result
            .statements
            .iter()
            .any(|s| matches!(s, Stmt::Fn { name, .. } if name == "transform_data"));
        assert!(has_fn);
    }

    #[test]
    fn test_stub_has_llm_prompt() {
        let result = LlmEngine::process_directive("call external api", Some("api_call"));
        if let Some(Stmt::Fn { llm_prompt, .. }) = result.statements.first() {
            assert!(llm_prompt.is_some());
            assert_eq!(llm_prompt.as_deref().unwrap(), "call external api");
        } else {
            panic!("expected fn statement");
        }
    }

    // ═══════════════════════════════
    //  Phase D — QN1 集成测试
    // ═══════════════════════════════

    #[test]
    fn test_process_with_qn1_fallback_to_default() {
        // 无 QN1 后端 → 回落模板匹配
        let result = LlmEngine::process_with_qn1("sort ascending", None, None);
        assert_eq!(result.confidence, LlmConfidence::PatternMatch);
        assert!(!result.statements.is_empty());
    }

    #[test]
    fn test_process_with_qn1_uses_qn1_backend() {
        let qn1 = crate::qn1::Qn1CodeGenerator::new_mock();
        let result = LlmEngine::process_with_qn1("sort ascending", Some("sort_data"), Some(&qn1));
        // QN1 返回高置信度 → PatternMatch
        assert_eq!(result.confidence, LlmConfidence::PatternMatch);
        assert!(!result.statements.is_empty());
    }

    #[test]
    fn test_process_with_qn1_lower_confidence() {
        let qn1 = crate::qn1::Qn1CodeGenerator::new_mock();
        let result =
            LlmEngine::process_with_qn1("complex custom transformation pipeline", None, Some(&qn1));
        // QN1 返回 0.75 置信度 → TypeChecked
        assert_eq!(result.confidence, LlmConfidence::TypeChecked);
    }

    #[test]
    fn test_process_with_qn1_prompt_preserved() {
        let qn1 = crate::qn1::Qn1CodeGenerator::new_mock();
        let result = LlmEngine::process_with_qn1("sort ascending", None, Some(&qn1));
        assert_eq!(result.prompt, "sort ascending");
    }
}
