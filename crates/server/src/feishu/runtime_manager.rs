use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use anyhow::Result;
use db::models::feishu_bot::FeishuBot;
use deployment::Deployment;
use sqlx::SqlitePool;
use tokio::{sync::Mutex, task::JoinHandle};
use uuid::Uuid;

use crate::feishu::{
    chat_runner::{DeploymentFeishuChatRunner, FeishuChatRunner, NoopFeishuChatRunner},
    client::{FeishuClient, HttpFeishuClient},
    runtime::{FeishuRuntime, FeishuRuntimeConfig},
    secret_store::FileFeishuSecretStore,
    service::FeishuService,
    types::FeishuRuntimeSnapshot,
};

struct RuntimeEntry {
    generation: u64,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
pub struct FeishuRuntimeManager<S, C>
where
    S: crate::feishu::secret_store::FeishuSecretStore,
    C: FeishuClient,
{
    pool: SqlitePool,
    service: Arc<FeishuService<S>>,
    client: Arc<C>,
    chat_runner: Arc<dyn FeishuChatRunner>,
    runtimes: Arc<Mutex<HashMap<Uuid, RuntimeEntry>>>,
    runtime_config: FeishuRuntimeConfig,
}

impl<S, C> FeishuRuntimeManager<S, C>
where
    S: crate::feishu::secret_store::FeishuSecretStore + 'static,
    C: FeishuClient + 'static,
{
    pub fn new(pool: SqlitePool, service: Arc<FeishuService<S>>, client: Arc<C>) -> Self {
        Self::with_config_and_chat_runner(
            pool,
            service,
            client,
            FeishuRuntimeConfig::default(),
            Arc::new(NoopFeishuChatRunner),
        )
    }

    pub fn with_config(
        pool: SqlitePool,
        service: Arc<FeishuService<S>>,
        client: Arc<C>,
        runtime_config: FeishuRuntimeConfig,
    ) -> Self {
        Self::with_config_and_chat_runner(
            pool,
            service,
            client,
            runtime_config,
            Arc::new(NoopFeishuChatRunner),
        )
    }

    pub fn with_config_and_chat_runner(
        pool: SqlitePool,
        service: Arc<FeishuService<S>>,
        client: Arc<C>,
        runtime_config: FeishuRuntimeConfig,
        chat_runner: Arc<dyn FeishuChatRunner>,
    ) -> Self {
        Self {
            pool,
            service,
            client,
            chat_runner,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            runtime_config,
        }
    }

    pub async fn restore_enabled_bots(&self) -> Result<()> {
        let bots = FeishuBot::find_enabled(&self.pool).await?;
        for bot in bots {
            self.upsert_runtime(bot.id).await?;
        }
        Ok(())
    }

    pub async fn upsert_runtime(&self, bot_id: Uuid) -> Result<()> {
        let bot = FeishuBot::find_by_id(&self.pool, bot_id).await?;
        if bot.as_ref().is_none_or(|bot| !bot.enabled) {
            self.stop_runtime(bot_id).await?;
            return Ok(());
        }

        let previous_generation = {
            let mut runtimes = self.runtimes.lock().await;
            if let Some(existing) = runtimes.remove(&bot_id) {
                existing.handle.abort();
                existing.generation
            } else {
                0
            }
        };

        let runtime = FeishuRuntime::new(
            bot_id,
            self.pool.clone(),
            self.service.clone(),
            self.client.clone(),
            self.chat_runner.clone(),
            self.runtime_config.clone(),
        );
        let handle = tokio::spawn(runtime.run());

        self.runtimes.lock().await.insert(
            bot_id,
            RuntimeEntry {
                generation: previous_generation + 1,
                handle,
            },
        );
        Ok(())
    }

    pub async fn stop_runtime(&self, bot_id: Uuid) -> Result<()> {
        if let Some(existing) = self.runtimes.lock().await.remove(&bot_id) {
            existing.handle.abort();
        }
        Ok(())
    }

    pub async fn snapshots(&self) -> Vec<FeishuRuntimeSnapshot> {
        let runtimes = self.runtimes.lock().await;
        runtimes
            .iter()
            .map(|(bot_id, entry)| FeishuRuntimeSnapshot {
                bot_id: *bot_id,
                running: !entry.handle.is_finished(),
                generation: entry.generation,
            })
            .collect()
    }
}

type GlobalFeishuRuntimeManager = FeishuRuntimeManager<FileFeishuSecretStore, HttpFeishuClient>;

static GLOBAL_RUNTIME_MANAGER: OnceLock<GlobalFeishuRuntimeManager> = OnceLock::new();

pub fn init_global_runtime_manager(
    deployment: &crate::DeploymentImpl,
) -> &'static GlobalFeishuRuntimeManager {
    GLOBAL_RUNTIME_MANAGER.get_or_init(|| {
        let service = Arc::new(FeishuService::from_deployment(deployment));
        FeishuRuntimeManager::with_config_and_chat_runner(
            deployment.db().pool.clone(),
            service,
            Arc::new(HttpFeishuClient::new()),
            FeishuRuntimeConfig::default(),
            Arc::new(DeploymentFeishuChatRunner::new(deployment.clone())),
        )
    })
}

