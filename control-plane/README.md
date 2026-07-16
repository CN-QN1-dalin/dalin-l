# Dalin L 控制面（Phase 2 脚手架）

## 架构

```
cpd (bin) 
 ├── server (gRPC ControlPlane, tonic 0.12)
 ├── scheduler (能力格放置 + 背压/配额/熔断)
 ├── registry (InMemoryTaskStore — 内存实现)
 ├── store (TaskStore trait — 持久化 seam)
 │   ├── InMemoryTaskStore (默认)
 │   ├── RedisTaskStore (redis://, 跨节点 via pub/sub) ★ 推荐
 │   ├── EtcdTaskStore (etcd://, 跨节点 via watch)
 │   └── PostgresTaskStore (postgres://, 本地进程内)
 ├── transport (EventBus — NATS / InMemory)
 ├── convert (TaskSpec ↔ proto 转换)
 └── client (gRPC 客户端)
```

## 启动

```bash
# 单机模式（内存后端 + 内存总线）
cargo run -p control-plane --bin cpd

# Redis 后端 + NATS 集群
TASK_STORE="redis://127.0.0.1:6379" NATS_URLS="nats://host1:4222,nats://host2:4222" \
  cargo run -p control-plane --bin cpd
```

## 运行测试

```bash
# 核心测试（无外部依赖）
cargo test -p control-plane

# 门控集成测试（需启动对应服务）
docker compose -f control-plane/docker-compose.yml up -d
cargo test -p control-plane -- --ignored store_redis  # Redis
cargo test -p control-plane -- --ignored store_etcd   # Etcd
cargo test -p control-plane -- --ignored postgres     # Postgres
```

## 已验证的后端

| 后端 | 契约测 | 跨节点事件 | 生产推荐 |
|------|--------|-----------|---------|
| InMemory | ✅ | ❌（单进程） | 测试/开发 |
| Redis | ✅ | ✅（pub/sub） | ★ 推荐 |
| Etcd | ✅ | ⚠️（易失环境未实跑） | 可选 |
| Postgres | ✅ | ⚠️（tokio-postgres 0.7 + PG18 限制） | 单实例 |

所有后端均通过 `TaskStore` trait 通用契约验收（`exercise_store`）。
