# Phase J：自进化闭环 (Self-Evolution Closed Loop)

## 愿景

让 Dalin L 编译器能够根据运行时的错误模式和用户行为，自动改进自身代码和策略。

Dalin L 3.0 已经实现了从词法分析到七通道类型系统、TaskSpec 生成、LLM 扩展、延迟验证、模块/包系统的完整编译管线。**Phase J 是最后一个阶段**，它让编译器不再是一个"被动翻译工具"，而是一个能够自我诊断、自我修复、自我优化的**智能系统**。

## 设计原则

1. **人类在环（Human-in-the-loop）**：所有进化变更必须经过人工审批，零未审批的自动提交
2. **可逆性（Reversibility）**：每次进化都有完整的回滚路径
3. **可验证（Verifiable）**：进化前后必须有量化指标对比
4. **渐进式（Incremental）**：小步快跑，避免大规模重写

---

## J1: 模式学习引擎 (Pattern Learning Engine)

### 功能目标

收集运行时错误模式 → 聚类相似错误 → 提取通用修复策略 → 存储为"修复模板"到知识库

### 错误数据源

| 来源 | 数据类型 | 捕获方式 |
|------|----------|----------|
| `panic!` trace | 堆栈 + 错误消息 | Rust `Backtrace::capture()` |
| `ChannelError` | 结构化七通道违规 | 编译期 `CompileResult.errors` |
| `LatencyViolation` | 时序约束违反 | `latency.rs` 验证结果 |
| Governance reject | 治理策略拒绝 | 控制面 `GovernanceInspector` |
| Recovery events | 运行时恢复操作 | DLVM 运行时挂钩 |
| User feedback | 用户修正反馈 | CLI 命令 `dalan fix` |

### 错误聚类算法

```rust
// 语义哈希：将错误描述映射为固定长度向量
fn semantic_hash(error_msg: &str) -> [u8; 16] {
    // 基于关键词提取 + TF-IDF 向量化
    let tokens = extract_keywords(error_msg);
    hash_vector(tokens_to_tfidf(tokens))
}

// 聚类：DBSCAN 聚类相似错误
fn cluster_errors(errors: &[ErrorCode]) -> Vec<Vec<&ErrorCode>> {
    let mut clustering = Dbscan::new(epsilon, min_points);
    for err in errors {
        let vec = semantic_hash(&err.message);
        clustering.add_point(vec, err);
    }
    clustering.cluster()
}
```

### 修复模板结构

```json
{
    "template_id": "fix_latency_violation_001",
    "error_pattern": "latency constraint exceeded by >50%",
    "root_causes": ["missing async annotation", "inefficient algorithm"],
    "fix_strategy": [
        "添加 @async 注解",
        "将同步调用替换为并发执行",
        "调整 timeout 阈值 2x"
    ],
    "confidence": 0.87,
    "tested": true,
    "regression_count": 23,
    "last_used": "2026-07-15T10:30:00Z"
}
```

### 知识库存储

- **本地格式**: JSON Lines 文件 (`~/.dalín/evolution_kb.jsonl`)
- **可选后端**: SQLite（当条目数 > 10000 时自动切换）
- **备份策略**: git-based diff，每次写入前自动 commit

---

## J2: 策略自动生成 (Strategy Auto-Generation)

### 核心机制

#### 2.1 Recovery Mode 生成器

从成功修复的案例中归纳出新 recovery mode：

```rust
// 给定一组成功修复记录，归纳 recovery rule
fn infer_recovery_rule(successful_fixes: &[FixRecord]) -> Option<RecoveryRule> {
    // 1. 提取共性模式（交集）
    let common_patterns = intersection(fixes.iter().map(|f| &f.applied_rules));
    
    // 2. 泛化规则（泛化率由泛化引擎决定）
    let generalized = generalize(common_patterns);
    
    // 3. 安全检查
    if validate_safety(&generalized) && passes_governance_check(&generalized) {
        Some(generalized)
    } else {
        None
    }
}
```

#### 2.2 ConfidenceCalibrator 权重动态更新

