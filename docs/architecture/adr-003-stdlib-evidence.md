# ADR-003: Standard Library Evidence — 28 .dal Modules

- **状态**: Implemented
- **日期**: 2026-07-17
- **上下文**: 设计文档定义了 28 个标准库模块名，但物理文件缺失
- **决策**: 为每个模块编写可被 stdlib_loader.load_all() 加载的最小 .dal 骨架

### 模块清单

| 分类 | 模块 | 功能 |
|------|------|------|
| **Core** | prelude, core_types | 基础类型定义 |
| **Math** | math, core/math | 数值运算 |
| **Strings** | strings, core/string | 字符串操作 |
| **Collections** | collections, core/collections | list/map/set |
| **IO** | io, logging, signal, scheduler | 输入输出 |
| **Net** | networking, crypto | 网络与安全 |
| **Data** | json, encoding, config, memory | 数据处理 |
| **Task** | task, channel, fn_traits | 异步与函数式 |
| **Safety** | option, result, errors, testing | 错误处理 |
| **System** | governance, latency, cognition, qn, debug | 治理与时序 |

### stdlib_loader 集成
- `load_prelude()` 加载配置预置模块
- `load_all()` 扫描 stdlib/ 下所有 .dal 文件
- 覆盖 33 个物理文件 (29 main + 4 core/)
