use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
pub struct FeishuConversation {
    pub id: Uuid,
    pub bot_id: Uuid,
    pub target_id: Uuid,
    pub workspace_id: Uuid,
    pub session_id: Option<Uuid>,
    pub feishu_user_id: Option<String>,
    pub last_message_at: Option<String>,
    pub last_card_context: Option<String>,
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateFeishuConversation {
    pub bot_id: Uuid,
    pub target_id: Uuid,
    pub workspace_id: Uuid,
    pub session_id: Option<Uuid>,
    pub feishu_user_id: Option<String>,
    pub last_message_at: Option<String>,
    pub last_card_context: Option<String>,
}

impl FeishuConversation {
    pub async fn list_by_workspace_id(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_id,
                      workspace_id,
                      session_id,
                      feishu_user_id,
                      last_message_at,
                      last_card_context,
                      created_at,
                      updated_at
               FROM feishu_conversations
               WHERE workspace_id = ?
               ORDER BY updated_at DESC"#,
        )
        .bind(workspace_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_id,
                      workspace_id,
                      session_id,
                      feishu_user_id,
                      last_message_at,
                      last_card_context,
                      created_at,
                      updated_at
               FROM feishu_conversations
               WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateFeishuConversation,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Self>(
            r#"INSERT INTO feishu_conversations (
                    id,
                    bot_id,
                    target_id,
                    workspace_id,
                    session_id,
                    feishu_user_id,
                    last_message_at,
                    last_card_context
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING id,
                          bot_id,
                          target_id,
                          workspace_id,
                          session_id,
                          feishu_user_id,
                          last_message_at,
                          last_card_context,
                          created_at,
                          updated_at"#,
        )
        .bind(id)
        .bind(data.bot_id)
        .bind(data.target_id)
        .bind(data.workspace_id)
        .bind(data.session_id)
        .bind(&data.feishu_user_id)
        .bind(&data.last_message_at)
        .bind(&data.last_card_context)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_binding(
        pool: &SqlitePool,
        workspace_id: Uuid,
        bot_id: Uuid,
        target_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_id,
                      workspace_id,
                      session_id,
                      feishu_user_id,
                      last_message_at,
                      last_card_context,
                      created_at,
                      updated_at
               FROM feishu_conversations
               WHERE workspace_id = ?
                 AND bot_id = ?
                 AND target_id = ?
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(workspace_id)
        .bind(bot_id)
        .bind(target_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_by_session_id(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      bot_id,
                      target_id,
                      workspace_id,
                      session_id,
                      feishu_user_id,
                      last_message_at,
                      last_card_context,
                      created_at,
                      updated_at
               FROM feishu_conversations
               WHERE session_id = ?
               ORDER BY updated_at DESC"#,
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
    }

    pub async fn update_session_id(
        pool: &SqlitePool,
        id: Uuid,
        session_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_conversations
               SET session_id = ?,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         bot_id,
                         target_id,
                         workspace_id,
                         session_id,
                         feishu_user_id,
                         last_message_at,
                         last_card_context,
                         created_at,
                         updated_at"#,
        )
        .bind(session_id)
        .bind(id)
        .fetch_one(pool)
        .await
    }

    pub async fn touch_last_message_at(pool: &SqlitePool, id: Uuid) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE feishu_conversations
               SET last_message_at = datetime('now', 'subsec'),
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         bot_id,
                         target_id,
                         workspace_id,
                         session_id,
                         feishu_user_id,
                         last_message_at,
                         last_card_context,
                         created_at,
                         updated_at"#,
        )
        .bind(id)
        .fetch_one(pool)
        .await
    }
}