```rust
// 根据历史准确性自动调整置信度权重
fn update_calibrator_weights(historical_accuracy: &WeightedAccuracy) {
    for channel in [Value, Effect, Capability, Governance, Latency, Confidence, QN] {
        // gradient descent 更新权重
        let gradient = derive_weight_gradient(channel, historical_accuracy);
        weights[channel] -= LEARNING_RATE * gradient;
        
        // 边界约束：权重 ∈ [0.05, 0.5]
        weights[channel] = weights[channel].clamp(0.05, 0.5);
    }
}
```

#### 2.3 Hot Recompile（热重编译）

定期重新编译运行时二进制：

```bash
# 触发条件：知识库中有 N+ 条新修复模板，且均通过回归测试
dalan evolve hot-recompile --threshold=5

# 流程：
# 1. 编译所有进化后的模块
# 2. 生成 delta patch
# 3. DLVM 检测到新版本，触发优雅重启
# 4. 旧进程 finish 当前任务后退出，新进程接管
```

---

## J3: 进化验证框架 (Evolution Verification Framework)

### 验证流水线

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  AB 实验分组  │  →  │  回归测试套件  │  →  │  性能对比     │
│             │     │              │     │             │
│ A组: 旧策略  │     │ 通过率 ≥ 95%? │     │ 性能提升 ≥10%?│
│ B组: 新策略  │     │ 性能开销 < 5%? │     │ 内存 < 5%?  │
└─────────────┘     └──────────────┘     └─────────────┘
                            │                      │
                      ┌─────▼──────┐     ┌────────▼───────┐
                      │  综合评分     │     │   PASS / FAIL   │
                      │ score ≥ 0.8? │     │  如果 PASS:     │
                      └──────────────┘     │  提交新配置      │
                                           └────────────────┘
```

### 回归测试套件

每个进化变更必须通过三层测试：

| 层级 | 测试内容 | 通过率要求 |
|------|----------|-----------|
| Unit | 编译器内部函数正确性 | 100% |
| Integration | TaskSpec 生成 + 类型推断一致性 | 100% |
| E2E | 真实项目编译 + 运行时行为 | ≥95% |

### 评分函数

```rust
struct EvolutionScore {
    regression_pass_rate: f64,    // 回归测试通过率 (0.0 - 1.0)
    performance_delta: f64,       // 性能变化 (-0.2 ~ +0.5)
    memory_delta: f64,            // 内存变化 (-0.1 ~ +0.1)
    coverage_impact: f64,         // 测试覆盖率变化
    governance_compliance: bool,  // 是否通过治理检查
}

impl EvolutionScore {
    fn composite(&self) -> f64 {
        let w = [0.4, 0.3, 0.1, 0.1, 0.1]; // 权重
        self.regression_pass_rate * w[0]
        + (1.0 + self.performance_delta).ln() * w[1]
        + (1.0 - abs(self.memory_delta)) * w[2]
        + self.coverage_impact * w[3]
        + if self.governance_compliance { 1.0 } else { 0.0 } * w[4]
    }
    
    fn passes_threshold(&self, threshold: f64) -> bool {
        self.composite() >= threshold
    }
}
```

---

## J4: 人类审查接口 (Human Review Interface)

### CLI 命令

```bash
# 查看待审批的进化建议
$ dalan evolve review

╔══════════════════════════════════════╗
║  进化建议 #42                        ║
╠══════════════════════════════════════╣
║  模块: latency.rs                    ║
║  变更: 延迟阈值计算方法                ║
║  影响范围: 3个 TaskSpec 生成逻辑       ║
║  预期收益: 延迟违规减少 15%           ║
║  风险等级: LOW                       ║
║                                       ║
║  [A]ccept  [R]eject  [D]iff  [H]elp  ║
╚══════════════════════════════════════╝

# 查看详细差异
$ dalan evolve diff --id=42

