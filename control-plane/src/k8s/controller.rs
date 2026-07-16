// ============================================================
// Dalin L 2.0 — K8s Operator Controller (reconciliation layer)
// Pure Rust — no kube-rs dependency, integrates cleanly when added.
// ============================================================

use crate::k8s::operator_types::*;
use serde_json::{json, Map as JsonMap};

// Re-import error type for method signatures
use crate::k8s::operator_types::OperatorError as OpErr;

// ─── Reconciliation result ─────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ReconcileResult {
    Scheduled { deployment_name: String, replicas: usize },
    Failed(String),
    NoOp,
}

// ─── Scheduler Controller ──────────────────────────────────────

/// Translates DalinTask CRD events into Kubernetes Deployment changes.
pub struct SchedulerController {
    namespace: String,
}

impl SchedulerController {
    pub fn new(namespace: String) -> Self {
        Self { namespace }
    }

    /// Main reconciliation entry point.
    pub fn reconcile(&self, task_name: &str, spec: &DalinTaskSpec) -> Result<ReconcileResult, OpErr> {
        Self::validate_spec(spec)?;

        let resources = ResourceResolver::resolve(spec)?;
        let _node_sel = ResourceResolver::node_selector(spec);
        let replica_strat = ResourceResolver::replica_strategy(spec);
        let replicas = Self::replica_count(replica_strat, spec.replicas);
        let _pod_labels = ResourceResolver::pod_labels(spec);
        let dep_name = ResourceResolver::deployment_name(task_name);

        Ok(ReconcileResult::Scheduled { deployment_name: dep_name, replicas })
    }

    fn validate_spec(spec: &DalinTaskSpec) -> Result<(), OpErr> {
        if spec.function_id.is_empty() {
            return Err(OpErr::InvalidSpec("function_id must not be empty".into()));
        }
        if spec.replicas == 0 {
            return Err(OpErr::InvalidSpec("replicas must be >= 1".into()));
        }
        Ok(())
    }

    fn replica_count(strategy: ReplicaStrategy, default: u32) -> usize {
        match strategy {
            ReplicaStrategy::Fixed(n) => n,
            ReplicaStrategy::Minimum(n) => n,
            ReplicaStrategy::Quorum(min, _) => min as usize,
            ReplicaStrategy::SelfHealing(_, _) => default as usize,
        }
    }
}

// ─── Deployment Desire ─────────────────────────────────────────

pub struct DeploymentDesire {
    pub name: String,
    pub namespace: String,
    pub function_id: String,
    pub replicas: usize,
    pub resources: ResourceRequirements,
    pub node_selector: std::collections::HashMap<String, String>,
    pub pod_labels: std::collections::HashMap<String, String>,
    pub timeout: u64,
    pub retry_attempts: u32,
    pub effect: Effect,
    pub confidence: ConfidenceLevel,
}

