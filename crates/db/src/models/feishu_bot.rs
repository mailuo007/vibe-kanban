use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
pub struct FeishuBot {
    pub id: Uuid,
    pub name: String,
    pub app_id: String,
    pub app_secret_ref: String,
    pub encrypt_key_ref: Option<String>,
    pub verification_token_ref: Option<String>,
    pub tenant_mode: String,
    pub enabled: bool,
    pub last_health_status: String,
    pub last_error: Option<String>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateFeishuBot {
    pub name: String,
    pub app_id: String,
    pub app_secret_ref: String,
    pub encrypt_key_ref: Option<String>,
    pub verification_token_ref: Option<String>,
    pub tenant_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateFeishuBot {
    pub name: String,
    pub app_id: String,
    pub app_secret_ref: String,
    pub encrypt_key_ref: Option<String>,
    pub verification_token_ref: Option<String>,
    pub tenant_mode: String,
    pub enabled: bool,
}

impl FeishuBot {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      name,
                      app_id,
                      app_secret_ref,
                      encrypt_key_ref,
                      verification_token_ref,
                      tenant_mode,
                      enabled,
                      last_health_status,
                      last_error,
                      created_at,
                      updated_at
               FROM feishu_bots
               ORDER BY name ASC"#,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_enabled(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      name,
                      app_id,
                      app_secret_ref,
                      encrypt_key_ref,
                      verification_token_ref,
                      tenant_mode,
                      enabled,
                      last_health_status,
                      last_error,
                      created_at,
                      updated_at
               FROM feishu_bots
               WHERE enabled = TRUE
               ORDER BY name ASC"#,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      name,
                      app_id,
                      app_secret_ref,
                      encrypt_key_ref,
                      verification_token_ref,
                      tenant_mode,
                      enabled,
                      last_health_status,
                      last_error,
                      created_at,
                      updated_at
               FROM feishu_bots
               WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn create(pool: &SqlitePool, data: &CreateFeishuBot) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Self>(
            r#"INSERT INTO feishu_bots (
                    id,
                    name,
                    app_id,
                    app_secret_ref,
                    encrypt_key_ref,
                    verification_token_ref,
                    tenant_mode,
                    enabled,
                    last_health_status
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, TRUE, 'starting')
                RETURNING id,
                          name,
                          app_id,
                          app_secret_ref,
                          encrypt_key_ref,
                          verification_token_ref,
                          tenant_mode,
                          enabled,
                          last_health_status,
                          last_error,
                          created_at,
                          updated_at"#,
        )
        .bind(id)
        .bind(&data.name)
        .bind(&data.app_id)
        .bind(&data.app_secret_ref)
        .bind(&data.encrypt_key_ref)
        .bind(&data.verification_token_ref)
        .bind(&data.tenant_mode)
        .fetch_one(pool)
        .await
    }