pub fn global_runtime_manager() -> Option<&'static GlobalFeishuRuntimeManager> {
    GLOBAL_RUNTIME_MANAGER.get()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use db::models::feishu_bot::FeishuBot;
    use secrecy::SecretString;
    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use super::FeishuRuntimeManager;
    use crate::feishu::{
        client::FeishuClient,
        runtime::FeishuRuntimeConfig,
        secret_store::FeishuSecretStore,
        service::FeishuService,
        types::{
            CreateFeishuBotInput, DiscoveredFeishuTarget, ResolvedFeishuSecrets,
            SendFeishuMessageResult,
        },
    };

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

    #[derive(Debug, Default)]
    struct FakeFeishuClient {
        results: Mutex<HashMap<Uuid, std::result::Result<Vec<DiscoveredFeishuTarget>, String>>>,
    }

    impl FakeFeishuClient {
        async fn set_ok(&self, bot_id: Uuid) {
            self.results.lock().await.insert(
                bot_id,
                Ok(vec![DiscoveredFeishuTarget {
                    target_type: "chat".into(),
                    open_chat_id: Some(format!("oc_{bot_id}")),
                    chat_id: None,
                    name: "Engineering".into(),
                    source: "test".into(),
                }]),
            );
        }

        async fn set_err(&self, bot_id: Uuid, message: &str) {
            self.results
                .lock()
                .await
                .insert(bot_id, Err(message.to_string()));
        }
    }

    #[async_trait]
    impl FeishuClient for FakeFeishuClient {
        async fn validate_bot(
            &self,
            bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
        ) -> Result<Vec<DiscoveredFeishuTarget>> {
            match self.results.lock().await.get(&bot.id).cloned() {
                Some(Ok(targets)) => Ok(targets),
                Some(Err(message)) => Err(anyhow!(message)),
                None => Ok(vec![]),
            }
        }

        async fn discover_targets(
            &self,
            bot: &db::models::feishu_bot::FeishuBot,
            secrets: &ResolvedFeishuSecrets,
        ) -> Result<Vec<DiscoveredFeishuTarget>> {
            self.validate_bot(bot, secrets).await
        }

        async fn send_message(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            _target: &db::models::feishu_bot_target::FeishuBotTarget,
            _message: &str,
        ) -> Result<SendFeishuMessageResult> {
            Ok(SendFeishuMessageResult {
                message_id: Some("test-message".into()),
            })
        }

        async fn add_message_reaction(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            _message_id: &str,
            _emoji_type: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn upsert_status_announcement(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            _chat_id: &str,
            _content: &str,
        ) -> Result<()> {
            Ok(())
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

    async fn create_bot(
        service: &FeishuService<MemoryFeishuSecretStore>,
        name: &str,
    ) -> db::models::feishu_bot::FeishuBot {
        service
            .create_bot(CreateFeishuBotInput {
                name: name.to_string(),
                app_id: format!("cli_{name}"),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create bot")
    }

    async fn wait_for_condition<F, Fut>(mut check: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        for _ in 0..20 {
            if check().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("condition not satisfied in time");
    }

    #[tokio::test]
    async fn one_enabled_bot_starts_one_runtime() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let bot = create_bot(&service, "bot-one").await;
        let client = Arc::new(FakeFeishuClient::default());
        client.set_ok(bot.id).await;

        let manager = FeishuRuntimeManager::with_config(
            pool.clone(),
            service,
            client,
            FeishuRuntimeConfig {
                heartbeat_interval: Duration::from_millis(50),
                enable_long_connection: false,
                long_connection_retry_interval: Duration::from_millis(50),
            },
        );

        manager
            .restore_enabled_bots()
            .await
            .expect("restore runtimes");
        wait_for_condition(|| async {
            let running = manager
                .snapshots()
                .await
                .iter()
                .any(|snapshot| snapshot.bot_id == bot.id && snapshot.running);
            let healthy = FeishuBot::find_by_id(&pool, bot.id)
                .await
                .expect("fetch bot")
                .is_some_and(|bot| bot.last_health_status == "healthy");
            running && healthy
        })
        .await;

        let refreshed = FeishuBot::find_by_id(&pool, bot.id)
            .await
            .expect("fetch bot")
            .expect("bot exists");
        assert_eq!(refreshed.last_health_status, "healthy");
    }

    #[tokio::test]
    async fn disabling_one_bot_stops_only_that_runtime() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let first = create_bot(&service, "bot-first").await;
        let second = create_bot(&service, "bot-second").await;
        let client = Arc::new(FakeFeishuClient::default());
        client.set_ok(first.id).await;
        client.set_ok(second.id).await;

        let manager = FeishuRuntimeManager::with_config(
            pool.clone(),
            service,
            client,
            FeishuRuntimeConfig {
                heartbeat_interval: Duration::from_millis(50),
                enable_long_connection: false,
                long_connection_retry_interval: Duration::from_millis(50),
            },
        );
        manager
            .restore_enabled_bots()
            .await
            .expect("restore runtimes");
        wait_for_condition(|| async { manager.snapshots().await.len() == 2 }).await;

        FeishuBot::set_enabled(&pool, first.id, false)
            .await
            .expect("disable first bot");
        manager
            .upsert_runtime(first.id)
            .await
            .expect("sync disabled runtime");

        wait_for_condition(|| async {
            let snapshots = manager.snapshots().await;
            snapshots.len() == 1 && snapshots[0].bot_id == second.id
        })
        .await;
    }

    #[tokio::test]
    async fn updating_credentials_rebuilds_only_the_changed_runtime() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let first = create_bot(&service, "bot-alpha").await;
        let second = create_bot(&service, "bot-beta").await;
        let client = Arc::new(FakeFeishuClient::default());
        client.set_ok(first.id).await;
        client.set_ok(second.id).await;

        let manager = FeishuRuntimeManager::with_config(
            pool.clone(),
            service,
            client,
            FeishuRuntimeConfig {
                heartbeat_interval: Duration::from_millis(50),
                enable_long_connection: false,
                long_connection_retry_interval: Duration::from_millis(50),
            },
        );
        manager
            .restore_enabled_bots()
            .await
            .expect("restore runtimes");
        wait_for_condition(|| async { manager.snapshots().await.len() == 2 }).await;

        let before = manager.snapshots().await;
        let before_first = before
            .iter()
            .find(|snapshot| snapshot.bot_id == first.id)
            .expect("first snapshot")
            .generation;
        let before_second = before
            .iter()
            .find(|snapshot| snapshot.bot_id == second.id)
            .expect("second snapshot")
            .generation;

        manager
            .upsert_runtime(first.id)
            .await
            .expect("rebuild first runtime");

        wait_for_condition(|| async {
            manager
                .snapshots()
                .await
                .iter()
                .find(|snapshot| snapshot.bot_id == first.id)
                .is_some_and(|snapshot| snapshot.generation > before_first)
        })
        .await;

        let after = manager.snapshots().await;
        let after_second = after
            .iter()
            .find(|snapshot| snapshot.bot_id == second.id)
            .expect("second snapshot after rebuild")
            .generation;
        assert_eq!(after_second, before_second);
    }

    #[tokio::test]
    async fn failed_connection_attempts_move_bot_to_reconnecting() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let bot = create_bot(&service, "bot-failing").await;
        let client = Arc::new(FakeFeishuClient::default());
        client.set_err(bot.id, "invalid credentials").await;

        let manager = FeishuRuntimeManager::with_config(
            pool.clone(),
            service,
            client,
            FeishuRuntimeConfig {
                heartbeat_interval: Duration::from_millis(50),
                enable_long_connection: false,
                long_connection_retry_interval: Duration::from_millis(50),
            },
        );

        manager.upsert_runtime(bot.id).await.expect("start runtime");
        wait_for_condition(|| async {
            FeishuBot::find_by_id(&pool, bot.id)
                .await
                .expect("fetch bot")
                .is_some_and(|bot| bot.last_health_status == "reconnecting")
        })
        .await;
    }
}
