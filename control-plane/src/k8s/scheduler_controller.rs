# ============================================================
# DalinTask Scheduler Controller
# Watches DalinTask CRD → creates Kubernetes Deployments
# Maps 7-channel metadata → resource requests/nodeSelector/pod-template
# ============================================================

use kube::{
    Api, Client, CustomResource,
    runtime::{controller, Controller, watcher::Watcher},
};
use kube::core::{DynamicObject, WatchRequest};
use kube::config::Configuration;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error, debug, info_span};

// ─── Error types ───────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum OperatorError {
    #[error("kube API error: {0}")]
    Kube(#[from] kube::Error),

    #[error("invalid DalinTask spec: {0}")]
    InvalidSpec(String),

    #[error("compilation failed for {func_id}: {msg}")]
    CompilationFailed { func_id: String, msg: String },

    #[error("resource limit exceeded: {msg}")]
    ResourceLimit(String),

    #[error("deployment failed for {name}: {reason}")]
    DeploymentFailed { name: String, reason: String },
}

// ─── DalinTaskSpec ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Pure,
    Io,
    Async,
    Spawn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Cpu,
    Gpu,
    Sfa,
    Net,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencyConstraint {
    pub max_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
    Verified,
    AutoRecover,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceLevel {
    None,
    Basic,
    Audit,
    Trace,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CognitiveLoopType {
    Observe,
    Reason,
    Decide,
    Act,
    Reflect,
    Sense,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    #[serde(default)]
    pub requests: Option<ResourceList>,
    #[serde(default)]
    pub limits: Option<ResourceList>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceList {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DalinTaskSpec {
    // Identity
    pub function_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cognitive_id: Option<String>,

    // 7-Channel metadata
    pub effect: Effect,
    pub capability: Capability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_constraint: Option<LatencyConstraint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_min: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_max: Option<u64>,
    pub governance: GovernanceLevel,
    pub confidence: ConfidenceLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cognitive_loop: Option<CognitiveLoopType>,

    // Scaling
    pub replicas: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_replicas: Option<u32>,

    // Resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,

    // Timeout/retry
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
    #[serde(default = "default_backoff_ms")]
    pub retry_backoff_ms: u64,

    // Source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_git_repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_git_ref: Option<String>,

    // Tags
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    // Extra annotations from source
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub annotations: std::collections::HashMap<String, String>,
}

fn default_timeout() -> u64 { 300 }
fn default_retry_attempts() -> u32 { 3 }
fn default_backoff_ms() -> u64 { 1000 }

// ─── DalinTask status ──────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DalinTaskStatus {
    pub phase: TaskPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<u64>,
    #[serde(default)]
    pub conditions: Vec<TaskCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ExecutionStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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

// ─── The DalinTask CRD binding ─────────────────────────────────

/// DalinTask is a custom resource representing a compiled Dalin L task.
#[derive(Debug, Clone, CustomResource, kube::CustomResourceExt)]
#[kube(group = "dalin.ai", version = "v1alpha1", kind = "DalinTask", namespaced)]
#[kube(status = "DalinTaskStatus", shortname = "dt")]
#[kube(scrollback = 100)]
pub struct DalinTaskSpecFn {
    pub spec: DalinTaskSpec,
}

impl DalinTaskSpecFn {
    /// Get the effective namespace (metadata.namespace or cluster-scoped).
    pub fn namespace(&self) -> Option<&str> {
        self.metadata.namespace.as_deref()
    }

    /// Get a metadata field by key.
    pub fn annotation(&self, key: &str) -> Option<&str> {
        self.metadata.annotations.get(key).map(|s| s.as_str())
    }

    /// Return the owner reference for HPA targeting.
    pub fn deployment_name(&self) -> String {
        format!("dalin-task-{}", self.function_id)
    }
}

// ─── Resource resolver ─────────────────────────────────────────

/// Translates Dalin L 7-channel metadata into Kubernetes resource specs.
pub struct ResourceResolver;

impl ResourceResolver {
    /// Resolve full resource spec from DalinTaskSpec.
    /// Combines explicit requests + defaults based on channels.
    pub fn resolve(spec: &DalinTaskSpec) -> Result<ResourceRequirements, OperatorError> {
        let mut reqs = spec.resources.clone().unwrap_or_default();
        let mut limits = ResourceRequirements::default();

        // 1) Default CPU/memory based on effect channel
        match &spec.effect {
            Effect::Pure => {
                reqs.ensure_default_cpu("100m");
                reqs.ensure_default_memory("128Mi");
            }
            Effect::Io => {
                reqs.ensure_default_cpu("250m");
                reqs.ensure_default_memory("256Mi");
            }
            Effect::Async => {
                reqs.ensure_default_cpu("500m");
                reqs.ensure_default_memory("512Mi");
            }
            Effect::Spawn => {
                reqs.ensure_default_cpu("1");
                reqs.ensure_default_memory("1Gi");
            }
        }

        // 2) GPU mapping
        if let Capability::Gpu | Capability::Sfa | Capability::Mixed = spec.capability {
            reqs.add_gpu("1");
            limits.add_gpu("1");
        }

        // 3) Latency constraint → CPU ceiling
        if let Some(lc) = &spec.latency_constraint {
            if lc.max_ms < 10 {
                // Ultra-low latency → dedicated CPU core
                reqs.set_cpu_limit("2");
            } else if lc.max_ms < 100 {
                reqs.set_cpu_limit("1");
            }
        }

        Ok(reqs)
    }

    /// Generate node selector based on capability channel.
    pub fn node_selector(spec: &DalinTaskSpec) -> std::collections::HashMap<String, String> {
        match spec.capability {
            Capability::Gpu => {
                let mut sel = std::collections::HashMap::new();
                sel.insert("nvidia.com/gpu".to_string(), "true".to_string());
                sel.insert("gpu.vendor".to_string(), "nvidia".to_string());
                sel
            }
            Capability::Sfa => {
                let mut sel = std::collections::HashMap::new();
                sel.insert("accelerator.type".to_string(), "sfa".to_string());
                sel.insert("accelerator.family".to_string(), "qn1".to_string());
                sel
            }
            Capability::Net => {
                let mut sel = std::collections::HashMap::new();
                sel.insert("network.speed".to_string(), "10gbit".to_string());
                sel
            }
            _ => std::collections::HashMap::new(),
        }
    }

    /// Map confidence level to replica strategy.
    pub fn replica_strategy(spec: &DalinTaskSpec) -> ReplicaStrategy {
        match (&spec.confidence, spec.replicas as usize) {
            (ConfidenceLevel::Verified, n) if n >= 2 => ReplicaStrategy::Quorum(
                spec.min_replicas.unwrap_or(1) as usize,
                spec.replicas as usize,
            ),
            (ConfidenceLevel::AutoRecover, _) => ReplicaStrategy::SelfHealing(
                spec.retry_attempts as usize,
                spec.retry_backoff_ms as u64,
            ),
            (ConfidenceLevel::High, _) => ReplicaStrategy::Minimum(spec.min_replicas.unwrap_or(1) as usize),
            _ => ReplicaStrategy::Fixed(spec.replicas as usize),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReplicaStrategy {
    Fixed(usize),
    Minimum(usize),
    Quorum(usize, usize),   // min, max
    SelfHealing(usize, u64), // retry_count, backoff_ms
}

// ─── Extension trait for ResourceRequirements ──────────────────

pub trait ResourceRequirementsExt {
    fn ensure_default_cpu(&mut self, val: &str);
    fn ensure_default_memory(&mut self, val: &str);
    fn add_gpu(&mut self, count: &str);
    fn set_cpu_limit(&mut self, val: &str);
}

impl ResourceRequirementsExt for ResourceRequirements {
    fn ensure_default_cpu(&mut self, val: &str) {
        if self.requests.as_mut().and_then(|r| r.cpu.is_none()).unwrap_or(true) {
            self.requests.get_or_insert_with(ResourceList::default)
                .cpu = Some(val.to_string());
        }
        if self.limits.as_mut().and_then(|l| l.cpu.is_none()).unwrap_or(true) {
            self.limits.get_or_insert_with(ResourceList::default)
                .cpu = Some(format!("{}x2", val.trim_end_matches('m')));
        }
    }

    fn ensure_default_memory(&mut self, val: &str) {
        if self.requests.as_mut().and_then(|r| r.memory.is_none()).unwrap_or(true) {
            self.requests.get_or_insert_with(ResourceList::default)
                .memory = Some(val.to_string());
        }
        if self.limits.as_mut().and_then(|l| l.memory.is_none()).unwrap_or(true) {
            self.limits.get_or_insert_with(ResourceList::default)
                .memory = Some(format!("{}x2", val));
        }
    }

    fn add_gpu(&mut self, count: &str) {
        self.requests.get_or_insert_with(ResourceList::default)
            .gpu = Some(count.to_string());
        self.limits.get_or_insert_with(ResourceList::default)
            .gpu = Some(count.to_string());
    }

    fn set_cpu_limit(&mut self, val: &str) {
        if let Some(limits) = &mut self.limits {
            limits.cpu = Some(val.to_string());
        }
    }
}

// ─── Scheduler Controller ──────────────────────────────────────

/// The core reconciliation loop for DalinTask CRD resources.
pub struct SchedulerController {
    client: Client,
    compiler_client: Option<CompilerGateway>, // RPC to compile .dal → task manifest
}

#[derive(Clone)]
pub struct CompilerGateway {
    addr: String,
}

impl CompilerGateway {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }

    /// Compile a .dal source file to a DalinTask manifest via gRPC.
    /// Returns parsed 7-channel metadata + validated spec.
    async fn compile(&self, source_file: &str) -> Result<DalinTaskSpec, OperatorError> {
        // TODO: implement real gRPC call to dalib compile
        // For now, return placeholder — this is wired up when control-plane server starts
        warn!(%source_file, "Compiler gateway not yet connected; using placeholder spec");
        Ok(DalinTaskSpec {
            function_id: source_file.to_string(),
            effect: Effect::Io,
            capability: Capability::Cpu,
            governance: GovernanceLevel::Basic,
            confidence: ConfidenceLevel::Medium,
            replicas: 1,
            timeout_seconds: 300,
            retry_attempts: 3,
            retry_backoff_ms: 1000,
            ..Default::default()
        })
    }
}

impl SchedulerController {
    pub fn new(client: Client, compiler_addr: Option<String>) -> Self {
        Self {
            client,
            compiler_client: compiler_addr.map(CompilerGateway::new),
        }
    }

    /// Main reconciliation entry point.
    pub async fn reconcile(dt: crate::k8s::DalinTask, ctx: crate::k8s::ReconcileCtx) -> Result<(), OperatorError> {
        let name = dt.metadata.name.as_deref().unwrap_or("unknown");
        let spec = &dt.spec;

        info!(name, %spec.effect, %spec.capability, "Reconciling DalinTask");

        // Step 1: Phase = Compiling (if source provided)
        if spec.source_file.is_some() || spec.source_git_repo.is_some() {
            Self::transition_phase(&ctx.client, name, &dt.metadata.namespace, TaskPhase::Compiling).await?;

            if let Some(ref src_file) = spec.source_file {
                let task_spec = if let Some(ref gw) = ctx.compiler_client {
                    gw.compile(src_file).await?
                } else {
                    // Fallback: use existing spec
                    spec.clone()
                };
                // Update spec with resolved compilation results
                // ... this modifies dt.metadata.annotations with compiled artifact ref
            }
        }

        // Step 2: Resolve resource requirements
        let resolved_resources = ResourceResolver::resolve(spec)?;
        let node_sel = ResourceResolver::node_selector(spec);
        let replica_strat = ResourceResolver::replica_strategy(spec);

        debug!(%name, ?resolved_resources, ?node_sel, ?replica_strat, "Resolved resources");

        // Step 3: Create/update Deployment
        Self::create_or_update_deployment(
            &ctx.client,
            name,
            &dt.metadata.namespace,
            spec,
            &resolved_resources,
            &node_sel,
            &replica_strat,
        ).await?;

        // Step 4: Phase = Scheduled → Running
        Self::transition_phase(&ctx.client, name, &dt.metadata.namespace, TaskPhase::Scheduled).await?;

        // Step 5: Monitor confidence / auto-recover
        Self::monitor_task_health(&ctx.client, name, &dt.metadata.namespace, spec).await;

        Ok(())
    }

    async fn transition_phase(
        client: &Client,
        name: &str,
        ns: &Option<String>,
        phase: TaskPhase,
    ) -> Result<(), OperatorError> {
        let patch = serde_json::json!({
            "apiVersion": "dalin.ai/v1alpha1",
            "kind": "DalinTask",
            "metadata": { "name": name },
            "status": {
                "phase": format!("{:?}", phase).to_lowercase().replace("_", "-"),
                "conditions": [{
                    "type": "PhaseChange",
                    "status": "True",
                    "lastTransitionTime": chrono::Utc::now().to_rfc3339(),
                    "message": format!("Transitioned to {:?}", phase)
                }]
            }
        });

        let api: Api<crate::k8s::DalinTask> = Api::namespaced(client.clone(), ns.as_deref().unwrap_or("default"));
        let _ = api.patch_status(
            name,
            &kube::api::Patch::Json(patch),
        ).await?;

        Ok(())
    }

    async fn create_or_update_deployment(
        client: &Client,
        task_name: &str,
        ns: &Option<String>,
        spec: &DalinTaskSpec,
        resources: &ResourceRequirements,
        node_sel: &std::collections::HashMap<String, String>,
        replica_strat: &ReplicaStrategy,
    ) -> Result<(), OperatorError> {
        let deploy_name = format!("dalin-task-{}", task_name);
        let namespace = ns.as_deref().unwrap_or("default");

        let replicas = match replica_strat {
            ReplicaStrategy::Fixed(n) => *n as i32,
            ReplicaStrategy::Minimum(min) => *min as i32,
            ReplicaStrategy::Quorum(min, _) => *min as i32,
            ReplicaStrategy::SelfHealing(_, _) => spec.replicas as i32,
        };

        // Build container spec
        let mut container_props = serde_json::Map::new();

        // resources
        let mut res_map = serde_json::Map::new();
        if let Some(ref req) = resources.requests {
            let mut req_map = serde_json::Map::new();
            if let Some(ref c) = req.cpu { req_map.insert("cpu".into(), json!(c)); }
            if let Some(ref m) = req.memory { req_map.insert("memory".into(), json!(m)); }
            if let Some(ref g) = req.gpu { req_map.insert("nvidia.com/gpu".into(), json!(g)); }
            res_map.insert("requests".into(), serde_json::Value::Object(req_map));
        }
        if let Some(ref lim) = resources.limits {
            let mut lim_map = serde_json::Map::new();
            if let Some(ref c) = lim.cpu { lim_map.insert("cpu".into(), json!(c)); }
            if let Some(ref m) = lim.memory { lim_map.insert("memory".into(), json!(m)); }
            if let Some(ref g) = lim.gpu { lim_map.insert("nvidia.com/gpu".into(), json!(g)); }
            res_map.insert("limits".into(), serde_json::Value::Object(lim_map));
        }
        container_props.insert("resources".into(), serde_json::Value::Object(res_map));

        // node selector
        let pod_anti_affinity = serde_json::json!({
            "preferredDuringSchedulingIgnoredDuringExecution": [{
                "weight": 100,
                "podAffinityTerm": {
                    "labelSelector": {
                        "matchExpressions": [{
                            "key": "app.dalin.ai/task",
                            "operator": "In",
                            "values": [task_name]
                        }]
                    },
                    "topologyKey": "kubernetes.io/hostname"
                }
            }]
        });

        let mut template_meta = serde_json::Map::new();
        let mut template_labels = serde_json::Map::new();
        template_labels.insert("app.dalin.ai/task".into(), json!(task_name));
        template_labels.insert("app.dalin.ai/effect".into(), json!(format!("{:?}", spec.effect).to_lowercase()));
        template_labels.insert("app.dalin.ai/capability".into(), json!(format!("{:?}", spec.capability).to_lowercase()));
        template_meta.insert("labels".into(), Value::Object(template_labels));

        let mut spec_template = serde_json::Map::new();
        spec_template.insert("template".into(), serde_json::json!({
            "metadata": Value::Object(template_meta),
            "spec": {
                "containers": [{
                    "name": task_name,
                    "image": "ghcr.io/cn-qn1-dalin/dalin-l-runtime:latest",
                    "imagePullPolicy": "IfNotPresent",
                    "command": ["/usr/local/bin/dalib", "cpd", "--mode", "runner"],
                    "env": vec![
                        serde_json::json!({"name": "DALIN_TASK_ID", "value": task_name}),
                        serde_json::json!({"name": "DALIN_LOG_LEVEL", "value": "info"}),
                    ],
                }],
                "nodeSelector": if node_sel.is_empty() {
                    serde_json::Value::Null
                } else {
                    let mut ns_map = serde_json::Map::new();
                    for (k, v) in node_sel {
                        ns_map.insert(k.clone(), json!(v));
                    }
                    Value::Object(ns_map)
                },
                "affinity": serde_json::json!({
                    "podAntiAffinity": pod_anti_affinity
                }),
            }
        }));

        let mut deploy_spec = serde_json::Map::new();
        deploy_spec.insert("replicas".into(), json!(replicas));
        deploy_spec.insert("selector".into(), serde_json::json!({
            "matchLabels": { "app.dalin.ai/task": task_name }
        }));
        deploy_spec.extend(spec_template);

        let deploy = serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {
                "name": deploy_name,
                "namespace": namespace,
                "labels": {
                    "app.kubernetes.io/part-of": "dalin-l",
                    "app.dalin.ai/task": task_name,
                }
            },
            "spec": serde_json::Value::Object(deploy_spec)
        });

        let api: Api<serde_json::Value> = Api::namespaced(client.clone(), namespace);
        let params = kube::api::UpsertParams {
            field_manager: Some("dalin-operator".to_string()),
            force: true,
        };

        let _ = api
            .patch(
                &task_name,
                &kube::api::Patch::Apply(deploy),
                &params,
            )
            .await?;

        info!(%deploy_name, ?replicas, "Deployment created/updated");
        Ok(())
    }

    async fn monitor_task_health(
        client: &Client,
        name: &str,
        ns: &Option<String>,
        spec: &DalinTaskSpec,
    ) {
        // TODO: implement health check loop with self-healing runtime integration
        // This should connect to the QN1 effect monitor to track execution status
        debug!(%name, "Health monitoring scheduled");
    }
}

// Re-export for convenience
pub use crate::k8s::DalinTask;
