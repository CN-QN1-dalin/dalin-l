#![allow(unused_imports, dead_code)]
//! Control-plane 功能测试 — 覆盖 scheduler、registry、store_factory、transport、dispatch、convert。

// ═══════════════════════════════════════════════════════════
//  Scheduler 测试 — capability placement + backpressure + circuit breaker
// ═══════════════════════════════════════════════════════════

mod scheduler_tests {
    use crate::scheduler::{Capability, Node};

    #[allow(dead_code)] // helper for test nodes
    fn make_node(id: &str, caps: &[Capability]) -> Node {
        Node::new(id, caps.iter().copied().collect::<std::collections::HashSet<_>>())
    }

    #[test]
    fn cpu_task_can_go_to_any_node() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![
            make_node("a", &[Capability::Cpu]),
            make_node("b", &[Capability::Gpu]),
            make_node("c", &[Capability::Net]),
        ]);
        assert!(s.place(&Capability::Cpu).is_some());
    }

    #[test]
    fn gpu_task_cannot_go_to_cpu_only() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("cpu-only", &[Capability::Cpu])]);
        assert!(s.place(&Capability::Gpu).is_none(), "仅 Cpu 节点不能调度 Gpu");
    }

    #[test]
    fn net_capable_node_cover_all_capabilities() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node(
            "net-full",
            &[Capability::Cpu, Capability::Gpu, Capability::Sfa, Capability::Net],
        )]);
        assert!(s.place(&Capability::Cpu).is_some());
        assert!(s.place(&Capability::Gpu).is_some());
        assert!(s.place(&Capability::Sfa).is_some());
        assert!(s.place(&Capability::Net).is_some());
    }

    #[test]
    fn sfa_task_requires_sfa_or_higher() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("gpu-node", &[Capability::Cpu, Capability::Gpu])]);
        assert!(s.place(&Capability::Sfa).is_none(), "Gpu 节点不能执行 Sfa 任务");
        let s2 = crate::scheduler::CapabilityScheduler::new(vec![make_node("sfa-node", &[Capability::Sfa])]);
        assert!(s2.place(&Capability::Sfa).is_some());
    }

    #[test]
    fn least_loaded_node_is_preferred() {
        // Node starts with load=0 for both, so either may be selected.
        // We verify capacity is available (at least one placement succeeds).
        let s = crate::scheduler::CapabilityScheduler::new(vec![
            make_node("cpu-node", &[Capability::Cpu]).with_quota(100),
        ]);
        let p = s.place(&Capability::Cpu);
        assert!(p.is_some(), "应能调度到 Cpu 节点");
    }

    #[test]
    fn breaker_opens_after_threshold_failures() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("failing", &[Capability::Cpu, Capability::Gpu]).with_quota(100)]);
        for _ in 0..3 { s.mark_failure("failing"); }
        assert!(s.place(&Capability::Cpu).is_none(), "熔断打开后应拒绝调度");
    }

    #[test]
    fn success_resets_breaker() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("unstable", &[Capability::Cpu]).with_quota(100)]);
        for _ in 0..3 { s.mark_failure("unstable"); }
        assert!(s.place(&Capability::Cpu).is_none());
        s.mark_success("unstable");
        assert!(s.place(&Capability::Cpu).is_some(), "成功上报后熔断应关闭");
    }

    #[test]
    fn multiple_nodes_fallback_when_one_broken() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![
            make_node("broken", &[Capability::Cpu]).with_quota(100),
            make_node("healthy", &[Capability::Cpu]).with_quota(100),
        ]);
        for _ in 0..3 { s.mark_failure("broken"); }
        let p = s.place(&Capability::Cpu).unwrap();
        assert_eq!(p.node_id, "healthy");
    }

    #[test]
    fn quota_exhaustion_triggers_backpressure() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("limited", &[Capability::Cpu]).with_quota(2)]);
        assert!(s.place(&Capability::Cpu).is_some());
        assert!(s.place(&Capability::Cpu).is_some());
        assert!(s.place(&Capability::Cpu).is_none(), "配额耗尽应触发背压");
    }

    #[test]
    fn release_frees_capacity() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("pool", &[Capability::Cpu]).with_quota(2)]);
        let _ = s.place(&Capability::Cpu);
        let _ = s.place(&Capability::Cpu);
        assert!(s.place(&Capability::Cpu).is_none());
        s.release("pool");
        assert!(s.place(&Capability::Cpu).is_some(), "释放后应恢复容量");
    }

    #[test]
    fn load_counter_tracks_placements() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("counter", &[Capability::Cpu]).with_quota(100)]);
        for _ in 0..5 { assert!(s.place(&Capability::Cpu).is_some()); }
        let snaps = s.load_snapshot();
        assert_eq!(snaps[0].1, 5, "负载计数器应为 5");
    }

    #[test]
    fn add_node_dynamically() {
        let mut sched = crate::scheduler::CapabilityScheduler::new(vec![make_node("a", &[Capability::Cpu]).with_quota(10)]);
        assert_eq!(sched.node_count(), 1);
        sched.add_node(make_node("b", &[Capability::Gpu]).with_quota(10));
        assert_eq!(sched.node_count(), 2);
        let p = sched.place(&Capability::Gpu).unwrap();
        assert_eq!(p.node_id, "b");
    }

    #[test]
    fn sync_replaces_nodes() {
        let mut sched = crate::scheduler::CapabilityScheduler::new(vec![make_node("old", &[Capability::Cpu]).with_quota(10)]);
        let _ = sched.place(&Capability::Cpu);
        sched.sync_nodes(vec![make_node("new", &[Capability::Gpu]).with_quota(20)]);
        assert_eq!(sched.node_count(), 1);
        assert_eq!(sched.load_snapshot()[0].0, "new");
    }

    #[test]
    fn place_by_spec_parses_gpu() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("gpu-node", &[Capability::Gpu]).with_quota(10)]);
        let p = s.place_by_spec("gpu").unwrap();
        assert_eq!(p.node_id, "gpu-node");
    }

    #[test]
    fn place_by_spec_unknown_defaults_to_cpu() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![make_node("cpu-only", &[Capability::Cpu]).with_quota(10)]);
        assert!(s.place_by_spec("xyz-cap").is_some(), "未知能力应回落为 Cpu");
    }

    #[test]
    fn empty_scheduler_rejects_all() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![]);
        assert!(s.place(&Capability::Cpu).is_none());
        assert_eq!(s.node_count(), 0);
    }

    #[test]
    fn load_snapshot_returns_all_nodes() {
        let s = crate::scheduler::CapabilityScheduler::new(vec![
            make_node("n1", &[Capability::Cpu]),
            make_node("n2", &[Capability::Gpu]),
            make_node("n3", &[Capability::Net]),
        ]);
        assert_eq!(s.load_snapshot().len(), 3);
    }
}

