//! gRPC 控制面服务实现（Phase 2 调度入口）
//!
//! 把注册表 / 调度器 / 事件总线 / 派发总线 / Agent 注册表串起来：
//! SubmitTask 做能力放置 → 写入任务树 → 经 DispatchBroker 派发到对应
//! capability topic → 状态变更推给 WatchTasks 订阅者 + 事件总线。

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures_core::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{async_trait, Request, Response, Status};

use crate::agent_registry::{AgentRegistryService, NodeRegistry};
use crate::agent_registry_server::AgentRegistryServer;
use crate::control_plane_server::{ControlPlane, ControlPlaneServer};
use crate::dispatch::{build_dispatch_task, DispatchBroker};
use crate::effect_monitor::EffectMonitor;
use crate::{
    CancelTaskRequest, CancelTaskResponse, GetTaskRequest, GetTaskResponse, ListTasksRequest,
    ListTasksResponse, SubmitTaskRequest, SubmitTaskResponse, Task, TaskSpec as PbTaskSpec,
    TaskStatus as PbTaskStatus,
};
use crate::registry::{TaskEvent, TaskRecord, TaskStatus};
use crate::scheduler::{Capability, CapabilityScheduler, Placement};
use crate::store::TaskStore;
use crate::transport::EventBus;

pub struct ControlPlaneService {
    registry: Arc<dyn TaskStore>,
    scheduler: Arc<Mutex<CapabilityScheduler>>,
    bus: Arc<dyn EventBus>,
    dispatch: Arc<dyn DispatchBroker>,
}

impl ControlPlaneService {
    pub fn new(
        registry: Arc<dyn TaskStore>,
        scheduler: Arc<Mutex<CapabilityScheduler>>,
        bus: Arc<dyn EventBus>,
        dispatch: Arc<dyn DispatchBroker>,
    ) -> Self {
        Self {
            registry,
            scheduler,
            bus,
            dispatch,
        }
    }
}

fn task_status_to_proto(s: &TaskStatus) -> PbTaskStatus {
    match s {
        TaskStatus::Queued => PbTaskStatus::Queued,
        TaskStatus::Scheduled => PbTaskStatus::Scheduled,
        TaskStatus::Running => PbTaskStatus::Running,
        TaskStatus::Succeeded => PbTaskStatus::Succeeded,
        TaskStatus::Failed => PbTaskStatus::Failed,
        TaskStatus::Canceled => PbTaskStatus::Canceled,
    }
}

fn task_to_proto(rec: &TaskRecord, node: Option<String>) -> Task {
    Task {
        id: rec.id.clone(),
        name: rec.name.clone(),
        parent: rec.parent.clone().unwrap_or_default(),
        spec: Some(PbTaskSpec {
            effect: rec.effect.clone(),
            capability: rec.capability.clone(),
            idempotency_key: rec.idempotency_key.clone(),
        }),
        status: task_status_to_proto(&rec.status) as i32,
        node: node.or_else(|| rec.node.clone()).unwrap_or_default(),
        submitted_at: rec.submitted_at,
    }
}

#[async_trait]
impl ControlPlane for ControlPlaneService {
    async fn submit_task(
        &self,
        req: Request<SubmitTaskRequest>,
    ) -> Result<Response<SubmitTaskResponse>, Status> {
        let r = req.into_inner();
        let effect = r.spec.as_ref().map(|s| s.effect.clone()).unwrap_or_default();
        let capability = r
            .spec
            .as_ref()
            .map(|s| s.capability.clone())
            .unwrap_or_default();
        let idem = r
            .spec
            .as_ref()
            .map(|s| s.idempotency_key.clone())
            .unwrap_or_default();
        let parent_ref: Option<&str> = if r.parent.is_empty() {
            None
        } else {
            Some(&r.parent)
        };

        // 效应检查：任务的效应必须在上下文允许范围内
        // pure 上下文禁 spawn/io/async，spawn 上下文允许 spawn
        let mut monitor = EffectMonitor::new();
        monitor.set_context("spawn");  // 控制面上下文默认为 spawn（可派生任何子任务）
        if let Err(v) = monitor.check_effect(&effect) {
            return Err(Status::failed_precondition(format!(
                "效应违规: {v} (task={}, effect={})", r.name, effect
            )));
        }

        // 能力放置（调度器已加锁，Mutex 保护）
        let placement: Option<Placement> = {
            let sched = self.scheduler.lock().unwrap();
            capability
                .parse::<Capability>()
                .ok()
                .and_then(|c| sched.place(&c))
        };
        let node = placement.as_ref().map(|p| p.node_id.clone());

        let rec = self
            .registry
            .register(&r.name, parent_ref, &effect, &capability, &idem)
            .await;
        if let Some(n) = &node {
            self.registry.assign_node(&rec.id, n).await;
            self.registry.set_status(&rec.id, TaskStatus::Scheduled).await;
        }

        // 通过 DispatchBroker 派发任务
        let task_id = rec.id.clone();
        let parent_opt = if r.parent.is_empty() { None } else { Some(r.parent.clone()) };
        let dt = build_dispatch_task(&task_id, &r.name, &effect, &capability, parent_opt);
        if let Err(e) = self.dispatch.dispatch(&dt).await {
            eprintln!("[cp] dispatch error: {e}");
        }

        self.bus.publish(&TaskEvent::Submitted(rec.clone())).await;

        Ok(Response::new(SubmitTaskResponse {
            task: Some(task_to_proto(&rec, node)),
        }))
    }

