//! Dalin L 3.0 — K8s Operator layer
//!
//! Maps DalinTask CRD → Kubernetes Deployments using the 7-channel metadata.
//!
//! Modules:
//! - `operator_types`: DalinTask spec/status types, resource resolver, replica strategies
//! - `controller`: SchedulerController + DeploymentDesire (pure Rust, no kube-rs dependency)

pub mod controller;
pub mod operator_types;

pub use controller::SchedulerController;
pub use operator_types::*;
