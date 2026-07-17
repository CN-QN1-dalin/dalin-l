//! 控制面 gRPC 客户端封装

use crate::control_plane_client::ControlPlaneClient;
use crate::{GetTaskRequest, SubmitTaskRequest, Task, TaskSpec};
use tonic::transport::Channel;

pub struct ControlPlaneClientWrapper {
    inner: ControlPlaneClient<Channel>,
}

impl ControlPlaneClientWrapper {
    pub async fn connect(dst: &str) -> Result<Self, tonic::transport::Error> {
        let inner = ControlPlaneClient::connect(dst.to_string()).await?;
        Ok(Self { inner })
    }

    pub async fn submit(
        &mut self,
        name: &str,
        parent: Option<&str>,
        effect: &str,
        capability: &str,
    ) -> Result<Task, tonic::Status> {
        let req = SubmitTaskRequest {
            name: name.to_string(),
            parent: parent.map(str::to_string).unwrap_or_default(),
            spec: Some(TaskSpec {
                effect: effect.to_string(),
                capability: capability.to_string(),
                idempotency_key: String::new(),
            }),
        };
        let resp = self.inner.submit_task(req).await?;
        Ok(resp.into_inner().task.ok_or_else(|| tonic::Status::internal("empty task response"))?)
    }

    pub async fn get(&mut self, id: &str) -> Result<Option<Task>, tonic::Status> {
        let resp = self.inner.get_task(GetTaskRequest { id: id.to_string() }).await?;
        Ok(resp.into_inner().task)
    }
}