    async fn get_task(
        &self,
        req: Request<GetTaskRequest>,
    ) -> Result<Response<GetTaskResponse>, Status> {
        let id = req.into_inner().id;
        match self.registry.get(&id).await {
            Some(rec) => Ok(Response::new(GetTaskResponse {
                task: Some(task_to_proto(&rec, rec.node.clone())),
            })),
            None => Err(Status::not_found(format!("task not found: {}", id))),
        }
    }

    async fn list_tasks(
        &self,
        req: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let parent = req.into_inner().parent;
        let parent_ref: Option<&str> = if parent.is_empty() {
            None
        } else {
            Some(&parent)
        };
        let tasks = self
            .registry
            .list(parent_ref)
            .await
            .into_iter()
            .map(|rec| task_to_proto(&rec, rec.node.clone()))
            .collect();
        Ok(Response::new(ListTasksResponse { tasks }))
    }

    async fn cancel_task(
        &self,
        req: Request<CancelTaskRequest>,
    ) -> Result<Response<CancelTaskResponse>, Status> {
        let id = req.into_inner().id;
        let ok = self.registry.cancel(&id).await;
        // 释放被取消任务占用的节点并发槽
        if ok
            && let Some(rec) = self.registry.get(&id).await
                && let Some(node) = &rec.node {
                    let sched = self.scheduler.lock().unwrap();
                    sched.release(node);
                }
        Ok(Response::new(CancelTaskResponse { ok }))
    }

    type WatchTasksStream = Pin<Box<dyn Stream<Item = Result<Task, Status>> + Send>>;

    async fn watch_tasks(
        &self,
        req: Request<crate::WatchTasksRequest>,
    ) -> Result<Response<Self::WatchTasksStream>, Status> {
        let parent = req.into_inner().parent;
        let rx = self.registry.subscribe().await;
        let stream = BroadcastStream::new(rx).filter_map(move |ev| {
            let parent = parent.clone();
            match ev {
                Ok(TaskEvent::Submitted(rec)) | Ok(TaskEvent::StatusChanged(rec)) => {
                    let matches = if parent.is_empty() {
                        true
                    } else {
                        rec.parent.as_deref() == Some(parent.as_str())
                    };
                    if matches {
                        Some(Ok(task_to_proto(&rec, rec.node.clone())))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

/// 启动 gRPC 控制面服务（ControlPlane + AgentRegistry）。
pub async fn serve(
    addr: std::net::SocketAddr,
    registry: Arc<dyn TaskStore>,
    scheduler: Arc<Mutex<CapabilityScheduler>>,
    bus: Arc<dyn EventBus>,
    node_registry: Arc<NodeRegistry>,
    dispatch: Arc<dyn DispatchBroker>,
) -> Result<(), tonic::transport::Error> {
    let cp_svc = ControlPlaneService::new(registry, scheduler, bus, dispatch);
    let ar_svc = AgentRegistryService::new(node_registry);
    tonic::transport::Server::builder()
        .add_service(ControlPlaneServer::new(cp_svc))
        .add_service(AgentRegistryServer::new(ar_svc))
        .serve(addr)
        .await
}
