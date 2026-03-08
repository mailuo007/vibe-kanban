use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use db::models::{
    feishu_bot::{CreateFeishuBot, FeishuBot, UpdateFeishuBot},
    feishu_bot_target::{CreateFeishuBotTarget, FeishuBotTarget, UpdateFeishuBotTarget},
    feishu_conversation::{CreateFeishuConversation, FeishuConversation},
    feishu_delivery_log::{CreateFeishuDeliveryLog, FeishuDeliveryLog},
    session::{CreateSession, Session},
    workspace::Workspace,
    workspace_feishu_binding::{
        CreateWorkspaceFeishuBinding, UpdateWorkspaceFeishuBinding, WorkspaceFeishuBinding,
    },
};
use deployment::Deployment;
use secrecy::ExposeSecret;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::feishu::{
    client::FeishuClient,
    secret_store::{FeishuSecretStore, FileFeishuSecretStore},
    types::{
        CreateFeishuBotInput, DiscoveredFeishuTarget, FeishuBindingDefaults,
        FeishuValidationResult, ResolvedFeishuSecrets, SendFeishuMessageResult,
        UpdateFeishuBotInput,
    },
};

#[derive(Clone)]
pub struct FeishuService<S: FeishuSecretStore> {
    pool: SqlitePool,
    secret_store: Arc<S>,
}

