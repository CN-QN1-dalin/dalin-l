//! 控制面端到端集成测试：起一个本地 gRPC server（随机端口），用客户端提交并查询。
//! 不依赖 NATS——事件总线走 InMemory，验证完整 gRPC 路径 + 调度 + 任务树。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use control_plane::client::ControlPlaneClientWrapper;
use control_plane::control_plane_server::ControlPlaneServer;
use control_plane::dispatch::InMemoryDispatchBroker;
use control_plane::registry::InMemoryTaskStore;
use control_plane::scheduler::{Capability, CapabilityScheduler, Node};
use control_plane::server::ControlPlaneService;
use control_plane::store::TaskStore;
use control_plane::transport::{EventBus, InMemoryEventBus};

#[tokio::test]
async fn submit_and_get_over_grpc() {
    let nodes = vec![Node {
        id: "n1".into(),
        capabilities: [Capability::Cpu].into_iter().collect(),
        load: 0,
        quota: None,
    }];
    let scheduler = Arc::new(Mutex::new(CapabilityScheduler::new(nodes)));
    let registry: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let bus: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new());
    let dispatch: Arc<dyn control_plane::dispatch::DispatchBroker> =
        Arc::new(InMemoryDispatchBroker::new());
    let svc = ControlPlaneService::new(registry, scheduler, bus, dispatch);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(ControlPlaneServer::new(svc))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // 客户端带重试连上（等 server ready）
    let mut client = loop {
        match ControlPlaneClientWrapper::connect(&format!("http://{}", addr)).await {
            Ok(c) => break c,
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    };

    let task = client.submit("worker", None, "spawn", "cpu").await.unwrap();
    assert_eq!(task.name, "worker");
    assert!(!task.id.is_empty());

    let got = client.get(&task.id).await.unwrap().expect("task present");
    assert_eq!(got.id, task.id);
    assert_eq!(got.spec.unwrap().capability, "cpu");
}
