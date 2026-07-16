# ADR-018: K8s Operator — 容器化 + 生产级调度

## Status
Proposed

## Context
Dalin L 的控制面目前是一个 gRPC 进程（cpd），运行在单台机器上。要支持多节点分布式调度、云原生部署，需要：
1. 将 Dalin Agent/Task 抽象为 Kubernetes 自定义资源（CRD）
2. 实现 K8s Operator 自动管理 Agent 生命周期
3. 适配 HPA（Horizontal Pod Autoscaler）和 VPA（Vertical Pod Autoscaler）

## Decision
**三层架构：LSP → pkg → K8s Operator**，按优先级推进：

### 第一层：K8s CRD 定义（`/deploy/crd/`）

```yaml
# dalinagent.dalin-lang.org.yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: dalinagents.dalin-lang.org
spec:
  group: dalin-lang.org
  versions:
    - name: v1alpha1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                code:
                  type: string
                  description: DALin L source code
                channelAttributes:
                  type: object
                  description: 七通道类型属性
                replicas:
                  type: integer
                  minimum: 1
                maxReplicas:
                  type: integer
                resources:
                  type: object
                  properties:
                    requests:
                      type: object
                    limits:
                      type: object
                schedulePolicy:
                  type: string
                  enum: [greedy, fair, latency_sensitive]
status:
  conditions:
    - type: Running
      status: "True"
    - type: Healthy
      status: "True"
```

### 第二层：Operator Controller（`/operator/`）

```
operator/
├── Cargo.toml
├── src/
│   ├── lib.rs           # crate 入口
│   ├── main.rs          # binary entrypoint
│   ├── controller.rs    # 主控制器 (reconcile loop)
│   ├── reconciler.rs    # DalinAgent 的 create/update/delete
│   ├── scheduler.rs     # 轻量级调度器（基于 TaskSpec）
│   └── metrics.rs       # Prometheus metrics 导出
├── Dockerfile
└── deploy/
    ├── deployment.yaml
    ├── serviceaccount.yaml
    └── clusterrole.yaml
```

核心逻辑：
```rust
/// reconcile: watcher 收到事件 → fetch DalinAgent CR → 
/// 对比当前 pod 状态 → 驱动到期望状态
fn reconcile(&self, agent: &DalinAgent) -> Result<ReconcileResult> {
    // 1. 编译代码 → task_spec
    let task_spec = compile(agent.spec.code)?;
    
    // 2. 校验七通道约束 → 不通过则标记 Failed
    if !validate_channel_constraints(&task_spec) {
        return Ok(ReconcileResult::Fail);
    }
    
    // 3. 根据 scale policy 决定 replicas
    let desired_replicas = self.scale_policy.evaluate(&task_spec, agent.spec.replicas);
    
    // 4. 创建/删除 pod 匹配 desired_replicas
    sync_pods(&desired_replicas, &task_spec)?;
    
    // 5. 更新 CRD status
    update_status(&agent.name, desired_replicas)?;
    
    Ok(ReconcileResult::Success)
}
```

### 第三层：HPA/VPA + Metrics

```yaml
# hpa.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: dalin-agent-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: dalin-agent
  minReplicas: 1
  maxReplicas: 50
  metrics:
    - type: Pods
      pods:
        metric:
          name: dalin_agent_active_loops
        target:
          type: AverageValue
          averageValue: "5"
```

## Consequences

### 变得更轻松
- Agent 以 CRD 形式声明，`kubectl apply` 即部署
- HPA 自动扩缩容，应对突发流量
- Prometheus + Grafana 监控 Agent 指标
- 与现有 K8s 生态完全兼容

### 变难的事情
- **这是最大的杠杆点，也是最大的风险**。Dalin L 定位为 AI Agent 专用语言，K8s Operator 面向生产环境的必要性存疑——除非有明确的商业化目标
- Operator 编写成本约 3000-5000 行 Rust，是包管理器的 5 倍
- 需要维护 `wasmtime` / `tokio` 等依赖来编译 CRD 定义和 controller-gen
- 容器镜像构建时间增加 2-3x