diff --git a/compiler/src/latency.rs b/compiler/src/latency.rs
--- a/compiler/src/latency.rs
+++ b/compiler/src/latency.rs
@@ -42,7 +42,10 @@
-    deadline.saturating_sub(now)
+    // 增加缓冲时间，避免边界情况误报
+    let buffer = if is_async { 10ms } else { 5ms };
+    deadline.saturating_sub(now).saturating_sub(buffer)

# 审批通过
$ dalan evolve accept --id=42

# 一键回滚
$ dalan revert --to=41
Applied: reverted to configuration at epoch 41
Rolled back 1 evolution change(s)
```

### 审批决策矩阵

| 风险等级 | 回归通过率要求 | 性能要求 | 审批人 |
|---------|-------------|---------|-------|
| TRIVIAL | ≥90% | 无限制 | 1人 |
| LOW | ≥95% | 性能提升 ≥5% | 1人 |
| MEDIUM | ≥98% | 性能提升 ≥10% | 2人 |
| HIGH | ≥99% | 性能提升 ≥15% | 3人 + 治理委员会 |
| CRITICAL | 100% | 不得退化 | 全委员会投票 |

### Revert 机制

```rust
/// 使用 git-based atomic swap 实现回滚
struct RevertOperation {
    target_epoch: u64,
    snapshot_before: PathBuf,
    operations: Vec<RollbackAction>,
}

enum RollbackAction {
    RestoreConfig(PathBuf, GitCommitId),    // 恢复配置文件到指定 commit
    RemoveTemplate(String),                  // 移除某条修复模板
    RevertModule(PathBuf, GitCommitId),      // 恢复某个模块到旧版本
    ManualReview(String),                    // 需要人工介入的特殊情况
}
```

---

## 技术方案

### 1. 知识库 (Knowledge Base)

```
~/.dalín/
├── evolution_kb.jsonl      # 进化日志（追加不可变）
├── templates/              # 修复模板目录
│   ├── fix_latency_*.json
│   ├── fix_type_error_*.json
│   └── fix_gov_reject_*.json
├── experiments/            # AB 实验结果
│   └── run_001/
│       ├── config.toml
│       ├── results.json
│       └── scores.json
└── audit_log/              # 审计日志
    └── 2026-07/
        └── 15.jsonl
```

### 2. 模式匹配与语义哈希

```rust
/// 基于语义哈希的错误聚类
struct ErrorClusteringEngine {
    index: InvertedIndex,          // 倒排索引：keyword → error_ids
    embeddings: HashMap<u128, Vec<f32>>,  // 语义向量缓存
    cluster_cache: RwLock<HashMap<String, Cluster>>,
}

impl ErrorClusteringEngine {
    /// 计算错误描述的语义向量（使用内置的轻量级 embedding）
    fn embed(error: &ErrorCode) -> Vec<f32> {
        // 使用词频 - IDF + 关键短语提取
        let tfidf = compute_tfidf(&extract_phrases(&error.message));
        l2_normalize(tfidf)
    }
    
    /// DBSCAN 聚类
    fn cluster(&self, errors: &[ErrorCode], eps: f32, min_pts: 3) -> Vec<Cluster> {
        let mut clusters = Vec::new();
        let mut visited = HashSet::new();
        
        for err in errors {
            if !visited.contains(&err.id) {
                visited.insert(err.id.clone());
                let neighbors = self.range_query(&err, eps);
                if neighbors.len() >= min_pts {
                    let cluster = self.expand_cluster(err, neighbors, eps);
                    clusters.push(cluster);
                }
            }
        }
        clusters
    }
}
```

### 3. 安全护栏 (Safety Guardrails)

所有进化操作必须通过治理检查器：

```rust
/// 进化操作治理检查
struct EvolutionGovernor {
    policy: EvolutionPolicy,
    audit_log: AuditLogger,
}

