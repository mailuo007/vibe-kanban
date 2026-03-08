use anyhow::{Context, Result, anyhow, bail, ensure};
use async_trait::async_trait;
use db::models::{feishu_bot::FeishuBot, feishu_bot_target::FeishuBotTarget};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::feishu::types::{
    DiscoveredFeishuTarget, ResolvedFeishuSecrets, SendFeishuMessageResult,
};

const FEISHU_API_BASE: &str = "https://open.feishu.cn";
const FEISHU_ANNOUNCEMENT_ROOT_BLOCK_TYPE: i32 = 2;

#[async_trait]
pub trait FeishuClient: Send + Sync {
    async fn validate_bot(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
    ) -> Result<Vec<DiscoveredFeishuTarget>>;

    async fn discover_targets(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
    ) -> Result<Vec<DiscoveredFeishuTarget>>;

    async fn send_message(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        target: &FeishuBotTarget,
        message: &str,
    ) -> Result<SendFeishuMessageResult>;

    async fn add_message_reaction(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        message_id: &str,
        emoji_type: &str,
    ) -> Result<()>;

    async fn upsert_status_announcement(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        chat_id: &str,
        content: &str,
    ) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct HttpFeishuClient {
    http: Client,
    api_base: String,
}

impl Default for HttpFeishuClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFeishuClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            api_base: FEISHU_API_BASE.to_string(),
        }
    }

    #[cfg(test)]
    pub fn with_api_base(api_base: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_base: api_base.into(),
        }
    }

    async fn tenant_access_token(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
    ) -> Result<String> {
        let endpoint = match bot.tenant_mode.as_str() {
            "self_built" | "internal" => "auth/v3/tenant_access_token/internal",
            other => bail!("Unsupported Feishu tenant mode: {other}"),
        };

        let response = self
            .http
            .post(format!("{}/open-apis/{}", self.api_base, endpoint))
            .json(&serde_json::json!({
                "app_id": bot.app_id,
                "app_secret": secrets.app_secret.expose_secret(),
            }))
            .send()
            .await
            .context("Failed to request Feishu tenant access token")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu token request failed with status {}: {}",
            status,
            body
        );

        let payload: AccessTokenResponse =
            serde_json::from_str(&body).context("Invalid Feishu token response")?;
        ensure!(
            payload.code == 0,
            "Feishu token request failed: {}",
            payload.msg
        );

        payload
            .tenant_access_token
            .ok_or_else(|| anyhow!("Feishu token response missing tenant_access_token"))
    }

    async fn list_chats(&self, access_token: &str) -> Result<Vec<DiscoveredFeishuTarget>> {
        let response = self
            .http
            .get(format!("{}/open-apis/im/v1/chats", self.api_base))
            .bearer_auth(access_token)
            .query(&[("page_size", "100")])
            .send()
            .await
            .context("Failed to list Feishu chats")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu chat list request failed with status {}: {}",
            status,
            body
        );

        let payload: ListChatsResponse =
            serde_json::from_str(&body).context("Invalid Feishu chat list response")?;
        ensure!(
            payload.code == 0,
            "Feishu chat list failed: {}",
            payload.msg
        );

        Ok(payload
            .data
            .map(|data| {
                data.items
                    .into_iter()
                    .map(|chat| DiscoveredFeishuTarget {
                        target_type: "chat".to_string(),
                        open_chat_id: chat.open_chat_id,
                        chat_id: chat.chat_id,
                        name: chat.name.unwrap_or_else(|| "Unnamed chat".to_string()),
                        source: "discovery".to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn list_announcement_blocks(
        &self,
        access_token: &str,
        chat_id: &str,
    ) -> Result<Vec<Value>> {
        let response = self
            .http
            .get(format!(
                "{}/open-apis/docx/v1/chats/{chat_id}/announcement/blocks/{chat_id}/children",
                self.api_base
            ))
            .bearer_auth(access_token)
            .query(&[("revision_id", "-1"), ("page_size", "200")])
            .send()
            .await
            .context("Failed to list Feishu announcement blocks")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu announcement block list request failed with status {}: {}",
            status,
            body
        );

        let payload: FeishuApiResponse<AnnouncementBlocksData> = serde_json::from_str(&body)
            .context("Invalid Feishu announcement block list response")?;
        ensure!(
            payload.code == 0,
            "Feishu announcement block list failed: {}",
            payload.msg
        );

        Ok(payload.data.map(|data| data.items).unwrap_or_default())
    }

    async fn create_announcement_block(
        &self,
        access_token: &str,
        chat_id: &str,
        content: &str,
    ) -> Result<()> {
        let response = self
            .http
            .post(format!(
                "{}/open-apis/docx/v1/chats/{chat_id}/announcement/blocks/{chat_id}/children",
                self.api_base
            ))
            .bearer_auth(access_token)
            .query(&[
                ("revision_id", "-1"),
                ("client_token", &Uuid::new_v4().to_string()),
            ])
            .json(&CreateAnnouncementChildrenRequest {
                index: 0,
                children: vec![AnnouncementBlock {
                    block_type: FEISHU_ANNOUNCEMENT_ROOT_BLOCK_TYPE,
                    text: announcement_text_payload(content),
                }],
            })
            .send()
            .await
            .context("Failed to create Feishu announcement block")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu announcement block create request failed with status {}: {}",
            status,
            body
        );

        let payload: FeishuApiResponse<Value> = serde_json::from_str(&body)
            .context("Invalid Feishu announcement block create response")?;
        ensure!(
            payload.code == 0,
            "Feishu announcement block create failed: {}",
            payload.msg
        );

        Ok(())
    }

    async fn update_announcement_blocks(
        &self,
        access_token: &str,
        chat_id: &str,
        updates: &[(&str, &str)],
    ) -> Result<()> {
        let response = self
            .http
            .patch(format!(
                "{}/open-apis/docx/v1/chats/{chat_id}/announcement/blocks/batch_update",
                self.api_base
            ))
            .bearer_auth(access_token)
            .query(&[
                ("revision_id", "-1"),
                ("client_token", &Uuid::new_v4().to_string()),
            ])
            .json(&BatchUpdateAnnouncementBlocksRequest {
                requests: updates
                    .iter()
                    .map(|(block_id, content)| UpdateAnnouncementBlockRequest {
                        block_id: (*block_id).to_string(),
                        update_text_elements: announcement_text_payload(content),
                    })
                    .collect(),
            })
            .send()
            .await
            .context("Failed to update Feishu announcement block")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu announcement block update request failed with status {}: {}",
            status,
            body
        );

        let payload: FeishuApiResponse<Value> = serde_json::from_str(&body)
            .context("Invalid Feishu announcement block update response")?;
        ensure!(
            payload.code == 0,
            "Feishu announcement block update failed: {}",
            payload.msg
        );

        Ok(())
    }
}

#[async_trait]
impl FeishuClient for HttpFeishuClient {
    async fn validate_bot(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
    ) -> Result<Vec<DiscoveredFeishuTarget>> {
        let token = self.tenant_access_token(bot, secrets).await?;
        self.list_chats(&token).await
    }

    async fn discover_targets(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
    ) -> Result<Vec<DiscoveredFeishuTarget>> {
        let token = self.tenant_access_token(bot, secrets).await?;
        self.list_chats(&token).await
    }

    async fn send_message(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        target: &FeishuBotTarget,
        message: &str,
    ) -> Result<SendFeishuMessageResult> {
        let token = self.tenant_access_token(bot, secrets).await?;

        let (receive_id_type, receive_id) = if let Some(open_chat_id) = &target.open_chat_id {
            ("open_chat_id", open_chat_id.clone())
        } else if let Some(chat_id) = &target.chat_id {
            ("chat_id", chat_id.clone())
        } else {
            bail!("Feishu target is missing both open_chat_id and chat_id");
        };

        let response = self
            .http
            .post(format!(
                "{}/open-apis/im/v1/messages?receive_id_type={receive_id_type}",
                self.api_base
            ))
            .bearer_auth(token)
            .json(&SendMessageRequest {
                receive_id,
                msg_type: "text".to_string(),
                content: serde_json::to_string(&TextMessageContent {
                    text: message.to_string(),
                })?,
            })
            .send()
            .await
            .context("Failed to send Feishu message")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu send message failed with status {}: {}",
            status,
            body
        );

        let payload: SendMessageResponse =
            serde_json::from_str(&body).context("Invalid Feishu send message response")?;
        ensure!(
            payload.code == 0,
            "Feishu send message failed: {}",
            payload.msg
        );

        Ok(SendFeishuMessageResult {
            message_id: payload.data.and_then(|data| data.message_id),
        })
    }

    async fn add_message_reaction(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        message_id: &str,
        emoji_type: &str,
    ) -> Result<()> {
        let token = self.tenant_access_token(bot, secrets).await?;

        let response = self
            .http
            .post(format!(
                "{}/open-apis/im/v1/messages/{message_id}/reactions",
                self.api_base
            ))
            .bearer_auth(token)
            .json(&CreateMessageReactionRequest {
                reaction_type: MessageReactionType {
                    emoji_type: emoji_type.to_string(),
                },
            })
            .send()
            .await
            .context("Failed to add Feishu message reaction")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ensure!(
            status.is_success(),
            "Feishu add message reaction failed with status {}: {}",
            status,
            body
        );

        let payload: FeishuApiResponse<Value> =
            serde_json::from_str(&body).context("Invalid Feishu add message reaction response")?;
        ensure!(
            payload.code == 0,
            "Feishu add message reaction failed: {}",
            payload.msg
        );

        Ok(())
    }

    async fn upsert_status_announcement(
        &self,
        bot: &FeishuBot,
        secrets: &ResolvedFeishuSecrets,
        chat_id: &str,
        content: &str,
    ) -> Result<()> {
        let token = self.tenant_access_token(bot, secrets).await?;
        let blocks = self.list_announcement_blocks(&token, chat_id).await?;
        let text_block_ids = find_text_announcement_block_ids(&blocks);

        if let Some((primary_block_id, stale_block_ids)) = text_block_ids.split_first() {
            let mut updates = Vec::with_capacity(1 + stale_block_ids.len());
            updates.push((primary_block_id.as_str(), content));
            for stale_block_id in stale_block_ids {
                updates.push((stale_block_id.as_str(), ""));
            }

            self.update_announcement_blocks(&token, chat_id, &updates)
                .await?;
        } else {
            self.create_announcement_block(&token, chat_id, content)
                .await?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct FeishuApiResponse<T> {
    code: i32,
    msg: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListChatsResponse {
    code: i32,
    msg: String,
    data: Option<ListChatsData>,
}

#[derive(Debug, Deserialize)]
struct ListChatsData {
    items: Vec<FeishuChat>,
}

#[derive(Debug, Deserialize)]
struct FeishuChat {
    chat_id: Option<String>,
    open_chat_id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    receive_id: String,
    msg_type: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct TextMessageContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    code: i32,
    msg: String,
    data: Option<SendMessageData>,
}

#[derive(Debug, Deserialize)]
struct SendMessageData {
    message_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateMessageReactionRequest {
    reaction_type: MessageReactionType,
}

#[derive(Debug, Serialize)]
struct MessageReactionType {
    emoji_type: String,
}

#[derive(Debug, Deserialize)]
struct AnnouncementBlocksData {
    items: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct CreateAnnouncementChildrenRequest {
    index: i32,
    children: Vec<AnnouncementBlock>,
}

#[derive(Debug, Serialize)]
struct AnnouncementBlock {
    block_type: i32,
    text: AnnouncementTextPayload,
}

#[derive(Debug, Serialize)]
struct BatchUpdateAnnouncementBlocksRequest {
    requests: Vec<UpdateAnnouncementBlockRequest>,
}

#[derive(Debug, Serialize)]
struct UpdateAnnouncementBlockRequest {
    block_id: String,
    update_text_elements: AnnouncementTextPayload,
}

#[derive(Debug, Serialize)]
struct AnnouncementTextPayload {
    elements: Vec<AnnouncementTextElement>,
}

#[derive(Debug, Serialize)]
struct AnnouncementTextElement {
    text_run: AnnouncementTextRun,
}

#[derive(Debug, Serialize)]
struct AnnouncementTextRun {
    content: String,
    text_element_style: AnnouncementTextElementStyle,
}

#[derive(Debug, Default, Serialize)]
struct AnnouncementTextElementStyle {}

fn announcement_text_payload(content: &str) -> AnnouncementTextPayload {
    AnnouncementTextPayload {
        elements: vec![AnnouncementTextElement {
            text_run: AnnouncementTextRun {
                content: content.to_string(),
                text_element_style: AnnouncementTextElementStyle::default(),
            },
        }],
    }
}

fn find_text_announcement_block_ids(blocks: &[Value]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            let block_id = block.get("block_id")?.as_str()?;
            if block.get("text").is_some() {
                Some(block_id.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::find_text_announcement_block_ids;

    #[test]
    fn finds_all_text_announcement_blocks_in_order() {
        let blocks = vec![
            json!({
                "block_id": "idle-block",
                "text": { "elements": [{ "text_run": { "content": "Vibe Kanban | Idle | Workspace: Demo" } }] }
            }),
            json!({
                "block_id": "running-block",
                "text": { "elements": [{ "text_run": { "content": "Vibe Kanban | Running | Workspace: Demo" } }] }
            }),
            json!({
                "block_id": "human-block",
                "text": { "elements": [{ "text_run": { "content": "开始了" } }] }
            }),
            json!({
                "block_id": "image-block",
                "block_type": 27
            }),
        ];

        assert_eq!(
            find_text_announcement_block_ids(&blocks),
            vec![
                "idle-block".to_string(),
                "running-block".to_string(),
                "human-block".to_string()
            ]
        );
    }
}
