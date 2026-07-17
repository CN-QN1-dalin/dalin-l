//! 编译器 TaskSpec（dalin_compiler::task_spec）↔ 控制面 gRPC TaskSpec 转换
//!
//! 这是"编译器 → 控制面"边界的具体落地：三通道枚举（Effect / Capability）
//! 序列化为与控制面/运行时约定一致的小写注解字符串。

use dalin_compiler::ty2::{Capability as CompilerCapability, Effect as CompilerEffect};

use crate::TaskSpec as PbTaskSpec;

/// 三通道效应枚举 → 小写注解字符串（pure / io / async / spawn）
pub fn effect_to_str(e: &CompilerEffect) -> String {
    format!("{:?}", e).to_lowercase()
}

/// 三通道能力枚举 → 小写注解字符串（cpu / gpu / sfa / net）
pub fn capability_to_str(c: &CompilerCapability) -> String {
    format!("{:?}", c).to_lowercase()
}

impl From<&dalin_compiler::task_spec::TaskSpec> for PbTaskSpec {
    fn from(spec: &dalin_compiler::task_spec::TaskSpec) -> Self {
        PbTaskSpec {
            effect: effect_to_str(&spec.effect),
            capability: capability_to_str(&spec.capability),
            idempotency_key: spec.idempotency_key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use dalin_compiler::lexer::Lexer;
    use dalin_compiler::parser::Parser;
    use dalin_compiler::task_spec::from_program;

    use crate::TaskSpec as PbTaskSpec;

    #[test]
    fn compiler_spec_converts_to_proto() -> Result<(), String> {
        let src = "fn worker() @ spawn @ cpu { return 1 }";
        let toks = Lexer::new(src).tokenize().map_err(|e| format!("lex error: {}", e))?;
        let prog = Parser::new(toks).parse().map_err(|e| format!("parse error: {}", e))?;
        let specs = from_program(&prog);
        assert!(!specs.is_empty(), "应至少为 worker 生成一个 TaskSpec");
        let pb: PbTaskSpec = (&specs[0]).into();
        assert_eq!(pb.effect, "spawn");
        assert_eq!(pb.capability, "cpu");
        assert!(!pb.idempotency_key.is_empty(), "幂等键必须稳定非空");
        Ok(())
    }
}
