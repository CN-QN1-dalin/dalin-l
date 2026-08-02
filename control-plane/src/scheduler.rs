//! Capability Scheduler — 能力格放置算法（Phase 2 调度核心 + 可靠性增强）
//!
//! 放置规则（与深化设计文档对齐）：
//!   1. 节点能力格必须 ⊇ 任务能力（Cpu ≤ Gpu ≤ Sfa ≤ Net，链状偏序）。
//!   2. 满足条件的节点里，选负载最低者（负载均衡）。
//!   3. 无一满足 → 拒绝（控制面不降级到不足能力节点，保证最小权限）。
//!
//! 可靠性增强（深化设计文档「可靠性」章节）：
//!   - **配额 / 背压**：每节点有并发配额 `quota`，达到配额则跳过（背压，不无限堆积）。
//!   - **熔断**：每节点连续失败达阈值则打开熔断器，冷却后半开探活，成功则关闭。
//!   - 调度器内部用原子计数 + 短临界区维护运行时状态，`place/release/mark_*` 均为 `&self`。

use std::collections::HashSet;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Capability channel (aligned with the three-channel type system's capability)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Cpu = 0,
    Gpu = 1,
    Sfa = 2,
    Net = 3,
}

impl Capability {
    /// Capability lattice partial order: a ≤ b means a's capability is a subset of b's (b covers a).
    #[must_use]
    pub fn leq(&self, other: &Capability) -> bool {
        (*self as u8) <= (*other as u8)
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
            other => Err(format!("未知能力: {other}")),
        }
    }
}

/// A schedulable compute node (config surface; runtime state is maintained internally by the scheduler).
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    /// 节点显式拥有的能力；按链状格，拥有 Net 即隐式覆盖 Cpu..Net 全部。
    pub capabilities: HashSet<Capability>,
    /// 初始负载种子（调度器据此初始化运行时负载）。
    pub load: usize,
    /// 最大并发任务数；None 表示不限（谨慎：生产应设上限以触发背压）。
    pub quota: Option<usize>,
}

impl Node {
    pub fn new(id: impl Into<String>, capabilities: HashSet<Capability>) -> Self {
        Self {
            id: id.into(),
            capabilities,
            load: 0,
            quota: None,
        }
    }

    /// Builder-style setter for the quota (backpressure threshold).
    #[must_use]
    pub fn with_quota(mut self, quota: usize) -> Self {
        self.quota = Some(quota);
        self
    }
}

/// Placement result
#[derive(Debug, Clone)]
pub struct Placement {
    pub node_id: String,
    pub capability: Capability,
}

/// Scheduling rejection reason (observability / future mapping to gRPC status).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    /// 没有能力覆盖该任务的节点。
    NoCapableNode,
    /// 所有能覆盖的节点都达到配额（背压）。
    Overloaded,
    /// 熔断开启，拒绝探活。
    CircuitOpen,
}

/// 每节点熔断器：连续失败达阈值 → 打开；冷却后半开探活；成功 → 关闭。
struct CircuitBreaker {
    failures: AtomicUsize,
    threshold: usize,
    /// 0 = closed, 1 = open, 2 = half-open
    state: AtomicU8,
    opened_at: StdMutex<Option<Instant>>,
    cooldown: Duration,
}

impl CircuitBreaker {
    fn new(threshold: usize, cooldown: Duration) -> Self {
        Self {
            failures: AtomicUsize::new(0),
            threshold,
            state: AtomicU8::new(0),
            opened_at: StdMutex::new(None),
            cooldown,
        }
    }

    /// 是否允许向该节点派发（含半开探活决策）。
    fn allow(&self) -> bool {
        match self.state.load(Ordering::SeqCst) {
            0 => true, // closed
            2 => true, // half-open：允许一次探活
            _ => {
                // open：冷却到期则转 half-open
                let guard = self.opened_at.lock().unwrap();
                match *guard {
                    Some(t) if t.elapsed() >= self.cooldown => {
                        drop(guard);
                        self.state.store(2, Ordering::SeqCst);
                        true
                    }
                    _ => false,
                }
            }
        }
    }

