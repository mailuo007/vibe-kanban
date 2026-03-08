use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
pub struct FeishuBotTarget {
    pub id: Uuid,
    pub bot_id: Uuid,
    pub target_type: String,
    pub open_chat_id: Option<String>,
    pub chat_id: Option<String>,
    pub name: String,
    pub source: String,
    pub is_active: bool,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateFeishuBotTarget {
    pub bot_id: Uuid,
    pub target_type: String,
    pub open_chat_id: Option<String>,
    pub chat_id: Option<String>,
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateFeishuBotTarget {
    pub target_type: String,
    pub open_chat_id: Option<String>,
    pub chat_id: Option<String>,
    pub name: String,
    pub source: String,
    pub is_active: bool,
}

impl FeishuBotTarget {
    pub async fn list_by_bot_id(pool: &SqlitePool, bot_id: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_type,
                      open_chat_id,
                      chat_id,
                      name,
                      source,
                      is_active,
                      created_at,
                      updated_at
               FROM feishu_bot_targets
               WHERE bot_id = ?
               ORDER BY name ASC"#,
        )
        .bind(bot_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_type,
                      open_chat_id,
                      chat_id,
                      name,
                      source,
                      is_active,
                      created_at,
                      updated_at
               FROM feishu_bot_targets
               WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_identity(
        pool: &SqlitePool,
        bot_id: Uuid,
        open_chat_id: Option<&str>,
        chat_id: Option<&str>,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_type,
                      open_chat_id,
                      chat_id,
                      name,
                      source,
                      is_active,
                      created_at,
                      updated_at
               FROM feishu_bot_targets
               WHERE bot_id = ?
                 AND (
                    (? IS NOT NULL AND open_chat_id = ?)
                    OR
                    (? IS NOT NULL AND chat_id = ?)
                 )
               LIMIT 1"#,
        )
        .bind(bot_id)
        .bind(open_chat_id)
        .bind(open_chat_id)
        .bind(chat_id)
        .bind(chat_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateFeishuBotTarget,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Self>(
            r#"INSERT INTO feishu_bot_targets (
                    id,
                    bot_id,
                    target_type,
                    open_chat_id,
                    chat_id,
                    name,
                    source,
                    is_active
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, TRUE)
                RETURNING id,
                          bot_id,
                          target_type,
                          open_chat_id,
                          chat_id,
                          name,
                          source,
                          is_active,
                          created_at,
                          updated_at"#,
        )
        .bind(id)
        .bind(data.bot_id)
        .bind(&data.target_type)
        .bind(&data.open_chat_id)
        .bind(&data.chat_id)
        .bind(&data.name)
        .bind(&data.source)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateFeishuBotTarget,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_bot_targets
               SET target_type = ?,
                   open_chat_id = ?,
                   chat_id = ?,
                   name = ?,
                   source = ?,
                   is_active = ?,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         bot_id,
                         target_type,
                         open_chat_id,
                         chat_id,
                         name,
                         source,
                         is_active,
                         created_at,
                         updated_at"#,
        )
        .bind(&data.target_type)
        .bind(&data.open_chat_id)
        .bind(&data.chat_id)
        .bind(&data.name)
        .bind(&data.source)
        .bind(data.is_active)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM feishu_bot_targets WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }
}
