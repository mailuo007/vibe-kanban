use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
pub struct WorkspaceFeishuBinding {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub bot_id: Uuid,
    pub target_id: Uuid,
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
    #[ts(type = "Date")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "Date")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateWorkspaceFeishuBinding {
    pub workspace_id: Uuid,
    pub bot_id: Uuid,
    pub target_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateWorkspaceFeishuBinding {
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

impl WorkspaceFeishuBinding {
    pub async fn list_by_workspace_id(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      workspace_id,
                      bot_id,
                      target_id,
                      enabled,
                      notify_on_status,
                      notify_on_agent_reply,
                      notify_on_pr,
                      allow_commands,
                      allow_chat_messages,
                      allow_cards,
                      ack_reaction_enabled,
                      ack_reaction_emoji_type,
                      sync_group_announcement,
                      created_at,
                      updated_at
               FROM workspace_feishu_bindings
               WHERE workspace_id = ?
               ORDER BY created_at DESC"#,
        )
        .bind(workspace_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      workspace_id,
                      bot_id,
                      target_id,
                      enabled,
                      notify_on_status,
                      notify_on_agent_reply,
                      notify_on_pr,
                      allow_commands,
                      allow_chat_messages,
                      allow_cards,
                      ack_reaction_enabled,
                      ack_reaction_emoji_type,
                      sync_group_announcement,
                      created_at,
                      updated_at
               FROM workspace_feishu_bindings
               WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_workspace_and_target(
        pool: &SqlitePool,
        workspace_id: Uuid,
        target_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      workspace_id,
                      bot_id,
                      target_id,
                      enabled,
                      notify_on_status,
                      notify_on_agent_reply,
                      notify_on_pr,
                      allow_commands,
                      allow_chat_messages,
                      allow_cards,
                      ack_reaction_enabled,
                      ack_reaction_emoji_type,
                      sync_group_announcement,
                      created_at,
                      updated_at
               FROM workspace_feishu_bindings
               WHERE workspace_id = ?
                 AND target_id = ?"#,
        )
        .bind(workspace_id)
        .bind(target_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_by_target_id(
        pool: &SqlitePool,
        target_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      workspace_id,
                      bot_id,
                      target_id,
                      enabled,
                      notify_on_status,
                      notify_on_agent_reply,
                      notify_on_pr,
                      allow_commands,
                      allow_chat_messages,
                      allow_cards,
                      ack_reaction_enabled,
                      ack_reaction_emoji_type,
                      sync_group_announcement,
                      created_at,
                      updated_at
               FROM workspace_feishu_bindings
               WHERE target_id = ?
               ORDER BY updated_at DESC, created_at DESC, rowid DESC"#,
        )
        .bind(target_id)
        .fetch_all(pool)
        .await
    }

    pub async fn list_enabled_by_chat_identity(
        pool: &SqlitePool,
        open_chat_id: Option<&str>,
        chat_id: Option<&str>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        if open_chat_id.is_none() && chat_id.is_none() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, Self>(
            r#"SELECT wb.id,
                      wb.workspace_id,
                      wb.bot_id,
                      wb.target_id,
                      wb.enabled,
                      wb.notify_on_status,
                      wb.notify_on_agent_reply,
                      wb.notify_on_pr,
                      wb.allow_commands,
                      wb.allow_chat_messages,
                      wb.allow_cards,
                      wb.ack_reaction_enabled,
                      wb.ack_reaction_emoji_type,
                      wb.sync_group_announcement,
                      wb.created_at,
                      wb.updated_at
               FROM workspace_feishu_bindings wb
               INNER JOIN feishu_bot_targets target
                       ON target.id = wb.target_id
               WHERE wb.enabled = TRUE
                 AND (
                    (? IS NOT NULL AND target.open_chat_id = ?)
                    OR
                    (? IS NOT NULL AND target.chat_id = ?)
                 )
               ORDER BY wb.created_at DESC"#,
        )
        .bind(open_chat_id)
        .bind(open_chat_id)
        .bind(chat_id)
        .bind(chat_id)
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateWorkspaceFeishuBinding,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Self>(
            r#"INSERT INTO workspace_feishu_bindings (
                    id,
                    workspace_id,
                    bot_id,
                    target_id,
                    enabled,
                    notify_on_status,
                    notify_on_agent_reply,
                    notify_on_pr,
                    allow_commands,
                    allow_chat_messages,
                    allow_cards
                )
                VALUES (?, ?, ?, ?, TRUE, TRUE, TRUE, TRUE, FALSE, FALSE, TRUE)
                RETURNING id,
                          workspace_id,
                          bot_id,
                          target_id,
                          enabled,
                          notify_on_status,
                          notify_on_agent_reply,
                          notify_on_pr,
                          allow_commands,
                          allow_chat_messages,
                          allow_cards,
                          ack_reaction_enabled,
                          ack_reaction_emoji_type,
                          sync_group_announcement,
                          created_at,
                          updated_at"#,
        )
        .bind(id)
        .bind(data.workspace_id)
        .bind(data.bot_id)
        .bind(data.target_id)
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workspace_feishu_bindings WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateWorkspaceFeishuBinding,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE workspace_feishu_bindings
               SET enabled = ?,
                   notify_on_status = ?,
                   notify_on_agent_reply = ?,
                   notify_on_pr = ?,
                   allow_commands = ?,
                   allow_chat_messages = ?,
                   allow_cards = ?,
                   ack_reaction_enabled = ?,
                   ack_reaction_emoji_type = ?,
                   sync_group_announcement = ?,
                   updated_at = datetime('now', 'subsec')
               WHERE id = ?
               RETURNING id,
                         workspace_id,
                         bot_id,
                         target_id,
                         enabled,
                         notify_on_status,
                         notify_on_agent_reply,
                         notify_on_pr,
                         allow_commands,
                         allow_chat_messages,
                         allow_cards,
                         ack_reaction_enabled,
                         ack_reaction_emoji_type,
                         sync_group_announcement,
                         created_at,
                         updated_at"#,
        )
        .bind(data.enabled)
        .bind(data.notify_on_status)
        .bind(data.notify_on_agent_reply)
        .bind(data.notify_on_pr)
        .bind(data.allow_commands)
        .bind(data.allow_chat_messages)
        .bind(data.allow_cards)
        .bind(data.ack_reaction_enabled)
        .bind(&data.ack_reaction_emoji_type)
        .bind(data.sync_group_announcement)
        .bind(id)
        .fetch_one(pool)
        .await
    }
}