    fn record_failure(&self) {
        let f = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if f >= self.threshold {
            self.state.store(1, Ordering::SeqCst); // open
            *self.opened_at.lock().unwrap() = Some(Instant::now());
        }
    }

    fn record_success(&self) {
        self.failures.store(0, Ordering::SeqCst);
        self.state.store(0, Ordering::SeqCst); // close
    }
}

/// 节点运行时状态（调度器内部持有，可被 `&self` 方法修改）。
struct NodeRuntime {
    id: String,
    capabilities: HashSet<Capability>,
    load: AtomicUsize,
    quota: Option<usize>,
    breaker: CircuitBreaker,
}

#[derive(Default)]
pub struct CapabilityScheduler {
    nodes: Vec<NodeRuntime>,
}

impl CapabilityScheduler {
    #[must_use]
    pub fn new(nodes: Vec<Node>) -> Self {
        let runtimes = nodes
            .into_iter()
            .map(|n| NodeRuntime {
                id: n.id,
                capabilities: n.capabilities,
                load: AtomicUsize::new(n.load),
                quota: n.quota,
                breaker: CircuitBreaker::new(3, Duration::from_secs(30)),
            })
            .collect();
        Self { nodes: runtimes }
    }

    /// Pick a node for a task: among nodes whose capability covers the task's (node ⊇ task capability), whose breaker is open, and that have not reached quota, choose the one with the lowest load.
    /// Not found → None (scheduling rejected: backpressure / no node / breaker).
    #[must_use]
    pub fn place(&self, required: &Capability) -> Option<Placement> {
        let mut best: Option<&NodeRuntime> = None;
        for n in &self.nodes {
            if !n.capabilities.iter().any(|c| required.leq(c)) {
                continue; // 能力不够
            }
            if !n.breaker.allow() {
                continue; // 熔断中
            }
            if let Some(q) = n.quota
                && n.load.load(Ordering::SeqCst) >= q
            {
                continue; // 配额耗尽（背压）
            }
            match best {
                None => best = Some(n),
                Some(b) => {
                    let bl = b.load.load(Ordering::SeqCst);
                    let nl = n.load.load(Ordering::SeqCst);
                    if nl < bl {
                        best = Some(n);
                    }
                }
            }
        }
        let node = best?;
        node.load.fetch_add(1, Ordering::SeqCst);
        Some(Placement {
            node_id: node.id.clone(),
            capability: *required,
        })
    }

    /// Release one concurrency slot on a node (called on task completion / cancellation / failure).
    pub fn release(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.load.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Report a node execution failure (drives the circuit breaker).
    pub fn mark_failure(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.breaker.record_failure();
        }
    }

