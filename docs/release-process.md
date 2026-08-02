# Dalin L 3.0 发布流程（Release Process）

> 建立时间：2026-08-02 | 适用：商业化发布（本地私有仓，遵守发布纪律：不 push 公网）

---

## 一、版本策略（SemVer）

### 版本号约定
- 主版本 `3.x.y`：语言/工具链核心能力（当前 `3.0.0-dev`）
- 预发布后缀：`-dev`（开发中）→ `-rc.N`（发布候选）→ 正式 `3.0.0`
- **单一版本真相源**：所有 crate 版本统一在根 `Cargo.toml` 的 `[workspace.package] version` 声明，crate 内用 `version.workspace = true` 引用

### 版本决策规则
| 变更类型 | 版本变化 |
|----------|----------|
| 破坏性 API 变更 / 语言语义变更 | 主版本 +1（3.0.0 → 4.0.0） |
| 向后兼容的新功能 | 次版本 +1（3.0.0 → 3.1.0） |
| Bug 修复 / 内部重构 | 补丁版本 +1（3.0.0 → 3.0.1） |

---

## 二、发布前检查清单（Release Checklist）

### 质量门禁（必须全绿）
```bash
# 1. 全量编译（0 errors）
cargo check --workspace --exclude dalin-pyo3

# 2. 全量测试（0 failed）
cargo test --workspace --exclude dalin-pyo3

# 3. Clippy 零警告（含所有 targets）
cargo clippy --workspace --exclude dalin-pyo3 --all-targets

# 4. 格式检查
cargo fmt --all -- --check

# 5. stdlib 解析守护（68/68 零错误）
cargo test -p dalin-compiler --test stdlib_parse_check

# 6. 冒烟测试（30 tests 值断言）
cargo test -p dalin-runtime --test smoke_test
```

### 文档门禁
- [ ] CHANGELOG.md 已更新（新版本条目 + 变更分类）
- [ ] docs/language-spec.md 与语言行为一致
- [ ] docs/api-reference.md 覆盖公开 API
- [ ] 已知限制已如实记录

### 仓库卫生
- [ ] Cargo.lock 已提交（可复现构建）
- [ ] 无未跟踪的调试文件（examples/ 手工脚本已清理）
- [ ] git status 干净（除预期变更）

---

## 三、发布步骤

### Step 1: 冻结功能
- 确认当前 commit 为发布候选基线
- 记录基线 commit hash

### Step 2: 版本号提升
```bash
# 根 Cargo.toml [workspace.package]
# version = "3.0.0-dev" → "3.0.0-rc.1"
# 或正式发布时 → "3.0.0"
# 同步 Cargo.lock:
cargo build --workspace 2>/dev/null  # 重新生成 lock
```

### Step 3: 更新 CHANGELOG
- 顶部新建版本条目
- 分类：新特性 / 关键修复 / 语言能力 / 测试覆盖 / 版本治理

### Step 4: 打 tag（本地私有）
```bash
git tag -a v3.0.0-rc.1 -m "Dalin L 3.0.0-rc.1 — commercial readiness candidate"
# 遵守发布纪律：不 push 到任何公网 remote
```

### Step 5: 最终验证
- 重跑质量门禁全套（清单第一节）
- 记录：测试数、clippy 状态、commit hash、tag

### Step 6: 归档
- 更新 docs/release-process.md 的"发布记录"表
- 将发布结论写入工作区 output/ 报告

---

## 四、发布记录

| 版本 | 日期 | 基线 commit | 测试数 | 说明 |
|------|------|------------|--------|------|
| v3.0.0-rc.1 | 2026-08-02 | 待定 | 690 | 商业化就绪候选（本流程建立后首个候选） |
| v0.1.0 | 2026-06 | — | — | 初始版本（历史） |

---

## 五、纪律（硬约束）

1. **绝不 push 公网**：git remote 恒为空，任何提交仅本地私有仓
2. **绝不开源**：除非用户显式撤销（当前有效）
3. **版本真相源唯一**：根 Cargo.toml `[workspace.package]`，禁止 crate 内硬编码版本号
4. **CHANGELOG 先于发布**：无 CHANGELOG 条目不发布
