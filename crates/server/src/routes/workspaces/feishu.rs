use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, patch, post},
};
use db::models::{
    feishu_bot::FeishuBot,
    feishu_bot_target::FeishuBotTarget,
    workspace::Workspace,
    workspace_feishu_binding::{UpdateWorkspaceFeishuBinding, WorkspaceFeishuBinding},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    feishu::{client::HttpFeishuClient, service::FeishuService},
};

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct CreateWorkspaceFeishuBindingRequest {
    pub bot_id: Uuid,
    pub target_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct UpdateWorkspaceFeishuBindingRequest {
    pub enabled: bool,
    pub notify_on_status: bool,
    pub notify_on_agent_reply: bool,
    pub notify_on_pr: bool,
    pub allow_commands: bool,
    pub allow_chat_messages: bool,
    pub allow_cards: bool,
    pub ack_reaction_enabled: bool,
    pub ack_reaction_emoji_type: String,
    pub sync_group_announcement: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceBindingPathParams {
    id: Uuid,
    binding_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorkspaceFeishuBindTarget {
    pub bot: FeishuBot,
    pub targets: Vec<FeishuBotTarget>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct SendWorkspaceFeishuTestMessageRequest {
    pub bot_id: Uuid,
    pub target_id: Uuid,
    pub message: Option<String>,
}

pub async fn list_bindings(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<WorkspaceFeishuBinding>>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    Ok(ResponseJson(ApiResponse::success(
        service
            .list_bindings(workspace.id)
            .await
            .map_err(map_feishu_error)?,
    )))
}

pub async fn create_binding(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateWorkspaceFeishuBindingRequest>,
) -> Result<ResponseJson<ApiResponse<WorkspaceFeishuBinding>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let binding = service
        .create_binding(workspace.id, payload.bot_id, payload.target_id)
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(binding)))
}

pub async fn update_binding(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Path(params): Path<WorkspaceBindingPathParams>,
    Json(payload): Json<UpdateWorkspaceFeishuBindingRequest>,
) -> Result<ResponseJson<ApiResponse<WorkspaceFeishuBinding>>, ApiError> {
    let _workspace_id = params.id;
    let binding_id = params.binding_id;
    let service = FeishuService::from_deployment(&deployment);
    let existing = service
        .list_bindings(workspace.id)
        .await
        .map_err(map_feishu_error)?
        .into_iter()
        .find(|binding| binding.id == binding_id)
        .ok_or_else(|| ApiError::BadRequest("Feishu binding not found".to_string()))?;

    let binding = service
        .update_binding(
            existing.id,
            UpdateWorkspaceFeishuBinding {
                enabled: payload.enabled,
                notify_on_status: payload.notify_on_status,
                notify_on_agent_reply: payload.notify_on_agent_reply,
                notify_on_pr: payload.notify_on_pr,
                allow_commands: payload.allow_commands,
                allow_chat_messages: payload.allow_chat_messages,
                allow_cards: payload.allow_cards,
                ack_reaction_enabled: payload.ack_reaction_enabled,
                ack_reaction_emoji_type: payload.ack_reaction_emoji_type,
                sync_group_announcement: payload.sync_group_announcement,
            },
        )
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(binding)))
}

pub async fn delete_binding(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Path(params): Path<WorkspaceBindingPathParams>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let _workspace_id = params.id;
    let binding_id = params.binding_id;
    let service = FeishuService::from_deployment(&deployment);
    let binding = service
        .list_bindings(workspace.id)
        .await
        .map_err(map_feishu_error)?
        .into_iter()
        .find(|binding| binding.id == binding_id)
        .ok_or_else(|| ApiError::BadRequest("Feishu binding not found".to_string()))?;
    service
        .delete_binding(binding.id)
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn list_bindable_targets(
    Extension(_workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<WorkspaceFeishuBindTarget>>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let bots = service.list_bots().await.map_err(map_feishu_error)?;
    let mut options = Vec::with_capacity(bots.len());

    for bot in bots {
        let targets = service
            .list_targets(bot.id)
            .await
            .map_err(map_feishu_error)?;
        options.push(WorkspaceFeishuBindTarget { bot, targets });
    }

    Ok(ResponseJson(ApiResponse::success(options)))
}

pub async fn send_workspace_test_message(
    Extension(workspace): Extension<Workspace>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<SendWorkspaceFeishuTestMessageRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let message = payload.message.unwrap_or_else(|| {
        format!(
            "Vibe Kanban test message for workspace {}",
            workspace.name.as_deref().unwrap_or(&workspace.branch)
        )
    });

    service
        .send_message_to_target(
            &HttpFeishuClient::new(),
            payload.bot_id,
            payload.target_id,
            Some(workspace.id),
            "manual_test_message",
            &message,
        )
        .await
        .map_err(map_feishu_error)?;

    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/bindings", get(list_bindings).post(create_binding))
        .route(
            "/bindings/{binding_id}",
            patch(update_binding).delete(delete_binding),
        )
        .route("/bindable-targets", get(list_bindable_targets))
        .route("/test-message", post(send_workspace_test_message))
}

fn map_feishu_error(error: impl std::fmt::Display) -> ApiError {
    let message = error.to_string();
    if message.contains("UNIQUE constraint failed") || message.contains("already bound") {
        ApiError::Conflict(message)
    } else {
        ApiError::BadRequest(message)
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
        routing::{delete, patch},
    };
    use tower::util::ServiceExt;

    use super::*;

    async fn extract_binding_id(
        Path(params): Path<WorkspaceBindingPathParams>,
    ) -> impl IntoResponse {
        params.binding_id.to_string()
    }

    #[tokio::test]
    async fn nested_patch_binding_route_extracts_binding_id() {
        let workspace_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let app = Router::<()>::new().nest(
            "/workspaces/{id}/feishu",
            Router::new().route("/bindings/{binding_id}", patch(extract_binding_id)),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!(
                        "/workspaces/{workspace_id}/feishu/bindings/{binding_id}"
                    ))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("request succeeds");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn nested_delete_binding_route_extracts_binding_id() {
        let workspace_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let app = Router::<()>::new().nest(
            "/workspaces/{id}/feishu",
            Router::new().route("/bindings/{binding_id}", delete(extract_binding_id)),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/workspaces/{workspace_id}/feishu/bindings/{binding_id}"
                    ))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("request succeeds");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
