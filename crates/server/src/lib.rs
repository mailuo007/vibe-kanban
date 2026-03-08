pub mod error;
pub mod feishu;
pub mod middleware;
pub mod preview_proxy;
pub mod routes;
pub mod session_follow_up;
pub mod tunnel;

// #[cfg(feature = "cloud")]
// type DeploymentImpl = vibe_kanban_cloud::deployment::CloudDeployment;
// #[cfg(not(feature = "cloud"))]
pub type DeploymentImpl = local_deployment::LocalDeployment;
