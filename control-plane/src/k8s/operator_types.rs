// ============================================================
// Dalin L 3.0 — K8s Operator Types
// Maps Dalin L 7-channel metadata → Kubernetes resource specs
// ============================================================

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Error types ───────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum OperatorError {
    #[error("invalid spec: {0}")]
    InvalidSpec(String),

    #[error("compilation failed for {func_id}: {msg}")]
    CompilationFailed { func_id: String, msg: String },

    #[error("deployment failed for {name}: {reason}")]
    DeploymentFailed { name: String, reason: String },

    #[error("resource limit exceeded: {0}")]
    ResourceLimit(String),
}

// ─── Effect channel ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Pure,
    Io,
    Async,
    Spawn,
}

impl Effect {
    pub fn resource_hints(&self) -> (&'static str, &'static str) {
        match self {
            Effect::Pure => ("100m", "128Mi"),
            Effect::Io => ("250m", "256Mi"),
            Effect::Async => ("500m", "512Mi"),
            Effect::Spawn => ("1", "1Gi"),
        }
    }
}

impl std::str::FromStr for Effect {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pure" => Ok(Effect::Pure),
            "io" => Ok(Effect::Io),
            "async" => Ok(Effect::Async),
            "spawn" => Ok(Effect::Spawn),
            other => Err(format!("Unknown effect: {}", other)),
        }
    }
}

// ─── Capability channel ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Cpu,
    Gpu,
    Sfa,
    Net,
    Mixed,
}

impl Capability {
    pub fn node_selector(&self) -> std::collections::HashMap<String, String> {
        match self {
            Capability::Gpu => {
                let mut m = std::collections::HashMap::new();
                m.insert("nvidia.com/gpu".into(), "true".into());
                m.insert("gpu.vendor".into(), "nvidia".into());
                m
            }
            Capability::Sfa => {
                let mut m = std::collections::HashMap::new();
                m.insert("accelerator.type".into(), "sfa".into());
                m.insert("accelerator.family".into(), "qn1".into());
                m
            }
            Capability::Net => {
                let mut m = std::collections::HashMap::new();
                m.insert("network.speed".into(), "10gbit".into());
                m
            }
            _ => std::collections::HashMap::new(),
        }
    }

    pub fn needs_gpu(&self) -> bool {
        matches!(self, Capability::Gpu | Capability::Mixed)
    }

    pub fn needs_sfa(&self) -> bool {
        *self == Capability::Sfa
    }
}

impl std::str::FromStr for Capability {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cpu" => Ok(Capability::Cpu),
            "gpu" => Ok(Capability::Gpu),
            "sfa" => Ok(Capability::Sfa),
            "net" => Ok(Capability::Net),
            "mixed" => Ok(Capability::Mixed),
            other => Err(format!("Unknown capability: {}", other)),
        }
    }
}

// ─── Confidence level ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
    Verified,
    AutoRecover,
}

impl ConfidenceLevel {
    pub fn replica_strategy(&self, requested: u32) -> ReplicaStrategy {
        match self {
            ConfidenceLevel::Verified if requested >= 2 => {
                ReplicaStrategy::Quorum(requested, requested * 2)
            }
            ConfidenceLevel::AutoRecover => {
                ReplicaStrategy::SelfHealing(3, 1000)
            }
            ConfidenceLevel::High => ReplicaStrategy::Minimum(1),
            _ => ReplicaStrategy::Fixed(requested as usize),
        }
    }
}

impl std::str::FromStr for ConfidenceLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(ConfidenceLevel::Low),
            "medium" => Ok(ConfidenceLevel::Medium),
            "high" => Ok(ConfidenceLevel::High),
            "verified" => Ok(ConfidenceLevel::Verified),
            "auto_recover" => Ok(ConfidenceLevel::AutoRecover),
            other => Err(format!("Unknown confidence: {}", other)),
        }
    }
}

// ─── Governance level ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceLevel {
    None,
    Basic,
    Audit,
    Trace,
    Full,
}

impl std::str::FromStr for GovernanceLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(GovernanceLevel::None),
            "basic" => Ok(GovernanceLevel::Basic),
            "audit" => Ok(GovernanceLevel::Audit),
            "trace" => Ok(GovernanceLevel::Trace),
            "full" => Ok(GovernanceLevel::Full),
            other => Err(format!("Unknown governance: {}", other)),
        }
    }
}

// ─── Cognitive loop ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CognitiveLoopType {
    Observe,
    Reason,
    Decide,
    Act,
    Reflect,
    Sense,
}

impl std::str::FromStr for CognitiveLoopType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "observe" => Ok(CognitiveLoopType::Observe),
            "reason" => Ok(CognitiveLoopType::Reason),
            "decide" => Ok(CognitiveLoopType::Decide),
            "act" => Ok(CognitiveLoopType::Act),
            "reflect" => Ok(CognitiveLoopType::Reflect),
            "sense" => Ok(CognitiveLoopType::Sense),
            other => Err(format!("Unknown cognitive loop: {}", other)),
        }
    }
}

