use std::{sync::Arc, time::Duration};

use anyhow::Result;
use db::models::feishu_bot::FeishuBot;
use sqlx::SqlitePool;

use crate::feishu::{
    chat_runner::FeishuChatRunner, client::FeishuClient, dispatcher::FeishuDispatcher,
    long_connection::FeishuLongConnection, secret_store::FeishuSecretStore, service::FeishuService,
};

#[derive(Debug, Clone)]
pub struct FeishuRuntimeConfig {
    pub heartbeat_interval: Duration,
    pub enable_long_connection: bool,
    pub long_connection_retry_interval: Duration,
}

impl Default for FeishuRuntimeConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(300),
            enable_long_connection: true,
            long_connection_retry_interval: Duration::from_secs(5),
        }
    }
}

pub struct FeishuRuntime<S: FeishuSecretStore, C: FeishuClient> {
    bot_id: uuid::Uuid,
    pool: SqlitePool,
    service: Arc<FeishuService<S>>,
    client: Arc<C>,
    chat_runner: Arc<dyn FeishuChatRunner>,
    config: FeishuRuntimeConfig,
}

impl<S: FeishuSecretStore, C: FeishuClient> Clone for FeishuRuntime<S, C> {
    fn clone(&self) -> Self {
        Self {
            bot_id: self.bot_id,
            pool: self.pool.clone(),
            service: self.service.clone(),
            client: self.client.clone(),
            chat_runner: self.chat_runner.clone(),
            config: self.config.clone(),
        }
    }
}

impl<S: FeishuSecretStore + 'static, C: FeishuClient + 'static> FeishuRuntime<S, C> {
    pub fn new(
        bot_id: uuid::Uuid,
        pool: SqlitePool,
        service: Arc<FeishuService<S>>,
        client: Arc<C>,
        chat_runner: Arc<dyn FeishuChatRunner>,
        config: FeishuRuntimeConfig,
    ) -> Self {
        Self {
            bot_id,
            pool,
            service,
            client,
            chat_runner,
            config,
        }
    }

    pub async fn run(self) {
        if !self.config.enable_long_connection {
            self.run_heartbeat_loop().await;
            return;
        }

        let heartbeat_runtime = self.clone();
        let long_connection_runtime = self;
        let dispatcher = FeishuDispatcher::with_chat_runner(
            long_connection_runtime.service.clone(),
            long_connection_runtime.client.clone(),
            long_connection_runtime.chat_runner.clone(),
        );

        tokio::join!(
            heartbeat_runtime.run_heartbeat_loop(),
            long_connection_runtime.run_long_connection_loop(dispatcher)
        );
    }

    async fn reconcile_once(&self) -> Result<()> {
        let Some(bot) = FeishuBot::find_by_id(&self.pool, self.bot_id).await? else {
            return Ok(());
        };

        if !bot.enabled {
            let _ = FeishuBot::update_health(&self.pool, bot.id, "disabled", None).await;
            return Ok(());
        }

        let secrets = self.service.resolve_bot_secrets(&bot).await?;
        self.client.validate_bot(&bot, &secrets).await?;
        let _ = FeishuBot::update_health(&self.pool, bot.id, "healthy", None).await;
        Ok(())
    }

    async fn run_heartbeat_loop(self) {
        let mut interval = tokio::time::interval(self.config.heartbeat_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            if let Err(error) = self.reconcile_once().await {
                let _ = FeishuBot::update_health(
                    &self.pool,
                    self.bot_id,
                    "reconnecting",
                    Some(&error.to_string()),
                )
                .await;
            }

            interval.tick().await;
        }
    }

    async fn run_long_connection_loop(self, dispatcher: FeishuDispatcher<S, C>) {
        let long_connection =
            FeishuLongConnection::new(self.bot_id, self.service.clone(), dispatcher);

        loop {
            if let Err(error) = long_connection.run_once().await {
                tracing::warn!(
                    bot_id = %self.bot_id,
                    ?error,
                    "Feishu long connection cycle ended"
                );
                let _ = FeishuBot::update_health(
                    &self.pool,
                    self.bot_id,
                    "degraded",
                    Some(&error.to_string()),
                )
                .await;
            }

            tokio::time::sleep(self.config.long_connection_retry_interval).await;
        }
    }
}