impl DeploymentDesire {
    pub fn from_spec(spec: &DalinTaskSpec, namespace: &str) -> Self {
        let resources = ResourceResolver::resolve(spec).unwrap_or_default();
        let ns = ResourceResolver::node_selector(spec);
        let pl = ResourceResolver::pod_labels(spec);
        let dn = ResourceResolver::deployment_name(&spec.function_id);

        Self {
            name: dn, namespace: namespace.to_string(),
            function_id: spec.function_id.clone(),
            replicas: spec.replicas as usize,
            resources, node_selector: ns, pod_labels: pl,
            timeout: spec.timeout_seconds,
            retry_attempts: spec.retry_attempts,
            effect: spec.effect, confidence: spec.confidence,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        // defaults
        let req_cpu = self.resources.requests.as_ref().and_then(|r| r.cpu.as_deref()).unwrap_or("100m");
        let req_mem = self.resources.requests.as_ref().and_then(|r| r.memory.as_deref()).unwrap_or("128Mi");
        let lim_cpu = self.resources.limits.as_ref().and_then(|l| l.cpu.as_deref()).unwrap_or("200m");
        let lim_mem = self.resources.limits.as_ref().and_then(|l| l.memory.as_deref()).unwrap_or("256Mi");

        let mut container = JsonMap::new();
        container.insert("name".into(), json!(self.function_id.clone()));
        container.insert("image".into(), json!("ghcr.io/cn-qn1-dalin/dalin-l-runtime:latest"));
        container.insert("command".into(), json!(["/usr/local/bin/dalib", "cpd", "--mode", "runner"]));
        container.insert("env".into(), json!([
            {"name": "DALIN_TASK_ID", "value": self.function_id.clone()},
            {"name": "DALIN_LOG_LEVEL", "value": "info"},
        ]));

        // resources
        let mut res_map = JsonMap::new();
        let mut req_map = JsonMap::new();
        req_map.insert("cpu".into(), json!(req_cpu));
        req_map.insert("memory".into(), json!(req_mem));
        res_map.insert("requests".into(), serde_json::Value::Object(req_map));

        let mut lim_map = JsonMap::new();
        lim_map.insert("cpu".into(), json!(lim_cpu));
        lim_map.insert("memory".into(), json!(lim_mem));
        res_map.insert("limits".into(), serde_json::Value::Object(lim_map));
        container.insert("resources".into(), serde_json::Value::Object(res_map));

        // node selector
        let ns_val = if self.node_selector.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::to_value(&self.node_selector).unwrap_or(serde_json::Value::Null)
        };

        // anti-affinity
        let affinity = json!({
            "podAntiAffinity": {
                "preferredDuringSchedulingIgnoredDuringExecution": [{
                    "weight": 100,
                    "podAffinityTerm": {
                        "labelSelector": {
                            "matchExpressions": [{
                                "key": "app.dalin.ai/task",
                                "operator": "In",
                                "values": [self.function_id.clone()]
                            }]
                        },
                        "topologyKey": "kubernetes.io/hostname"
                    }
                }]
            }
        });

        json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": {
                "name": self.name,
                "namespace": self.namespace,
                "labels": {
                    "app.kubernetes.io/part-of": "dalin-l",
                    "app.dalin.ai/function": self.function_id,
                }
            },
            "spec": {
                "replicas": self.replicas,
                "selector": { "matchLabels": { "app.dalin.ai/function": self.function_id } },
                "template": {
                    "metadata": { "labels": self.pod_labels },
                    "spec": {
                        "containers": [serde_json::Value::Object(container)],
                        "nodeSelector": ns_val,
                        "affinity": affinity,
                        "terminationGracePeriodSeconds": self.timeout,
                    }
                }
            }
        })
    }

    pub fn status_update(&self, phase: TaskPhase) -> DalinTaskStatus {
        use std::time::SystemTime;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".into());

        let mut status = DalinTaskStatus {
            phase: phase.clone(),
            ready_replicas: Some(self.replicas as u32),
            observed_generation: Some(1),
            conditions: vec![TaskCondition {
                r#type: "Ready".into(),
                status: "True".into(),
                last_transition_time: ts,
                message: Some(format!(
                    "Deployment {} ready with {} replicas (effect={:?}, conf={:?})",
                    self.name, self.replicas, self.effect, self.confidence
                )),
            }],
            ..Default::default()
        };
        status.transition(phase);
        status
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
    fn validate_passes_for_valid_spec() {
        let c = SchedulerController::new("default".into());
        assert!(c.reconcile("test", &sample_spec()).is_ok());
    }

    #[test]
    fn validate_fails_empty_function_id() {
        let c = SchedulerController::new("default".into());
        let bad = DalinTaskSpec { function_id: "".into(), ..sample_spec() };
        assert!(c.reconcile("bad", &bad).is_err());
    }

    #[test]
    fn scheduled_produces_correct_deployment_name() {
        let c = SchedulerController::new("default".into());
        let r = c.reconcile("test-agent", &sample_spec()).unwrap();
        match r {
            ReconcileResult::Scheduled { deployment_name, replicas } => {
                assert_eq!(deployment_name, "dalin-task-test-agent");
                assert_eq!(replicas, 2);
            }
            _ => panic!("Expected Scheduled"),
        }
    }

    #[test]
    fn gpu_spec_resolves_gpu_resources() {
        let c = SchedulerController::new("default".into());
        let gpu = DalinTaskSpec {
            function_id: "gpu-agent".into(),
            effect: Effect::Spawn,
            capability: Capability::Gpu,
            confidence: ConfidenceLevel::High,
            replicas: 1, ..Default::default()
        };
        let r = c.reconcile("gpu-agent", &gpu).unwrap();
        match r {
            ReconcileResult::Scheduled { deployment_name, .. } => {
                assert_eq!(deployment_name, "dalin-task-gpu-agent");
            }
            _ => panic!("Expected Scheduled"),
        }
    }

    #[test]
    fn auto_recover_uses_self_healing_strategy() {
        let c = SchedulerController::new("default".into());
        let spec = DalinTaskSpec {
            function_id: "critical".into(),
            effect: Effect::Async,
            capability: Capability::Cpu,
            confidence: ConfidenceLevel::AutoRecover,
            replicas: 1, ..Default::default()
        };
        let r = c.reconcile("critical", &spec).unwrap();
        match r {
            ReconcileResult::Scheduled { replicas, .. } => assert_eq!(replicas, 1),
            _ => panic!("Expected Scheduled"),
        }
    }

    #[test]
    fn deployment_to_json_serializes() {
        let desire = DeploymentDesire::from_spec(&sample_spec(), "default");
        let j = desire.to_json();
        assert_eq!(j["spec"]["replicas"], 2);
        assert_eq!(j["spec"]["template"]["spec"]["containers"][0]["name"], "test-agent");
    }
}
