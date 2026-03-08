use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateFeishuBotInput {
    pub name: String,
    pub app_id: String,
    pub app_secret: Option<SecretString>,
    pub encrypt_key: Option<SecretString>,
    pub verification_token: Option<SecretString>,
    pub tenant_mode: String,
}

#[derive(Debug, Clone)]
pub struct UpdateFeishuBotInput {
    pub name: String,
    pub app_id: String,
    pub app_secret: Option<SecretString>,
    pub encrypt_key: Option<SecretString>,
    pub verification_token: Option<SecretString>,
    pub tenant_mode: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedFeishuSecrets {
    pub app_secret: SecretString,
    pub encrypt_key: Option<SecretString>,
    pub verification_token: Option<SecretString>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DiscoveredFeishuTarget {
    pub target_type: String,
    pub open_chat_id: Option<String>,
    pub chat_id: Option<String>,
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FeishuValidationResult {
    pub ok: bool,
    pub message: String,
    pub chat_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SendFeishuMessageInput {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SendFeishuMessageResult {
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FeishuRuntimeSnapshot {
    pub bot_id: Uuid,
    pub running: bool,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeishuBindingDefaults {
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

impl Default for FeishuBindingDefaults {
    fn default() -> Self {
        Self {
            enabled: true,
            notify_on_status: true,
            notify_on_agent_reply: true,
            notify_on_pr: true,
            allow_commands: false,
            allow_chat_messages: false,
            allow_cards: true,
            ack_reaction_enabled: true,
            ack_reaction_emoji_type: "Typing".to_string(),
            sync_group_announcement: true,
        }
    }
}