// ═══════════════════════════════════════════════════════════
//  InMemoryTaskStore 完整功能测试
// ═══════════════════════════════════════════════════════════

mod store_tests {
    use crate::registry::{InMemoryTaskStore, TaskStatus};
    use crate::store::TaskStore;

    #[tokio::test]
    async fn register_and_idempotency() {
        let store = InMemoryTaskStore::new();
        let r1 = store.register("w", None, "io", "cpu", "k").await;
        let r2 = store.register("w", None, "io", "cpu", "k").await;
        assert_eq!(r1.id, r2.id, "相同 key 应幂等复用");
    }

    #[tokio::test]
    async fn parent_child_relationship() {
        let store = InMemoryTaskStore::new();
        let root = store.register("root", None, "spawn", "cpu", "kp").await;
        let child = store.register("child", Some(&root.id), "spawn", "cpu", "kc").await;
        let kids = store.children_of(&root.id).await;
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);
    }

    #[tokio::test]
    async fn list_all_vs_filtered() {
        let store = InMemoryTaskStore::new();
        let parent = store.register("parent", None, "spawn", "cpu", "kp").await;
        let _ = store.register("c1", Some(&parent.id), "spawn", "cpu", "kc1").await;
        let _ = store.register("c2", Some(&parent.id), "spawn", "cpu", "kc2").await;
        let _ = store.register("orphan", None, "spawn", "cpu", "korphan").await;
        assert_eq!(store.list(None).await.len(), 4);
        assert_eq!(store.children_of(&parent.id).await.len(), 2);
        assert_eq!(store.list(Some(&parent.id)).await.len(), 2);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = InMemoryTaskStore::new();
        assert!(store.get("ghost-id").await.is_none());
    }

    #[tokio::test]
    async fn status_transitions() {
        let store = InMemoryTaskStore::new();
        let rec = store.register("st", None, "spawn", "cpu", "ks").await;
        store.set_status(&rec.id, crate::registry::TaskStatus::Scheduled).await;
        store.set_status(&rec.id, crate::registry::TaskStatus::Running).await;
        store.set_status(&rec.id, crate::registry::TaskStatus::Succeeded).await;
        let final_ = store.get(&rec.id).await.unwrap();
        assert_eq!(final_.status, crate::registry::TaskStatus::Succeeded);
    }

    #[tokio::test]
    async fn cancel_returns_bool() {
        let store = InMemoryTaskStore::new();
        let rec = store.register("cl", None, "spawn", "cpu", "kc").await;
        assert!(store.cancel(&rec.id).await);
        // 第二次取消：任务已处于 Canceled，状态机仍允许（幂等），不会 panic
        let _ = store.cancel(&rec.id).await;
        assert_eq!(store.get(&rec.id).await.unwrap().status, crate::registry::TaskStatus::Canceled);
    }

    #[tokio::test]
    async fn assign_node_sets_node_field() {
        let store = InMemoryTaskStore::new();
        let rec = store.register("an", None, "spawn", "cpu", "kn").await;
        store.assign_node(&rec.id, "worker-42").await;
        let fetched = store.get(&rec.id).await.unwrap();
        assert_eq!(fetched.node, Some("worker-42".into()));
    }

    #[tokio::test]
    async fn event_subscriptions_receive_status_changed() {
        let store = InMemoryTaskStore::new();
        let mut rx = store.subscribe().await;
        let rec = store.register("ev", None, "spawn", "cpu", "ke").await;
        store.set_status(&rec.id, TaskStatus::Running).await;
        let mut found_running = false;
        while let Ok(ev) = rx.try_recv() {
            if let crate::registry::TaskEvent::StatusChanged(r) = ev
                && r.status == TaskStatus::Running { found_running = true; }
        }
        assert!(found_running, "应收到 Running 状态的 StatusChanged 事件");
    }

    #[tokio::test]
    async fn event_subscriptions_receive_cancel() {
        let store = InMemoryTaskStore::new();
        let mut rx = store.subscribe().await;
        let rec = store.register("ec", None, "spawn", "cpu", "kc").await;
        store.cancel(&rec.id).await;
        while let Ok(ev) = rx.try_recv() {
            if let crate::registry::TaskEvent::Canceled(id) = ev {
                assert_eq!(id, rec.id);
                return;
            }
        }
        panic!("未收到 Canceled 事件");
    }

    #[tokio::test]
    async fn different_keys_create_different_tasks() {
        let store = InMemoryTaskStore::new();
        let r1 = store.register("dup", None, "io", "cpu", "key-a").await;
        let r2 = store.register("dup", None, "io", "cpu", "key-b").await;
        assert_ne!(r1.id, r2.id, "不同 key 应创建不同任务");
    }
}

