use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use db::models::{execution_process::ExecutionProcess, session::Session};
use deployment::Deployment;
use executors::{
    executors::StandardCodingAgentExecutor,
    profile::{ExecutorConfig, ExecutorConfigs},
};
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    session_follow_up::{SessionFollowUpRequest, start_session_follow_up},
};

#[async_trait]
pub trait FeishuChatRunner: Send + Sync {
    async fn submit_workspace_chat_message(
        &self,
        session_id: Uuid,
        workspace_id: Uuid,
        prompt: &str,
    ) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopFeishuChatRunner;

#[async_trait]
impl FeishuChatRunner for NoopFeishuChatRunner {
    async fn submit_workspace_chat_message(
        &self,
        _session_id: Uuid,
        _workspace_id: Uuid,
        _prompt: &str,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct DeploymentFeishuChatRunner {
    deployment: DeploymentImpl,
}

impl DeploymentFeishuChatRunner {
    pub fn new(deployment: DeploymentImpl) -> Self {
        Self { deployment }
    }

    async fn resolve_executor_config(&self, session: &Session) -> ExecutorConfig {
        let default_profile = self
            .deployment
            .config()
            .read()
            .await
            .executor_profile
            .clone();
        let profile_id = ExecutionProcess::latest_executor_profile_for_session(
            &self.deployment.db().pool,
            session.id,
        )
        .await
        .ok()
        .flatten()
        .unwrap_or(default_profile);

        ExecutorConfigs::get_cached()
            .get_coding_agent(&profile_id)
            .map(|agent| agent.get_preset_options())
            .unwrap_or_else(|| ExecutorConfig::from(profile_id))
    }
}

#[async_trait]
impl FeishuChatRunner for DeploymentFeishuChatRunner {
    async fn submit_workspace_chat_message(
        &self,
        session_id: Uuid,
        workspace_id: Uuid,
        prompt: &str,
    ) -> Result<()> {
        let session = Session::find_by_id(&self.deployment.db().pool, session_id)
            .await?
            .context("Feishu chat session not found")?;
        ensure!(
            session.workspace_id == workspace_id,
            "Feishu chat session does not belong to workspace"
        );

        let executor_config = self.resolve_executor_config(&session).await;
        start_session_follow_up(
            &self.deployment,
            &session,
            SessionFollowUpRequest {
                prompt: prompt.to_string(),
                executor_config,
                retry_process_id: None,
                force_when_dirty: None,
                perform_git_reset: None,
            },
        )
        .await?;

        Ok(())
    }
}