impl<S: FeishuSecretStore> FeishuService<S> {
    pub fn new(pool: SqlitePool, secret_store: Arc<S>) -> Self {
        Self { pool, secret_store }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn binding_defaults(&self) -> FeishuBindingDefaults {
        let _ = &self.pool;
        FeishuBindingDefaults::default()
    }

    pub async fn list_bots(&self) -> Result<Vec<FeishuBot>> {
        Ok(FeishuBot::list(&self.pool).await?)
    }

    pub async fn get_bot(&self, bot_id: Uuid) -> Result<Option<FeishuBot>> {
        Ok(FeishuBot::find_by_id(&self.pool, bot_id).await?)
    }

    pub async fn create_bot(&self, input: CreateFeishuBotInput) -> Result<FeishuBot> {
        self.validate_create_bot_input(&input)?;

        let app_secret_ref = self
            .secret_store
            .put(input.app_secret.context("Feishu app secret is required")?)
            .await?;

        let encrypt_key_ref = match input.encrypt_key {
            Some(value) => Some(self.secret_store.put(value).await?),
            None => None,
        };

        let verification_token_ref = match input.verification_token {
            Some(value) => Some(self.secret_store.put(value).await?),
            None => None,
        };

        Ok(FeishuBot::create(
            &self.pool,
            &CreateFeishuBot {
                name: input.name,
                app_id: input.app_id,
                app_secret_ref,
                encrypt_key_ref,
                verification_token_ref,
                tenant_mode: input.tenant_mode,
            },
        )
        .await?)
    }

    pub async fn update_bot(&self, bot_id: Uuid, input: UpdateFeishuBotInput) -> Result<FeishuBot> {
        ensure!(!input.name.trim().is_empty(), "Feishu bot name is required");
        ensure!(!input.app_id.trim().is_empty(), "Feishu app id is required");
        ensure!(
            !input.tenant_mode.trim().is_empty(),
            "Feishu tenant mode is required"
        );

        let current = FeishuBot::find_by_id(&self.pool, bot_id)
            .await?
            .context("Feishu bot not found")?;

        let app_secret_ref = match input.app_secret {
            Some(secret) => {
                ensure!(
                    !secret.expose_secret().trim().is_empty(),
                    "Feishu app secret is required"
                );
                self.secret_store.put(secret).await?
            }
            None => current.app_secret_ref.clone(),
        };

        let encrypt_key_ref = match input.encrypt_key {
            Some(secret) => {
                ensure!(
                    !secret.expose_secret().trim().is_empty(),
                    "Feishu encrypt key cannot be empty"
                );
                Some(self.secret_store.put(secret).await?)
            }
            None => current.encrypt_key_ref.clone(),
        };

        let verification_token_ref = match input.verification_token {
            Some(secret) => {
                ensure!(
                    !secret.expose_secret().trim().is_empty(),
                    "Feishu verification token cannot be empty"
                );
                Some(self.secret_store.put(secret).await?)
            }
            None => current.verification_token_ref.clone(),
        };

        Ok(FeishuBot::update(
            &self.pool,
            bot_id,
            &UpdateFeishuBot {
                name: input.name,
                app_id: input.app_id,
                app_secret_ref,
                encrypt_key_ref,
                verification_token_ref,
                tenant_mode: input.tenant_mode,
                enabled: input.enabled,
            },
        )
        .await?)
    }

    pub async fn delete_bot(&self, bot_id: Uuid) -> Result<u64> {
        Ok(FeishuBot::delete(&self.pool, bot_id).await?)
    }

    pub async fn resolve_bot_secrets(&self, bot: &FeishuBot) -> Result<ResolvedFeishuSecrets> {
        let app_secret = self.secret_store.get(&bot.app_secret_ref).await?;
        let encrypt_key = match &bot.encrypt_key_ref {
            Some(reference) => Some(self.secret_store.get(reference).await?),
            None => None,
        };
        let verification_token = match &bot.verification_token_ref {
            Some(reference) => Some(self.secret_store.get(reference).await?),
            None => None,
        };

        Ok(ResolvedFeishuSecrets {
            app_secret,
            encrypt_key,
            verification_token,
        })
    }

    fn validate_create_bot_input(&self, input: &CreateFeishuBotInput) -> Result<()> {
        ensure!(!input.name.trim().is_empty(), "Feishu bot name is required");
        ensure!(!input.app_id.trim().is_empty(), "Feishu app id is required");
        ensure!(
            !input.tenant_mode.trim().is_empty(),
            "Feishu tenant mode is required"
        );

        let app_secret = input
            .app_secret
            .as_ref()
            .context("Feishu app secret is required")?;
        ensure!(
            !app_secret.expose_secret().trim().is_empty(),
            "Feishu app secret is required"
        );

        if let Some(encrypt_key) = input.encrypt_key.as_ref() {
            ensure!(
                !encrypt_key.expose_secret().trim().is_empty(),
                "Feishu encrypt key cannot be empty"
            );
        }

        if let Some(verification_token) = input.verification_token.as_ref() {
            ensure!(
                !verification_token.expose_secret().trim().is_empty(),
                "Feishu verification token cannot be empty"
            );
        }

        Ok(())
    }

    pub async fn validate_bot<C: FeishuClient>(
        &self,
        client: &C,
        bot_id: Uuid,
    ) -> Result<FeishuValidationResult> {
        let bot = self
            .get_bot(bot_id)
            .await?
            .context("Feishu bot not found")?;
        let secrets = self.resolve_bot_secrets(&bot).await?;
        let targets = client.validate_bot(&bot, &secrets).await?;
        let status = FeishuValidationResult {
            ok: true,
            message: "Bot validated successfully".to_string(),
            chat_count: targets.len(),
        };
        let _ = FeishuBot::update_health(&self.pool, bot.id, "healthy", None).await;
        Ok(status)
    }

    pub async fn list_targets(&self, bot_id: Uuid) -> Result<Vec<FeishuBotTarget>> {
        Ok(FeishuBotTarget::list_by_bot_id(&self.pool, bot_id).await?)
    }

    pub async fn find_target_by_identity(
        &self,
        bot_id: Uuid,
        open_chat_id: Option<&str>,
        chat_id: Option<&str>,
    ) -> Result<Option<FeishuBotTarget>> {
        Ok(FeishuBotTarget::find_by_identity(&self.pool, bot_id, open_chat_id, chat_id).await?)
    }

    pub async fn refresh_targets<C: FeishuClient>(
        &self,
        client: &C,
        bot_id: Uuid,
    ) -> Result<Vec<FeishuBotTarget>> {
        let bot = self
            .get_bot(bot_id)
            .await?
            .context("Feishu bot not found")?;
        let secrets = self.resolve_bot_secrets(&bot).await?;
        let discovered = client.discover_targets(&bot, &secrets).await?;
        self.sync_discovered_targets(bot_id, &discovered).await?;
        self.list_targets(bot_id).await
    }

    pub async fn list_bindings(&self, workspace_id: Uuid) -> Result<Vec<WorkspaceFeishuBinding>> {
        Ok(WorkspaceFeishuBinding::list_by_workspace_id(&self.pool, workspace_id).await?)
    }

    pub async fn list_bindings_for_target(
        &self,
        target_id: Uuid,
    ) -> Result<Vec<WorkspaceFeishuBinding>> {
        Ok(WorkspaceFeishuBinding::list_by_target_id(&self.pool, target_id).await?)
    }

    async fn ensure_chat_group_is_available(
        &self,
        target_id: Uuid,
        ignored_binding_id: Option<Uuid>,
    ) -> Result<()> {
        let target = FeishuBotTarget::find_by_id(&self.pool, target_id)
            .await?
            .context("Feishu target not found")?;
        let conflicting_binding = WorkspaceFeishuBinding::list_enabled_by_chat_identity(
            &self.pool,
            target.open_chat_id.as_deref(),
            target.chat_id.as_deref(),
        )
        .await?
        .into_iter()
        .find(|binding| Some(binding.id) != ignored_binding_id);

        if let Some(binding) = conflicting_binding {
            let workspace_label = Workspace::find_by_id(&self.pool, binding.workspace_id)
                .await?
                .map(|workspace| workspace.name.unwrap_or(workspace.branch))
                .unwrap_or_else(|| binding.workspace_id.to_string());
            bail!(
                "Current Feishu group \"{}\" is already bound to workspace \"{}\". Unbind it before binding again.",
                target.name,
                workspace_label
            );
        }

        Ok(())
    }

    pub async fn ensure_single_active_binding_for_target(
        &self,
        bot_id: Uuid,
        target_id: Uuid,
        preferred_binding_id: Uuid,
    ) -> Result<Vec<WorkspaceFeishuBinding>> {
        let bindings = WorkspaceFeishuBinding::list_by_target_id(&self.pool, target_id).await?;
        let mut retained = Vec::new();

        for binding in bindings {
            if binding.bot_id != bot_id {
                continue;
            }

            if binding.id != preferred_binding_id && binding.enabled {
                WorkspaceFeishuBinding::update(
                    &self.pool,
                    binding.id,
                    &UpdateWorkspaceFeishuBinding {
                        enabled: false,
                        notify_on_status: binding.notify_on_status,
                        notify_on_agent_reply: binding.notify_on_agent_reply,
                        notify_on_pr: binding.notify_on_pr,
                        allow_commands: binding.allow_commands,
                        allow_chat_messages: binding.allow_chat_messages,
                        allow_cards: binding.allow_cards,
                        ack_reaction_enabled: binding.ack_reaction_enabled,
                        ack_reaction_emoji_type: binding.ack_reaction_emoji_type.clone(),
                        sync_group_announcement: binding.sync_group_announcement,
                    },
                )
                .await?;
                continue;
            }

            retained.push(binding);
        }

        Ok(retained)
    }

    pub async fn create_binding(
        &self,
        workspace_id: Uuid,
        bot_id: Uuid,
        target_id: Uuid,
    ) -> Result<WorkspaceFeishuBinding> {
        self.ensure_chat_group_is_available(target_id, None).await?;
        let binding = WorkspaceFeishuBinding::create(
            &self.pool,
            &CreateWorkspaceFeishuBinding {
                workspace_id,
                bot_id,
                target_id,
            },
        )
        .await?;
        Ok(binding)
    }

    pub async fn update_binding(
        &self,
        binding_id: Uuid,
        update: UpdateWorkspaceFeishuBinding,
    ) -> Result<WorkspaceFeishuBinding> {
        let existing = WorkspaceFeishuBinding::find_by_id(&self.pool, binding_id)
            .await?
            .context("Feishu binding not found")?;
        if update.enabled {
            self.ensure_chat_group_is_available(existing.target_id, Some(existing.id))
                .await?;
        }

        WorkspaceFeishuBinding::update(&self.pool, binding_id, &update)
            .await
            .map_err(Into::into)
    }

    pub async fn delete_binding(&self, binding_id: Uuid) -> Result<u64> {
        Ok(WorkspaceFeishuBinding::delete(&self.pool, binding_id).await?)
    }

    pub async fn send_message_to_target<C: FeishuClient>(
        &self,
        client: &C,
        bot_id: Uuid,
        target_id: Uuid,
        workspace_id: Option<Uuid>,
        event_type: &str,
        message: &str,
    ) -> Result<SendFeishuMessageResult> {
        let bot = self
            .get_bot(bot_id)
            .await?
            .context("Feishu bot not found")?;
        let target = FeishuBotTarget::find_by_id(&self.pool, target_id)
            .await?
            .context("Feishu target not found")?;
        let secrets = self.resolve_bot_secrets(&bot).await?;
        match client.send_message(&bot, &secrets, &target, message).await {
            Ok(result) => {
                let _ = FeishuDeliveryLog::create(
                    &self.pool,
                    &CreateFeishuDeliveryLog {
                        workspace_id,
                        bot_id,
                        target_id,
                        event_type: event_type.to_string(),
                        payload_summary: Some(message.to_string()),
                        status: "sent".to_string(),
                        retry_count: 0,
                        message_id: result.message_id.clone(),
                        error_message: None,
                    },
                )
                .await;

                Ok(result)
            }
            Err(error) => {
                let error_message = error.to_string();

                let _ = FeishuDeliveryLog::create(
                    &self.pool,
                    &CreateFeishuDeliveryLog {
                        workspace_id,
                        bot_id,
                        target_id,
                        event_type: event_type.to_string(),
                        payload_summary: Some(message.to_string()),
                        status: "failed".to_string(),
                        retry_count: 0,
                        message_id: None,
                        error_message: Some(error_message),
                    },
                )
                .await;

                Err(error)
            }
        }
    }

    pub async fn add_message_reaction<C: FeishuClient>(
        &self,
        client: &C,
        bot_id: Uuid,
        message_id: &str,
        emoji_type: &str,
    ) -> Result<()> {
        let bot = self
            .get_bot(bot_id)
            .await?
            .context("Feishu bot not found")?;
        let secrets = self.resolve_bot_secrets(&bot).await?;

        client
            .add_message_reaction(&bot, &secrets, message_id, emoji_type)
            .await
    }

    pub async fn upsert_status_announcement<C: FeishuClient>(
        &self,
        client: &C,
        bot_id: Uuid,
        target_id: Uuid,
        content: &str,
    ) -> Result<()> {
        let bot = self
            .get_bot(bot_id)
            .await?
            .context("Feishu bot not found")?;
        let target = FeishuBotTarget::find_by_id(&self.pool, target_id)
            .await?
            .context("Feishu target not found")?;
        let chat_id = target
            .chat_id
            .as_deref()
            .context("Feishu target is missing chat_id for group announcement")?;
        let secrets = self.resolve_bot_secrets(&bot).await?;

        client
            .upsert_status_announcement(&bot, &secrets, chat_id, content)
            .await
    }

    pub async fn ensure_chat_session_for_binding(
        &self,
        binding: &WorkspaceFeishuBinding,
    ) -> Result<FeishuConversation> {
        let existing = FeishuConversation::find_by_binding(
            &self.pool,
            binding.workspace_id,
            binding.bot_id,
            binding.target_id,
        )
        .await?;

        let conversation = match existing {
            Some(conversation) => conversation,
            None => {
                let session_id = Uuid::new_v4();
                Session::create(
                    &self.pool,
                    &CreateSession { executor: None },
                    session_id,
                    binding.workspace_id,
                )
                .await?;

                match FeishuConversation::create(
                    &self.pool,
                    &CreateFeishuConversation {
                        bot_id: binding.bot_id,
                        target_id: binding.target_id,
                        workspace_id: binding.workspace_id,
                        session_id: Some(session_id),
                        feishu_user_id: None,
                        last_message_at: None,
                        last_card_context: None,
                    },
                )
                .await
                {
                    Ok(conversation) => conversation,
                    Err(error) if error.to_string().contains("UNIQUE constraint failed") => {
                        FeishuConversation::find_by_binding(
                            &self.pool,
                            binding.workspace_id,
                            binding.bot_id,
                            binding.target_id,
                        )
                        .await?
                        .context("Feishu conversation not found after unique constraint")?
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };

        let session_id = conversation.session_id.unwrap_or_else(Uuid::new_v4);
        if Session::find_by_id(&self.pool, session_id).await?.is_none() {
            Session::create(
                &self.pool,
                &CreateSession { executor: None },
                session_id,
                binding.workspace_id,
            )
            .await?;
        }

        let conversation = if conversation.session_id == Some(session_id) {
            conversation
        } else {
            FeishuConversation::update_session_id(&self.pool, conversation.id, session_id).await?
        };

        Ok(FeishuConversation::touch_last_message_at(&self.pool, conversation.id).await?)
    }

    pub async fn list_conversations_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<FeishuConversation>> {
        Ok(FeishuConversation::list_by_session_id(&self.pool, session_id).await?)
    }

    async fn sync_discovered_targets(
        &self,
        bot_id: Uuid,
        discovered: &[DiscoveredFeishuTarget],
    ) -> Result<()> {
        let existing = FeishuBotTarget::list_by_bot_id(&self.pool, bot_id).await?;
        let mut retained_ids = std::collections::HashSet::new();

        for target in discovered {
            let existing_target = FeishuBotTarget::find_by_identity(
                &self.pool,
                bot_id,
                target.open_chat_id.as_deref(),
                target.chat_id.as_deref(),
            )
            .await?;

            let record = if let Some(existing_target) = existing_target {
                FeishuBotTarget::update(
                    &self.pool,
                    existing_target.id,
                    &UpdateFeishuBotTarget {
                        target_type: target.target_type.clone(),
                        open_chat_id: target.open_chat_id.clone(),
                        chat_id: target.chat_id.clone(),
                        name: target.name.clone(),
                        source: target.source.clone(),
                        is_active: true,
                    },
                )
                .await?
            } else {
                FeishuBotTarget::create(
                    &self.pool,
                    &CreateFeishuBotTarget {
                        bot_id,
                        target_type: target.target_type.clone(),
                        open_chat_id: target.open_chat_id.clone(),
                        chat_id: target.chat_id.clone(),
                        name: target.name.clone(),
                        source: target.source.clone(),
                    },
                )
                .await?
            };

            retained_ids.insert(record.id);
        }

        for target in existing {
            if !retained_ids.contains(&target.id) && target.is_active {
                let _ = FeishuBotTarget::update(
                    &self.pool,
                    target.id,
                    &UpdateFeishuBotTarget {
                        target_type: target.target_type.clone(),
                        open_chat_id: target.open_chat_id.clone(),
                        chat_id: target.chat_id.clone(),
                        name: target.name.clone(),
                        source: target.source.clone(),
                        is_active: false,
                    },
                )
                .await;
            }
        }

        Ok(())
    }
}

impl FeishuService<FileFeishuSecretStore> {
    pub fn from_deployment<D: Deployment>(deployment: &D) -> Self {
        let secret_store = Arc::new(FileFeishuSecretStore::new(
            deployment.feishu_secret_store_path(),
        ));
        Self::new(deployment.db().pool.clone(), secret_store)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr, sync::Arc};

    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use db::models::workspace_feishu_binding::{
        CreateWorkspaceFeishuBinding, UpdateWorkspaceFeishuBinding, WorkspaceFeishuBinding,
    };
    use secrecy::{ExposeSecret, SecretString};
    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::FeishuService;
    use crate::feishu::{secret_store::FeishuSecretStore, types::CreateFeishuBotInput};

    #[derive(Debug, Default)]
    struct MemoryFeishuSecretStore {
        values: Mutex<HashMap<String, SecretString>>,
    }

    #[async_trait]
    impl FeishuSecretStore for MemoryFeishuSecretStore {
        async fn put(&self, value: SecretString) -> Result<String> {
            let reference = format!("memory:{}", uuid::Uuid::new_v4());
            self.values.lock().await.insert(reference.clone(), value);
            Ok(reference)
        }

        async fn get(&self, reference: &str) -> Result<SecretString> {
            self.values
                .lock()
                .await
                .get(reference)
                .cloned()
                .ok_or_else(|| anyhow!("missing secret"))
        }
    }

    async fn test_pool() -> SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("memory sqlite url")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect memory db");

        sqlx::migrate!("../db/migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        pool
    }

    #[tokio::test]
    async fn stores_secret_references_instead_of_inline_values() {
        let pool = test_pool().await;
        let service = FeishuService::new(pool, Arc::new(MemoryFeishuSecretStore::default()));
        let input = CreateFeishuBotInput {
            name: "engineering-bot".into(),
            app_id: "cli_123".into(),
            app_secret: Some(SecretString::new("app-secret".into())),
            encrypt_key: Some(SecretString::new("encrypt-key".into())),
            verification_token: Some(SecretString::new("verify-token".into())),
            tenant_mode: "self_built".into(),
        };

        let bot = service.create_bot(input).await.expect("create bot");
        let secrets = service
            .resolve_bot_secrets(&bot)
            .await
            .expect("resolve bot secrets");

        assert_ne!(bot.app_secret_ref, "app-secret");
        assert_eq!(secrets.app_secret.expose_secret(), "app-secret");
        assert_eq!(
            secrets.encrypt_key.expect("encrypt key").expose_secret(),
            "encrypt-key"
        );
        assert_eq!(
            secrets
                .verification_token
                .expect("verification token")
                .expose_secret(),
            "verify-token"
        );
    }

    #[tokio::test]
    async fn rejects_incomplete_bot_credentials() {
        let pool = test_pool().await;
        let service = FeishuService::new(pool, Arc::new(MemoryFeishuSecretStore::default()));
        let err = service
            .create_bot(CreateFeishuBotInput {
                name: "broken-bot".into(),
                app_id: "cli_missing_secret".into(),
                app_secret: None,
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect_err("missing secret should fail");

        assert!(err.to_string().contains("app secret"));
    }

    #[tokio::test]
    async fn binding_defaults_are_safe() {
        let pool = test_pool().await;
        let service = FeishuService::new(pool, Arc::new(MemoryFeishuSecretStore::default()));
        let defaults = service.binding_defaults();

        assert!(defaults.enabled);
        assert!(defaults.notify_on_status);
        assert!(defaults.notify_on_agent_reply);
        assert!(defaults.notify_on_pr);
        assert!(!defaults.allow_commands);
        assert!(!defaults.allow_chat_messages);
        assert!(defaults.allow_cards);
        assert!(defaults.ack_reaction_enabled);
        assert_eq!(defaults.ack_reaction_emoji_type, "Typing");
        assert!(defaults.sync_group_announcement);
    }

    async fn create_workspace(pool: &SqlitePool, name: &str) -> db::models::workspace::Workspace {
        db::models::workspace::Workspace::create(
            pool,
            &db::models::workspace::CreateWorkspace {
                branch: format!("branch-{name}"),
                name: Some(name.to_string()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create workspace")
    }

    async fn create_target(
        pool: &SqlitePool,
        bot_id: Uuid,
        chat_id: &str,
    ) -> db::models::feishu_bot_target::FeishuBotTarget {
        db::models::feishu_bot_target::FeishuBotTarget::create(
            pool,
            &db::models::feishu_bot_target::CreateFeishuBotTarget {
                bot_id,
                target_type: "chat".into(),
                open_chat_id: None,
                chat_id: Some(chat_id.to_string()),
                name: chat_id.to_string(),
                source: "test".into(),
            },
        )
        .await
        .expect("create target")
    }

    #[tokio::test]
    async fn ensure_chat_session_reuses_existing_fixed_session() {
        let pool = test_pool().await;
        let service =
            FeishuService::new(pool.clone(), Arc::new(MemoryFeishuSecretStore::default()));
        let workspace = create_workspace(&pool, "ws-chat").await;
        let bot = service
            .create_bot(CreateFeishuBotInput {
                name: "chat-bot".into(),
                app_id: "cli_chat_bot".into(),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create bot");
        let target = create_target(&pool, bot.id, "chat-fixed").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");

        let first = service
            .ensure_chat_session_for_binding(&binding)
            .await
            .expect("first session");
        let second = service
            .ensure_chat_session_for_binding(&binding)
            .await
            .expect("second session");

        assert_eq!(first.id, second.id);
        assert_eq!(first.session_id, second.session_id);
        assert_eq!(
            service
                .list_conversations_for_session(first.session_id.expect("session id"))
                .await
                .expect("list by session")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn create_binding_rejects_group_already_bound_to_another_workspace() {
        let pool = test_pool().await;
        let service =
            FeishuService::new(pool.clone(), Arc::new(MemoryFeishuSecretStore::default()));
        let first_workspace = create_workspace(&pool, "ws-first").await;
        let second_workspace = create_workspace(&pool, "ws-second").await;
        let first_bot = service
            .create_bot(CreateFeishuBotInput {
                name: "group-unique-bot-one".into(),
                app_id: "cli_group_unique_bot_one".into(),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create first bot");
        let second_bot = service
            .create_bot(CreateFeishuBotInput {
                name: "group-unique-bot-two".into(),
                app_id: "cli_group_unique_bot_two".into(),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create second bot");
        let first_target = create_target(&pool, first_bot.id, "chat-unique").await;
        let second_target = create_target(&pool, second_bot.id, "chat-unique").await;

        service
            .create_binding(first_workspace.id, first_bot.id, first_target.id)
            .await
            .expect("create first binding");

        let err = service
            .create_binding(second_workspace.id, second_bot.id, second_target.id)
            .await
            .expect_err("second binding should be rejected");

        assert!(err.to_string().contains("already bound"));
        assert!(err.to_string().contains("ws-first"));
    }

    #[tokio::test]
    async fn update_binding_rejects_enabling_group_bound_to_another_workspace() {
        let pool = test_pool().await;
        let service =
            FeishuService::new(pool.clone(), Arc::new(MemoryFeishuSecretStore::default()));
        let first_workspace = create_workspace(&pool, "ws-active").await;
        let second_workspace = create_workspace(&pool, "ws-blocked").await;
        let first_bot = service
            .create_bot(CreateFeishuBotInput {
                name: "enable-check-bot-one".into(),
                app_id: "cli_enable_check_bot_one".into(),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create first bot");
        let second_bot = service
            .create_bot(CreateFeishuBotInput {
                name: "enable-check-bot-two".into(),
                app_id: "cli_enable_check_bot_two".into(),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create second bot");
        let first_target = create_target(&pool, first_bot.id, "chat-enable-unique").await;
        let second_target = create_target(&pool, second_bot.id, "chat-enable-unique").await;

        service
            .create_binding(first_workspace.id, first_bot.id, first_target.id)
            .await
            .expect("create active binding");
        let blocked_binding = WorkspaceFeishuBinding::create(
            &pool,
            &CreateWorkspaceFeishuBinding {
                workspace_id: second_workspace.id,
                bot_id: second_bot.id,
                target_id: second_target.id,
            },
        )
        .await
        .expect("create legacy binding");

        let disabled_binding = service
            .update_binding(
                blocked_binding.id,
                UpdateWorkspaceFeishuBinding {
                    enabled: false,
                    notify_on_status: true,
                    notify_on_agent_reply: true,
                    notify_on_pr: true,
                    allow_commands: false,
                    allow_chat_messages: false,
                    allow_cards: true,
                    ack_reaction_enabled: true,
                    ack_reaction_emoji_type: "Typing".into(),
                    sync_group_announcement: true,
                },
            )
            .await
            .expect("disable legacy binding");
        assert!(!disabled_binding.enabled);

        let err = service
            .update_binding(
                blocked_binding.id,
                UpdateWorkspaceFeishuBinding {
                    enabled: true,
                    notify_on_status: true,
                    notify_on_agent_reply: true,
                    notify_on_pr: true,
                    allow_commands: false,
                    allow_chat_messages: false,
                    allow_cards: true,
                    ack_reaction_enabled: true,
                    ack_reaction_emoji_type: "Typing".into(),
                    sync_group_announcement: true,
                },
            )
            .await
            .expect_err("re-enabling duplicate binding should fail");

        assert!(err.to_string().contains("already bound"));
        assert!(err.to_string().contains("ws-active"));
    }
}
