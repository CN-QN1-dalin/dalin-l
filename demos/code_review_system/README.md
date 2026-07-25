# Code Review System Demo

静态分析工具，扫描 Dalin L 源码的命名规范、未处理 Result、函数长度等，给出质量评分。

## 功能

- 命名规范检查 (CamelCase → snake_case 建议)
- 未处理 Result/Option 检测
- 综合质量评分 (A/B/C/D 等级)
- 逐行 issue 报告

## 用法

```bash
dalib run demos/code_review_system/main.dal
```

## 输出示例

```
=== Code Review System ===
Reviewing: example.dal

  Found 3 issue(s):
    [warning] line 1: Variable 'MyVar' uses CamelCase, prefer snake_case
    [info] line 0: No Result/Option handling detected

=== Code Quality Score ===
  Grade: B
  Score: 79.0/100
```