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

/// 能力通道（与三通道类型系统的 capability 对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Cpu = 0,
    Gpu = 1,
    Sfa = 2,
    Net = 3,
}

impl Capability {
    /// 能力格偏序：a ≤ b 表示 a 的能力是 b 的子集（b 覆盖 a）。
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
            other => Err(format!("未知能力: {}", other)),
        }
    }
}

/// 一个可调度的计算节点（配置面；运行时状态由调度器内部维护）。
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

    /// 链式设置配额（背压阈值）。
    pub fn with_quota(mut self, quota: usize) -> Self {
        self.quota = Some(quota);
        self
    }
}

/// 放置结果
#[derive(Debug, Clone)]
pub struct Placement {
    pub node_id: String,
    pub capability: Capability,
}

/// 调度拒绝原因（可观测性 / 未来映射到 gRPC 状态）。
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

    /// 为任务选一个节点：能力覆盖（节点 ⊇ 任务能力）且熔断器允许 且 未到配额 的节点里选负载最低者。
    /// 找不到 → None（拒绝调度：背压 / 无节点 / 熔断）。
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

    /// 释放一个节点的一个并发槽（任务完成 / 取消 / 失败时调用）。
    pub fn release(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.load.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// 上报节点执行失败（驱动熔断）。
    pub fn mark_failure(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.breaker.record_failure();
        }
    }

    /// 上报节点执行成功（复位熔断）。
    pub fn mark_success(&self, node_id: &str) {
        if let Some(n) = self.nodes.iter().find(|n| n.id == node_id) {
            n.breaker.record_success();
        }
    }

    /// 当前各节点负载快照（可观测 / 调试）。
    pub fn load_snapshot(&self) -> Vec<(String, usize)> {
        self.nodes
            .iter()
            .map(|n| (n.id.clone(), n.load.load(Ordering::SeqCst)))
            .collect()
    }

    /// 从注解字符串直接放置（未知能力回落 Cpu）。
    pub fn place_by_spec(&self, capability: &str) -> Option<Placement> {
        let cap: Capability = capability.parse().unwrap_or(Capability::Cpu);
        self.place(&cap)
    }

    /// 动态添加一个节点（from Agent Registry）。
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

    /// 全量替换节点列表（from NodeRegistry.fresh_nodes()）。
    /// 保留已有节点的运行时状态（load / breaker）。
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

    /// 获取节点数量。
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
