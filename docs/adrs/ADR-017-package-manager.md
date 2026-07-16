# ADR-017: Dalin L 包管理器 — dalib pkg

## Status
Accepted

## Context
Dalin L 当前没有包管理系统，开发者要复用第三方库只能手动复制 `.dal` 文件到项目目录。随着生态扩展，这会导致：
1. 版本冲突无法解决
2. 依赖传递性无法声明
3. 包发布/安装流程不透明
4. 开发者体验远低于 Rust/Cargo 的基本门槛

## Decision
**Cargo 风格包管理器 `dalib pkg`**，但根据 Dalin L 的特性做裁剪：
- **manifest 格式**：`project.dcl`（Dalin Config），YAML 或 JSON
- **仓库**：`.dalan/packages/`（本地缓存）+ `https://packages.dalin-lang.org`（远程索引）
- **命令集**：

```bash
# 初始化
dalib pkg init my_project --version=0.1.0

# 添加依赖
dalib pkg add serde_json@0.24
dalib pkg add uuid@1.6 --dev
dalib pkg add jsonwebtoken@3.1 --git https://github.com/user/repo

# 更新依赖
dalib pkg update
dalib pkg upgrade serde_json        # 升级单一包

# 发布/发布到远程仓库
dalib pkg publish --registry=https://my-registry.io

# 查询/搜索
dalib pkg search uuid
dalib pkg show jsonwebtoken
dalib pkg list

# 构建/测试/清理
dalib pkg build
dalib pkg test
dalib pkg clean
dalib pkg outdated
```

- **依赖解析策略**：
  - 语义化版本 `SemVer` 约束（`"1.2.x"`, `"^2.0"`, `">=0.3"`）
  - `cargo-resolve` 风格的统一解析（一个 crate 多个版本只保留一个 instance）
  - 可选依赖通过 `--optional` 标记

## Consequences

### 变得更轻松
- 开发者一行命令获取第三方库，开发效率质的飞跃
- SemVer 约束提供依赖稳定性保证
- 包结构清晰：`pkg.json` 声明，`src/` 包含源码
- 社区分发标准化，第三方库作者知道如何打包发布

### 变难的事情
- 需要额外维护仓库服务端或至少索引 API
- `cargo-resolve` 风格的统一解析算法编写（约 500-800 行）
- 需要处理网络超时、重试、TLS 证书等细节
- 初期包数量少，生态冷启动问题
