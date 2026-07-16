/// Dalin L 2.0 — TaskSpec：七通道类型系统的"可执行单元"
///
/// 编译器在 TIR 阶段为每个顶层 `fn` 生成一份 `TaskSpec`，把七通道标注
/// (值类型 / 效应 / 能力) 固化为调度契约。控制面（Capability Scheduler）
/// **只读** `effect` / `capability` 字段做 Placement，不再重新推断——
/// 类型系统是唯一事实源。
///
/// 语义映射（lowering 阶段插入）：
///   @spawn f(x) → 子 TaskSpec{ effect: Spawn, parent: current } 入队
///   @async f(x) → TaskSpec{ effect: Async }，返回 Future handle，caller 非阻塞
///   @net   f(x) → TaskSpec{ capability: Net }，调度器路由到远程网关（需 net 凭证）
///   @sfa   f(x) → TaskSpec{ capability: Sfa }，路由到 SFA 路由服务（QN1）
///   plain  f(x) → TaskSpec{ effect: Io|Pure, capability: Cpu }，本地 DLVM 执行

use crate::ast::{Stmt, Program};
use crate::ty2::{parse_capability, parse_effect, parse_governance, Capability, Effect, GovernanceLevel};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 可执行单元：携带七通道契约 + 幂等键
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSpec {
    pub fn_id: String,
    pub effect: Effect,
    pub capability: Capability,
    /// spawn 链：子任务指向父任务；顶层任务为 None
    pub parent_task: Option<String>,
    /// 幂等去重键（控制面 at-least-once 投递 + 幂等消费）
    pub idempotency_key: String,
    /// 是否为 spawn 派生任务（影响配额计数）
    pub is_spawn: bool,
    /// @llm 编译指令生成的 prompt（调试/审计用）
    pub llm_prompt: Option<String>,
    /// Phase C: 治理级别（@gov(prepare|suggest|approve|execute)）
    pub governance_level: Option<GovernanceLevel>,
}

impl TaskSpec {
    /// 由函数名 + 七通道标注构造；幂等键用 (fn_id + 标注) 稳定哈希。
    fn for_fn(name: &str, effect: Effect, capability: Capability, is_spawn: bool, llm_prompt: Option<String>, governance_level: Option<GovernanceLevel>) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        effect.hash(&mut hasher);
        capability.hash(&mut hasher);
        if let Some(ref prompt) = llm_prompt {
            prompt.hash(&mut hasher);
        }
        if let Some(ref gov) = governance_level {
            // 治理级别影响幂等键（不同治理级别 → 不同任务）
            std::mem::discriminant(gov).hash(&mut hasher);
        }
        let key = format!("{:x}", hasher.finish());
        TaskSpec {
            fn_id: name.to_string(),
            effect,
            capability,
            parent_task: None,
            idempotency_key: key,
            is_spawn,
            llm_prompt,
            governance_level,
        }
    }

    /// 由父任务派生一个 spawn 子任务规格（parent 指向父，is_spawn=true，继承父治理级别）
    pub fn spawn_child(&self, child_name: &str, effect: Effect, capability: Capability) -> Self {
        let mut child = TaskSpec::for_fn(child_name, effect, capability, true, None, self.governance_level.clone());
        child.parent_task = Some(self.idempotency_key.clone());
        child
    }
}

/// 从整份 Program 生成顶层 TaskSpec 列表（控制面消费的编译期契约）。
///
/// 这是"编译器 → 控制面"的边界：只暴露七通道标注，不暴露 AST 细节。
pub fn from_program(prog: &Program) -> Vec<TaskSpec> {
    let mut specs = Vec::new();
    for stmt in &prog.statements {
        if let Stmt::Fn { name, effect, capability, llm_prompt, governance, async_, .. } = stmt {
            let eff = effect
                .as_deref()
                .map(parse_effect)
                .unwrap_or(if *async_ { Effect::Async } else { Effect::Pure });
            let cap = capability
                .as_deref()
                .map(parse_capability)
                .unwrap_or(Capability::Cpu);
            let gov = governance.as_deref().map(parse_governance);
            specs.push(TaskSpec::for_fn(name, eff, cap, false, llm_prompt.clone(), gov));
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Program, Stmt};

    fn fn_stmt(name: &str, effect: Option<&str>, capability: Option<&str>, async_: bool) -> Stmt {
        Stmt::Fn {
            name: name.to_string(),
            params: vec![],
            return_type: None,
            effect: effect.map(|s| s.to_string()),
            capability: capability.map(|s| s.to_string()),
            llm_prompt: None,
            confidence: None,
            cognitive_loop: None,
            governance: None,
            latency: None,
            timeout: None,
            throughput: None,
            body: vec![],
            async_,
            pub_: false,
        }
    }

    #[test]
    fn builds_specs_from_annotations() {
        let mut prog = Program::new();
        prog.add(fn_stmt("fetch", Some("async"), Some("net"), false));
        prog.add(fn_stmt("encode", None, Some("sfa"), false));
        prog.add(fn_stmt("stream", None, None, true)); // async fn, 无能力注解
        let specs = from_program(&prog);
        assert_eq!(specs.len(), 3);

        let fetch = &specs[0];
        assert_eq!(fetch.fn_id, "fetch");
        assert_eq!(fetch.effect, Effect::Async);
        assert_eq!(fetch.capability, Capability::Net);

        let encode = &specs[1];
        assert_eq!(encode.capability, Capability::Sfa);
        assert_eq!(encode.effect, Effect::Pure); // 缺失效应 → 回落 Pure

        let stream = &specs[2];
        assert_eq!(stream.effect, Effect::Async); // async fn → Async
        assert_eq!(stream.capability, Capability::Cpu); // 缺失能力 → 回落 Cpu
    }

    #[test]
    fn spawn_child_links_parent_and_marks_spawn() {
        let parent = TaskSpec::for_fn("orchestrator", Effect::Spawn, Capability::Cpu, false, None, None);
        let child = parent.spawn_child("worker", Effect::Io, Capability::Cpu);
        assert!(child.is_spawn);
        assert_eq!(child.parent_task, Some(parent.idempotency_key));
        assert_eq!(child.capability, Capability::Cpu);
    }

    #[test]
    fn idempotency_key_is_stable_per_signature() {
        let a = TaskSpec::for_fn("f", Effect::Io, Capability::Cpu, false, None, None);
        let b = TaskSpec::for_fn("f", Effect::Io, Capability::Cpu, false, None, None);
        assert_eq!(a.idempotency_key, b.idempotency_key);
        let c = TaskSpec::for_fn("f", Effect::Async, Capability::Cpu, false, None, None);
        assert_ne!(a.idempotency_key, c.idempotency_key);
    }
}