    pub async fn update_health(
        pool: &SqlitePool,
        id: Uuid,
        last_health_status: &str,
        last_error: Option<&str>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_bots
               SET last_health_status = ?,
                   last_error = ?,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         name,
                         app_id,
                         app_secret_ref,
                         encrypt_key_ref,
                         verification_token_ref,
                         tenant_mode,
                         enabled,
                         last_health_status,
                         last_error,
                         created_at,
                         updated_at"#,
        )
        .bind(last_health_status)
        .bind(last_error)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn set_enabled(
        pool: &SqlitePool,
        id: Uuid,
        enabled: bool,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_bots
               SET enabled = ?,
                   last_health_status = CASE
                       WHEN ? THEN last_health_status
                       ELSE 'disabled'
                   END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         name,
                         app_id,
                         app_secret_ref,
                         encrypt_key_ref,
                         verification_token_ref,
                         tenant_mode,
                         enabled,
                         last_health_status,
                         last_error,
                         created_at,
                         updated_at"#,
        )
        .bind(enabled)
        .bind(enabled)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateFeishuBot,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_bots
               SET name = ?,
                   app_id = ?,
                   app_secret_ref = ?,
                   encrypt_key_ref = ?,
                   verification_token_ref = ?,
                   tenant_mode = ?,
                   enabled = ?,
                   last_health_status = CASE
                       WHEN ? THEN last_health_status
                       ELSE 'disabled'
                   END,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         name,
                         app_id,
                         app_secret_ref,
                         encrypt_key_ref,
                         verification_token_ref,
                         tenant_mode,
                         enabled,
                         last_health_status,
                         last_error,
                         created_at,
                         updated_at"#,
        )
        .bind(&data.name)
        .bind(&data.app_id)
        .bind(&data.app_secret_ref)
        .bind(&data.encrypt_key_ref)
        .bind(&data.verification_token_ref)
        .bind(&data.tenant_mode)
        .bind(data.enabled)
        .bind(data.enabled)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM feishu_bots WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use uuid::Uuid;

    use super::{CreateFeishuBot, FeishuBot};
    use crate::models::{
        feishu_bot_target::{CreateFeishuBotTarget, FeishuBotTarget},
        workspace::{CreateWorkspace, Workspace},
        workspace_feishu_binding::{CreateWorkspaceFeishuBinding, WorkspaceFeishuBinding},
    };

    async fn test_pool() -> SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("memory sqlite url")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect memory db");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        pool
    }

    async fn create_workspace(pool: &SqlitePool) -> Workspace {
        Workspace::create(
            pool,
            &CreateWorkspace {
                branch: "main".into(),
                name: Some("Feishu Workspace".into()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create workspace")
    }

    #[tokio::test]
    async fn rejects_duplicate_feishu_bot_names() {
        let pool = test_pool().await;
        let create = CreateFeishuBot {
            name: "engineering-bot".into(),
            app_id: "cli_a".into(),
            app_secret_ref: "secret-a".into(),
            encrypt_key_ref: None,
            verification_token_ref: None,
            tenant_mode: "self_built".into(),
        };

        let _ = FeishuBot::create(&pool, &create).await.expect("first bot");
        let err = FeishuBot::create(&pool, &create)
            .await
            .expect_err("duplicate bot");

        assert!(matches!(err, sqlx::Error::Database(_)));
    }

    #[tokio::test]
    async fn rejects_duplicate_workspace_target_bindings() {
        let pool = test_pool().await;
        let bot = FeishuBot::create(
            &pool,
            &CreateFeishuBot {
                name: "product-bot".into(),
                app_id: "cli_b".into(),
                app_secret_ref: "secret-b".into(),
                encrypt_key_ref: None,
                verification_token_ref: None,
                tenant_mode: "self_built".into(),
            },
        )
        .await
        .expect("create bot");
        let target = FeishuBotTarget::create(
            &pool,
            &CreateFeishuBotTarget {
                bot_id: bot.id,
                target_type: "chat".into(),
                open_chat_id: Some("oc_123".into()),
                chat_id: Some("chat_123".into()),
                name: "Engineering".into(),
                source: "manual".into(),
            },
        )
        .await
        .expect("create target");
        let workspace = create_workspace(&pool).await;

        let create = CreateWorkspaceFeishuBinding {
            workspace_id: workspace.id,
            bot_id: bot.id,
            target_id: target.id,
        };

        let _ = WorkspaceFeishuBinding::create(&pool, &create)
            .await
            .expect("first binding");
        let err = WorkspaceFeishuBinding::create(&pool, &create)
            .await
            .expect_err("duplicate binding");

        assert!(matches!(err, sqlx::Error::Database(_)));
    }

    #[tokio::test]
    async fn blocks_deleting_a_bot_with_targets_or_bindings() {
        let pool = test_pool().await;
        let bot = FeishuBot::create(
            &pool,
            &CreateFeishuBot {
                name: "ops-bot".into(),
                app_id: "cli_c".into(),
                app_secret_ref: "secret-c".into(),
                encrypt_key_ref: None,
                verification_token_ref: None,
                tenant_mode: "self_built".into(),
            },
        )
        .await
        .expect("create bot");
        let target = FeishuBotTarget::create(
            &pool,
            &CreateFeishuBotTarget {
                bot_id: bot.id,
                target_type: "chat".into(),
                open_chat_id: Some("oc_456".into()),
                chat_id: Some("chat_456".into()),
                name: "Ops".into(),
                source: "manual".into(),
            },
        )
        .await
        .expect("create target");
        let workspace = create_workspace(&pool).await;

        let _binding = WorkspaceFeishuBinding::create(
            &pool,
            &CreateWorkspaceFeishuBinding {
                workspace_id: workspace.id,
                bot_id: bot.id,
                target_id: target.id,
            },
        )
        .await
        .expect("create binding");

        let err = FeishuBot::delete(&pool, bot.id)
            .await
            .expect_err("delete should be restricted");
        assert!(matches!(err, sqlx::Error::Database(_)));
    }
}
