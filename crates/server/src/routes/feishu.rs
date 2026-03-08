use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{feishu_bot::FeishuBot, feishu_bot_target::FeishuBotTarget};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{
    DeploymentImpl,
    error::ApiError,
    feishu::{
        client::HttpFeishuClient,
        dispatcher::init_global_dispatcher,
        runtime_manager::{global_runtime_manager, init_global_runtime_manager},
        service::FeishuService,
        types::{
            CreateFeishuBotInput, FeishuValidationResult, SendFeishuMessageInput,
            SendFeishuMessageResult, UpdateFeishuBotInput,
        },
    },
};

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct CreateFeishuBotRequest {
    pub name: String,
    pub app_id: String,
    pub app_secret: String,
    pub encrypt_key: Option<String>,
    pub verification_token: Option<String>,
    pub tenant_mode: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct UpdateFeishuBotRequest {
    pub name: String,
    pub app_id: String,
    pub app_secret: Option<String>,
    pub encrypt_key: Option<String>,
    pub verification_token: Option<String>,
    pub tenant_mode: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ValidateFeishuBotResponse {
    pub validation: FeishuValidationResult,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct FeishuInboundCommandRequest {
    pub event_id: String,
    pub workspace_id: Uuid,
    pub binding_id: Uuid,
    pub command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct FeishuInboundCardActionRequest {
    pub event_id: String,
    pub workspace_id: Uuid,
    pub binding_id: Uuid,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct FeishuInboundActionResponse {
    pub message: String,
}

pub async fn list_bots(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<FeishuBot>>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    Ok(ResponseJson(ApiResponse::success(
        service.list_bots().await.map_err(map_feishu_error)?,
    )))
}

pub async fn create_bot(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateFeishuBotRequest>,
) -> Result<ResponseJson<ApiResponse<FeishuBot>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let bot = service
        .create_bot(CreateFeishuBotInput {
            name: payload.name,
            app_id: payload.app_id,
            app_secret: Some(SecretString::new(payload.app_secret.into())),
            encrypt_key: payload
                .encrypt_key
                .map(|value| SecretString::new(value.into())),
            verification_token: payload
                .verification_token
                .map(|value| SecretString::new(value.into())),
            tenant_mode: payload.tenant_mode,
        })
        .await
        .map_err(map_feishu_error)?;

    init_global_runtime_manager(&deployment)
        .upsert_runtime(bot.id)
        .await
        .map_err(map_feishu_error)?;

    Ok(ResponseJson(ApiResponse::success(bot)))
}

pub async fn get_bot(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<FeishuBot>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let bot = service
        .get_bot(bot_id)
        .await
        .map_err(map_feishu_error)?
        .ok_or_else(|| ApiError::BadRequest("Feishu bot not found".to_string()))?;
    Ok(ResponseJson(ApiResponse::success(bot)))
}

pub async fn update_bot(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
    Json(payload): Json<UpdateFeishuBotRequest>,
) -> Result<ResponseJson<ApiResponse<FeishuBot>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let bot = service
        .update_bot(
            bot_id,
            UpdateFeishuBotInput {
                name: payload.name,
                app_id: payload.app_id,
                app_secret: payload
                    .app_secret
                    .map(|value| SecretString::new(value.into())),
                encrypt_key: payload
                    .encrypt_key
                    .map(|value| SecretString::new(value.into())),
                verification_token: payload
                    .verification_token
                    .map(|value| SecretString::new(value.into())),
                tenant_mode: payload.tenant_mode,
                enabled: payload.enabled,
            },
        )
        .await
        .map_err(map_feishu_error)?;

    let runtime_manager = init_global_runtime_manager(&deployment);
    if bot.enabled {
        runtime_manager
            .upsert_runtime(bot.id)
            .await
            .map_err(map_feishu_error)?;
    } else {
        runtime_manager
            .stop_runtime(bot.id)
            .await
            .map_err(map_feishu_error)?;
    }

    Ok(ResponseJson(ApiResponse::success(bot)))
}

pub async fn delete_bot(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    service.delete_bot(bot_id).await.map_err(map_feishu_error)?;

    if let Some(runtime_manager) = global_runtime_manager() {
        runtime_manager
            .stop_runtime(bot_id)
            .await
            .map_err(map_feishu_error)?;
    }

    Ok(ResponseJson(ApiResponse::success(())))
}

pub async fn validate_bot(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<ValidateFeishuBotResponse>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let validation = service
        .validate_bot(&HttpFeishuClient::new(), bot_id)
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(
        ValidateFeishuBotResponse { validation },
    )))
}

pub async fn list_targets(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<FeishuBotTarget>>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    Ok(ResponseJson(ApiResponse::success(
        service
            .list_targets(bot_id)
            .await
            .map_err(map_feishu_error)?,
    )))
}

pub async fn discover_targets(
    State(deployment): State<DeploymentImpl>,
    Path(bot_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<FeishuBotTarget>>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let targets = service
        .refresh_targets(&HttpFeishuClient::new(), bot_id)
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(targets)))
}

pub async fn send_test_message(
    State(deployment): State<DeploymentImpl>,
    Path((bot_id, target_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<SendFeishuMessageInput>,
) -> Result<ResponseJson<ApiResponse<SendFeishuMessageResult>>, ApiError> {
    let service = FeishuService::from_deployment(&deployment);
    let result = service
        .send_message_to_target(
            &HttpFeishuClient::new(),
            bot_id,
            target_id,
            None,
            "manual_test_message",
            &payload.message,
        )
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(result)))
}

pub async fn handle_inbound_command(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<FeishuInboundCommandRequest>,
) -> Result<ResponseJson<ApiResponse<FeishuInboundActionResponse>>, ApiError> {
    let dispatcher = init_global_dispatcher(&deployment);
    let message = dispatcher
        .handle_inbound_command(
            &payload.event_id,
            payload.workspace_id,
            payload.binding_id,
            &payload.command,
        )
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(
        FeishuInboundActionResponse { message },
    )))
}

pub async fn handle_inbound_card_action(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<FeishuInboundCardActionRequest>,
) -> Result<ResponseJson<ApiResponse<FeishuInboundActionResponse>>, ApiError> {
    let dispatcher = init_global_dispatcher(&deployment);
    let message = dispatcher
        .handle_card_action(
            &payload.event_id,
            payload.workspace_id,
            payload.binding_id,
            &payload.action,
        )
        .await
        .map_err(map_feishu_error)?;
    Ok(ResponseJson(ApiResponse::success(
        FeishuInboundActionResponse { message },
    )))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/feishu/bots", get(list_bots).post(create_bot))
        .route(
            "/feishu/bots/{bot_id}",
            get(get_bot).patch(update_bot).delete(delete_bot),
        )
        .route("/feishu/bots/{bot_id}/validate", post(validate_bot))
        .route("/feishu/bots/{bot_id}/targets", get(list_targets))
        .route(
            "/feishu/bots/{bot_id}/targets/discover",
            post(discover_targets),
        )
        .route(
            "/feishu/bots/{bot_id}/targets/{target_id}/test-message",
            post(send_test_message),
        )
        .route("/feishu/inbound/commands", post(handle_inbound_command))
        .route("/feishu/inbound/cards", post(handle_inbound_card_action))
}

fn map_feishu_error(error: impl std::fmt::Display) -> ApiError {
    let message = error.to_string();
    if message.contains("UNIQUE constraint failed") {
        ApiError::Conflict(message)
    } else {
        ApiError::BadRequest(message)
    }
}
