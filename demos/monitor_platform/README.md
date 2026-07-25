# Monitor Platform Demo

模拟服务器监控仪表盘，采集 CPU/内存/错误率虚拟指标，并在超过阈值时触发告警。

## 功能

- 生成 8 个模拟数据点（CPU/MEM/ERR）
- ASCII 柱状图展示最新指标快照
- 阈值告警检测（CPU > 90%, MEM > 85%, ERR > 5/s）
- 警报列表输出

## 用法

```bash
dalib run demos/monitor_platform/main.dal
```

## 输出示例

```
=== Monitor Platform Report ===
Server: production-01
Metrics collected: 8

Latest Metrics Snapshot:
CPU [##############################] 95.0
MEM [###########################] 88.0
ERR [***********************] 7.5

Alert Check:
  CPU threshold: > 90%
  MEM threshold: > 85%
  ERR threshold: > 5/s

  Alerts triggered: 3
    [cpu] sample=7 value=95 exceeded 90
    [mem] sample=7 value=88 exceeded 85
    [err] sample=7 value=7.5 exceeded 5

=== End of Report ===
```