# DevOps Pipeline Demo

模拟 CI/CD 流水线：lint → test → build → deploy，支持失败自动回滚。

## 功能

- 多阶段顺序执行 (lint / test / build / deploy)
- 多包并行处理 (core / api / cli)
- 部署失败自动触发回滚
- 流水线统计：总阶段数、通过/失败、总耗时

## 用法

```bash
dalib run demos/devops_pipeline/main.dal
```

## 输出示例

```
=== DevOps Pipeline ===
Pipeline: lint → test → build → deploy
Packages: 3

── Processing package: core ──
  [lint] checking core...
    Found 2 warnings
  [test] running tests for core...
    24/24 tests passed
  [build] compiling core...
    Artifact size: 12 MB
  [deploy] deploying core to production...
    Deploy successful!
...
=== Pipeline Summary ===
  Result: FAILED (rolled back)
  Stages executed: 14
  Passed: 12, Failed: 2
  Total time: 32.6s
```