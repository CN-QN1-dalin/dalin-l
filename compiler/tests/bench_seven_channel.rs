//! Dalin L 3.0 — Seven-Channel Type Inference benchmarks
//!
//! Measures per-channel inference overhead and composite scoring.

#[test]
fn bench_effect_check_performance() {
    use dalin_compiler::ty2::{EffectInferencer, Effect};
    
    let mut inferencer = EffectInferencer::new();
    
    // Simulate 100 expressions
    for i in 0..100 {
        let eff = if i % 3 == 0 { Effect::Async } 
                 else if i % 3 == 1 { Effect::Io }
                 else { Effect::Pure };
        inferencer.check(&eff, &eff, "test_expr");
    }
    
    // No panics = pass; same-effect checks should not produce errors
    assert!(inferencer.errors.is_empty(), "Same-effect checks should not produce errors");
}

#[test]
fn bench_capability_check_performance() {
    use dalin_compiler::ty2::{CapabilityInferencer, Capability};
    
    let mut inferencer = CapabilityInferencer::new();
    
    for i in 0..30 {
        let cap = match i % 3 {
             0 => Capability::Cpu,
            1 => Capability::Net,
            _ => Capability::Gpu,
        };
        inferencer.check(&cap, &cap, "same_cap");
    }
    
    assert!(inferencer.errors.is_empty(), "Same-capability checks should pass");
}

#[test]
fn bench_cognitive_loop_check() {
    use dalin_compiler::ty2::{CognitiveLoopInferencer, CognitiveLoop};
    
    let mut inferencer = CognitiveLoopInferencer::new();
    
    for _ in 0..20 {
        inferencer.check(&CognitiveLoop::Perceive, &CognitiveLoop::Perceive, "test");
    }
    
    assert!(inferencer.errors.is_empty(), "Matching loops should pass");
}

#[test]
fn bench_governance_check() {
    use dalin_compiler::ty2::{GovernanceInferencer, GovernanceLevel};
    
    let mut inferencer = GovernanceInferencer::new();
    let required = GovernanceLevel::Approve;
    
    for _ in 0..15 {
        inferencer.check(&required, &required, "audit_check");
    }
    
    assert!(inferencer.errors.is_empty(), "Same governance level should pass");
}

#[test]
fn bench_time_constraint_meet() {
    use dalin_compiler::ty2::TimeConstraint;
    
    let tc1 = TimeConstraint {
        latency_ms: Some(100),
        timeout_ms: Some(500),
        throughput: None,
    };
    let tc2 = TimeConstraint {
        latency_ms: Some(200),
        timeout_ms: Some(1000),
        throughput: None,
    };
    
    let meet_result = TimeConstraint::meet(&tc1, &tc2);
    assert_eq!(meet_result.latency_ms, Some(100), "Meet takes min (stricter) latency");
    assert_eq!(meet_result.timeout_ms, Some(500), "Meet takes min (stricter) timeout");
}

#[test]
fn bench_confidence_score_boundary() {
    use dalin_compiler::ty2::Confidence;
    
    // Confidence 为五档枚举；score() 映射为固定数值
    assert_eq!(Confidence::Proven.score(), 1.0, "Proven scores 1.0");
    assert_eq!(Confidence::Uncertain.score(), 0.5, "Uncertain scores 0.5");
    assert!((Confidence::Generated.score() - 0.7).abs() < 1e-9, "Generated scores ~0.7");
    // 偏序：Proven 高于 Uncertain
    assert!(Confidence::Proven.score() > Confidence::Uncertain.score());
}

#[test]
fn bench_seven_channel_composite() {
    // 七通道错误构造 + Display 格式化（ChannelError 现为带命名词段的枚举）
    for i in 0..50 {
        let err = format!("channel_check_{}", i);
        let sev = dalin_compiler::error::SourceLocation {
            filename: "".to_string(),
            line: i + 1,
            column: 0,
        };
        let error = dalin_compiler::error::ChannelError::EffectViolation {
            location: sev,
            context: "test".to_string(),
            required: "pure".to_string(),
            detail: err.clone(),
        };
        
        let err_str = format!("{}", error);
        assert!(!err_str.is_empty(), "Error display should not be empty");
    }
}