// ─── Resources ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub requests: Option<ResourceList>,
    pub limits: Option<ResourceList>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceList {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    #[serde(rename = "nvidia.com/gpu", skip_serializing_if = "Option::is_none")]
    pub gpu: Option<String>,
}

// ─── Replica strategies ────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ReplicaStrategy {
    Fixed(usize),
    Minimum(usize),
    Quorum(u32, u32),
    SelfHealing(usize, u64),
}

// ─── DalinTaskSpec (maps to K8s CRD) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DalinTaskSpec {
    pub function_id: String,
    pub cognitive_id: Option<String>,
    pub effect: Effect,
    pub capability: Capability,
    pub latency_constraint_ms: Option<u64>,
    pub throughput_min: Option<u64>,
    pub throughput_max: Option<u64>,
    pub governance: GovernanceLevel,
    pub confidence: ConfidenceLevel,
    pub cognitive_loop: Option<CognitiveLoopType>,
    pub replicas: u32,
    pub max_replicas: Option<u32>,
    pub min_replicas: Option<u32>,
    pub resources: Option<ResourceRequirements>,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
    pub retry_backoff_ms: u64,
    pub source_file: Option<String>,
    pub source_git_repo: Option<String>,
    pub source_git_ref: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub annotations: std::collections::HashMap<String, String>,
}

impl Default for DalinTaskSpec {
    fn default() -> Self {
        Self {
            function_id: String::new(),
            cognitive_id: None,
            effect: Effect::Io,
            capability: Capability::Cpu,
            latency_constraint_ms: None,
            throughput_min: None,
            throughput_max: None,
            governance: GovernanceLevel::None,
            confidence: ConfidenceLevel::Medium,
            cognitive_loop: None,
            replicas: 1,
            max_replicas: Some(10),
            min_replicas: Some(1),
            resources: None,
            timeout_seconds: 300,
            retry_attempts: 3,
            retry_backoff_ms: 1000,
            source_file: None,
            source_git_repo: None,
            source_git_ref: None,
            tags: Vec::new(),
            annotations: std::collections::HashMap::new(),
        }
    }
}