    /// Report a node execution success (resets the circuit breaker).
    pub fn mark_success(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.breaker.record_success();
        }
    }

    /// Snapshot of current per-node load (observability / debugging).
    #[must_use]
    pub fn load_snapshot(&self) -> Vec<(String, usize)> {
        self.nodes
            .iter()
            .map(|n| (n.id.clone(), n.load.load(Ordering::SeqCst)))
            .collect()
    }

    /// Place directly from an annotation string (unknown capabilities fall back to Cpu).
    #[must_use]
    pub fn place_by_spec(&self, capability: &str) -> Option<Placement> {
        let cap: Capability = capability.parse().unwrap_or(Capability::Cpu);
        self.place(&cap)
    }

    /// Dynamically add a node (from the Agent Registry).
    pub fn add_node(&mut self, node: Node) {
        let rt = NodeRuntime {
            id: node.id,
            capabilities: node.capabilities,
            load: AtomicUsize::new(node.load),
            quota: node.quota,
            breaker: CircuitBreaker::new(3, Duration::from_secs(30)),
        };
        self.nodes.retain(|n| n.id != rt.id);
        self.nodes.push(rt);
    }

    /// Replace the node list entirely (from `NodeRegistry.fresh_nodes()`).
    /// Preserves existing nodes' runtime state (load / breaker).
    pub fn sync_nodes(&mut self, nodes: Vec<Node>) {
        self.nodes = nodes
            .into_iter()
            .map(|n| NodeRuntime {
                id: n.id.clone(),
                capabilities: n.capabilities,
                load: AtomicUsize::new(n.load),
                quota: n.quota,
                breaker: CircuitBreaker::new(3, Duration::from_secs(30)),
            })
            .collect();
    }

    /// Get the number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes() -> Vec<Node> {
        vec![
            Node::new("cpu-only", [Capability::Cpu].into_iter().collect()).with_quota(4),
            Node::new(
                "gpu-rich",
                [Capability::Cpu, Capability::Gpu, Capability::Sfa]
                    .into_iter()
                    .collect(),
            )
            .with_quota(8),
        ]
    }

    #[test]
    fn cpu_task_placed_on_cpu_node() {
        let s = CapabilityScheduler::new(nodes());
        let p = s.place(&Capability::Cpu).unwrap();
        assert!(p.node_id == "cpu-only" || p.node_id == "gpu-rich");
    }

    #[test]
    fn sfa_task_requires_capable_node() {
        let s = CapabilityScheduler::new(nodes());
        let p = s.place(&Capability::Sfa).unwrap();
        assert_eq!(p.node_id, "gpu-rich");
    }

    #[test]
    fn net_task_rejected_when_no_node_covers() {
        let s = CapabilityScheduler::new(nodes());
        assert!(s.place(&Capability::Net).is_none());
    }

    #[test]
    fn least_loaded_preferred() {
        let mut ns = nodes();
        ns[1].load = 10; // gpu-rich 重载
        let s = CapabilityScheduler::new(ns);
        let p = s.place(&Capability::Cpu).unwrap();
        assert_eq!(p.node_id, "cpu-only");
    }

    #[test]
    fn quota_exhaustion_triggers_backpressure() {
        // cpu-only 配额 1：放两次，第二次应背压拒绝
        let ns = vec![Node::new("n1", [Capability::Cpu].into_iter().collect()).with_quota(1)];
        let s = CapabilityScheduler::new(ns);
        assert!(s.place(&Capability::Cpu).is_some());
        assert!(s.place(&Capability::Cpu).is_none(), "超过配额应背压");
        // 释放后恢复容量
        s.release("n1");
        assert!(s.place(&Capability::Cpu).is_some());
    }

    #[test]
    fn circuit_breaker_opens_and_skips_node() {
        let ns = vec![Node::new(
            "gpu-rich",
            [Capability::Cpu, Capability::Gpu, Capability::Sfa]
                .into_iter()
                .collect(),
        )];
        let s = CapabilityScheduler::new(ns);
        // 连续 3 次失败 → 熔断打开
        s.mark_failure("gpu-rich");
        s.mark_failure("gpu-rich");
        s.mark_failure("gpu-rich");
        assert!(s.place(&Capability::Sfa).is_none(), "熔断开启应拒绝");
        // 其它节点仍可用
        let ns2 = vec![
            Node::new("a", [Capability::Cpu].into_iter().collect()),
            Node::new(
                "b",
                [Capability::Cpu, Capability::Gpu, Capability::Sfa]
                    .into_iter()
                    .collect(),
            ),
        ];
        let s2 = CapabilityScheduler::new(ns2);
        s2.mark_failure("b");
        s2.mark_failure("b");
        s2.mark_failure("b");
        // b 熔断，cpu 任务仍可落到 a
        let p = s2.place(&Capability::Cpu).unwrap();
        assert_eq!(p.node_id, "a");
        // b 成功上报 → 熔断复位
        s2.mark_success("b");
        let p2 = s2.place(&Capability::Sfa).unwrap();
        assert_eq!(p2.node_id, "b");
    }
}