impl EvolutionGovernor {
    /// 在每次进化前执行安全检查
    fn check_evolution(&self, change: &EvolutionChange) -> Result<(), GovernError> {
        // 1. 变更不能修改核心安全原语
        if change.affects_safety_primitives() {
            return Err(GovernError::CannotModifySafetyPrimitives);
        }
        
        // 2. 变更必须有对应的回归测试覆盖
        if !change.has_test_coverage() {
            return Err(GovernError::MissingTestCoverage);
        }
        
        // 3. 变更不能降低系统安全基线
        if change.degrades_safety_baseline() {
            return Err(GovernError::SafetyBaselineDegraded);
        }
        
        // 4. 审批链检查
        required_approvals(change.risk_level)
            .check_all(change.approvals)?;
        
        Ok(())
    }
}
```

### 4. 回滚机制 (Rollback Mechanism)

```rust
/// Atomic swap-based rollback
struct RollbackManager {
    git_repo: GitRepository,
    snapshot_dir: PathBuf,
    active_epoch: RwLock<u64>,
}

impl RollbackManager {
    /// 创建当前状态的快照
    fn take_snapshot(&self) -> Result<Snapshot, Error> {
        let snapshot = self.git_repo.create_branch(format!(
            "evolution/snapshot-{epoch}",
            epoch = *self.active_epoch.read()
        ))?;
        Ok(snapshot)
    }
    
    /// 原子切换到目标 epoch 的配置
    fn atomic_swap_to(&self, target_epoch: u64) -> Result<(), Error> {
        let current = *self.active_epoch.read();
        
        // 1. 保存当前状态
        self.take_snapshot()?;
        
        // 2. 从目标 epoch 恢复配置
        let config_files = self.git_repo.checkout_files(target_epoch)?;
        
        // 3. 原子替换
        atomic_replace(&config_files)?;
        
        // 4. 验证新配置
        validate_configuration(&config_files)?;
        
        // 5. 更新 epoch 指针
        *self.active_epoch.write() = target_epoch;
        
        Ok(())
    }
}
```

---

## 与现有系统的集成

### 编译管线集成点

```
Source → Lexer → Parser → LLM Expand → Ty2 Infer → Latency Verify
                                                          ↑
                                                   [J1] 错误收集和聚类
                                                          ↓
                                                    TaskSpec
                                                          ↓
                                                 Control Plane
                                                          ↓
                                          [J2/J3] 运行时反馈循环
                                           错误模式 → 知识库
                                              ↓
                                          [J4] 人类审批
                                              ↓
                                          更新配置 → hot-recompile
```

### 与 Phase H（模块/包系统）协作

- 标准库加载器 (`stdlib_loader.rs`) 加载的 .dal 模块也是进化目标的载体
- 进化策略按模块粒度应用：不同模块可以有独立进化的配置
- `dalin.toml` 中的 `evolution` 段落定义模块级别的进化策略

### 与 LLM Engine 协作

- LLM 作为"高级推理层"参与修复模板的生成和评审
- `@llm("分析这类错误并推荐修复策略")` 指令触发 LLM 辅助聚类分析
- LLM 生成的模板必须通过 J3 验证框架才能进入生产

---

## 验收标准

### 必须达标项

- [ ] 能自动从 100+ 次运行时错误中提取至少 5 种通用修复模式
- [ ] 进化前后的编译器在相同负载下性能提升 > 10%
- [ ] 所有进化变更可通过 `dalan revert` 一键回滚
- [ ] 零未审批的自动提交（human-in-the-loop 强制执行）

### 建议达标项

- [ ] 知识库中能存储至少 50 条有效修复模板
- [ ] 支持模块级别独立进化（不同模块有不同进化节奏）
- [ ] 进化建议的平均审批时间 < 24 小时
- [ ] 与 LLM Engine 集成的自动模板生成准确率达到 80%

---

## 未来演进方向

### Phase K（预留）：跨 Agent 协同进化

- 多个 Dalin L Agent 共享进化经验
- 联邦学习风格的分布式知识库
- 社区进化生态：`dalan share-template`, `dalan vote-evolution`

### Phase L（预留）：自主进化

- 在严格 sandbox 内的有限自主进化权限
- AI-native 项目的零人工干预自动修复
- 持续部署场景下的 CI/CD 原生进化