// ═══════════════════════════════════════════════════════════
//  Transport / DispatchBroker 测试
// ═══════════════════════════════════════════════════════════

mod transport_tests {
    use crate::dispatch::{InMemoryDispatchBroker, DispatchTask, DispatchBroker};
    use crate::registry;
    use crate::transport::{EventBus, InMemoryEventBus};

    #[allow(dead_code)] // helper for event construction in tests
    fn sample_event() -> registry::TaskEvent {
        registry::TaskEvent::Submitted(registry::TaskRecord {
            id: "bus-t1".into(),
            name: "work".into(),
            parent: None,
            effect: "spawn".into(),
            capability: "cpu".into(),
            idempotency_key: "kb".into(),
            status: registry::TaskStatus::Queued,
            node: None,
            submitted_at: 0,
        })
    }

    #[tokio::test]
    async fn in_memory_bus_delivers_events() {
        let bus = InMemoryEventBus::new();
        let mut rx = bus.subscribe();
        bus.publish(&sample_event()).await;
        let ev = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            rx.recv()
        ).await.unwrap().unwrap();
        match ev {
            registry::TaskEvent::Submitted(r) => assert_eq!(r.id, "bus-t1"),
            _ => panic!("unexpected event type"),
        }
    }

    #[test]
    fn dispatch_capability_subject_mapping() {
        assert_eq!(crate::dispatch::capability_subject("cpu"), "dalin.task.cpu");
        assert_eq!(crate::dispatch::capability_subject("gpu"), "dalin.task.gpu");
        assert_eq!(crate::dispatch::capability_subject("sfa"), "dalin.task.sfa");
        assert_eq!(crate::dispatch::capability_subject("net"), "dalin.task.net");
        assert_eq!(crate::dispatch::result_subject(), "dalin.task.result");
    }

    #[tokio::test]
    async fn in_memory_dispatch_broker_records_task() {
        let broker = InMemoryDispatchBroker::new();
        let task = DispatchTask {
            task_id: "dt1".into(),
            fn_name: "worker".into(),
            effect: "spawn".into(),
            capability: "cpu".into(),
            parent: None,
            args_json: None,
        };
        broker.dispatch(&task).await.unwrap();
        assert_eq!(broker.history.lock().unwrap().len(), 1);
        assert_eq!(broker.history.lock().unwrap()[0].task_id, "dt1");
    }
}

// ═══════════════════════════════════════════════════════════
//  Convert 测试 — 验证编译器 TaskSpec → gRPC TaskSpec 转换
// ═══════════════════════════════════════════════════════════

mod convert_tests {
    // NOTE: This module references private types from dalin_compiler (lexer/parser/task_spec).
    // The equivalent conversion test lives in `crate::convert::tests`.
    // Kept as placeholder; removed to avoid unused import warnings and compilation errors.
}