// ─── Status ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TaskPhase {
    #[default]
    Pending,
    Compiling,
    Scheduled,
    Running,
    Completed,
    Failed,
    Recovering,
    Unknown,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCondition {
    pub r#type: String,
    pub status: String,
    #[serde(rename = "lastTransitionTime")]
    pub last_transition_time: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_latency_ms: f64,
    pub recovery_events: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DalinTaskStatus {
    pub phase: TaskPhase,
    pub ready_replicas: Option<u32>,
    pub observed_generation: Option<u64>,
    pub conditions: Vec<TaskCondition>,
    pub stats: Option<ExecutionStats>,
    pub last_run_at: Option<String>,
}

impl DalinTaskStatus {
    pub fn transition(&mut self, new_phase: TaskPhase) {
        use std::time::SystemTime;
        self.phase = new_phase.clone();
        // Use a simple timestamp string since chrono is not a dependency
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.conditions.push(TaskCondition {
            r#type: "PhaseChange".into(),
            status: "True".into(),
            last_transition_time: format!("{}ms", ts),
            message: Some(format!("Phase changed to {:?}", new_phase)),
        });
    }
}

// ─── Resource resolver ─────────────────────────────────────────

pub struct ResourceResolver;

impl ResourceResolver {
    pub fn resolve(spec: &DalinTaskSpec) -> Result<ResourceRequirements, OperatorError> {
        let mut reqs = spec.resources.clone().unwrap_or_default();
        let mut limits = ResourceRequirements::default();

        // 1) Base CPU/memory from effect channel
        let (base_cpu, base_mem) = spec.effect.resource_hints();
        let _ = reqs.requests.get_or_insert_with(ResourceList::default);
        if let Some(ref mut r) = reqs.requests {
            r.cpu.get_or_insert_with(|| base_cpu.to_string());
            r.memory.get_or_insert_with(|| base_mem.to_string());
        }

        // Copy to limits (x2)
        if let Some(ref r) = reqs.requests {
            let l = limits.limits.get_or_insert_with(ResourceList::default);
            if let Some(ref cpu) = r.cpu {
                l.cpu = Some(format!("{}x2", cpu.trim_end_matches('m')));
            }
            if let Some(ref mem) = r.memory {
                l.memory = Some(format!("{}x2", mem));
            }
        }

        // 2) GPU mapping
        if spec.capability.needs_gpu() {
            let _ = reqs.requests.get_or_insert_with(ResourceList::default);
            let _ = limits.limits.get_or_insert_with(ResourceList::default);
            if let Some(ref mut r) = reqs.requests {
                r.gpu = Some("1".into());
            }
            if let Some(ref mut l) = limits.limits {
                l.gpu = Some("1".into());
            }
        }

        // 3) Ultra-low latency ceiling
        if let Some(latency) = spec.latency_constraint_ms {
            if latency < 10 {
                limits.limits.get_or_insert_with(ResourceList::default).cpu = Some("2".into());
            } else if latency < 100 {
                limits.limits.get_or_insert_with(ResourceList::default).cpu = Some("1".into());
            }
        }

        Ok(reqs)
    }

    pub fn node_selector(spec: &DalinTaskSpec) -> std::collections::HashMap<String, String> {
        spec.capability.node_selector()
    }

    pub fn replica_strategy(spec: &DalinTaskSpec) -> ReplicaStrategy {
        spec.confidence.replica_strategy(spec.replicas)
    }

    pub fn deployment_name(function_id: &str) -> String {
        format!("dalin-task-{}", function_id)
    }

    pub fn pod_labels(spec: &DalinTaskSpec) -> std::collections::HashMap<String, String> {
        let mut labels = std::collections::HashMap::new();
        labels.insert("app.dalin.ai/task".into(), spec.function_id.clone());
        labels.insert("app.dalin.ai/effect".into(), format!("{:?}", spec.effect).to_lowercase());
        labels.insert("app.dalin.ai/capability".into(), format!("{:?}", spec.capability).to_lowercase());
        labels.insert("app.dalin.ai/confidence".into(), format!("{:?}", spec.confidence).to_lowercase().replace("_", "-"));
        for tag in &spec.tags {
            labels.insert(format!("tag/{}", tag.replace('/', "-")), "true".into());
        }
        labels
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> DalinTaskSpec {
        DalinTaskSpec {
            function_id: "test-agent".into(),
            effect: Effect::Io,
            capability: Capability::Cpu,
            confidence: ConfidenceLevel::Medium,
            replicas: 2,
            ..Default::default()
        }
    }

    #[test]
    fn effect_resource_hints() {
        assert_eq!(Effect::Pure.resource_hints(), ("100m", "128Mi"));
        assert_eq!(Effect::Spawn.resource_hints(), ("1", "1Gi"));
    }

    #[test]
    fn capability_node_selector() {
        let gs = Capability::Gpu.node_selector();
        assert_eq!(gs.get("nvidia.com/gpu"), Some(&"true".into()));

        let sfa_sel = Capability::Sfa.node_selector();
        assert_eq!(sfa_sel.get("accelerator.type"), Some(&"sfa".into()));

        let empty = Capability::Cpu.node_selector();
        assert!(empty.is_empty());
    }

    #[test]
    fn capability_needs_gpu() {
        assert!(Capability::Gpu.needs_gpu());
        assert!(Capability::Mixed.needs_gpu());
        assert!(!Capability::Cpu.needs_gpu());
    }

    #[test]
    fn resource_resolver_basic() {
        let spec = sample_spec();
        let resolved = ResourceResolver::resolve(&spec).unwrap();
        assert_eq!(resolved.requests.as_ref().unwrap().cpu.as_deref(), Some("250m"));
        assert_eq!(resolved.requests.as_ref().unwrap().memory.as_deref(), Some("256Mi"));
    }

    #[test]
    fn resource_resolver_gpu() {
        let spec = DalinTaskSpec {
            function_id: "gpu-func".into(),
            effect: Effect::Spawn,
            capability: Capability::Gpu,
            ..Default::default()
        };
        let resolved = ResourceResolver::resolve(&spec).unwrap();
        assert_eq!(resolved.requests.as_ref().unwrap().gpu.as_deref(), Some("1"));
    }

    #[test]
    fn resource_resolver_node_selector() {
        let spec = DalinTaskSpec {
            function_id: "gpu-func".into(),
            capability: Capability::Gpu,
            ..Default::default()
        };
        let sel = ResourceResolver::node_selector(&spec);
        assert_eq!(sel.get("nvidia.com/gpu"), Some(&"true".into()));
    }

    #[test]
    fn confidence_replica_strategy() {
        let spec = DalinTaskSpec {
            confidence: ConfidenceLevel::Verified,
            replicas: 3,
            ..Default::default()
        };
        assert!(matches!(ResourceResolver::replica_strategy(&spec), ReplicaStrategy::Quorum(_, _)));

        let spec2 = DalinTaskSpec {
            confidence: ConfidenceLevel::AutoRecover,
            replicas: 1,
            ..Default::default()
        };
        assert!(matches!(ResourceResolver::replica_strategy(&spec2), ReplicaStrategy::SelfHealing(..)));
    }

    #[test]
    fn task_status_transition() {
        let mut status = DalinTaskStatus::default();
        assert_eq!(status.phase, TaskPhase::Pending);

        status.transition(TaskPhase::Compiling);
        assert_eq!(status.phase, TaskPhase::Compiling);
        assert_eq!(status.conditions.len(), 1);
        assert!(status.conditions[0].message.as_ref().unwrap().contains("Compiling"));
    }
}
