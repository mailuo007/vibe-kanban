use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
pub struct FeishuDeliveryLog {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub bot_id: Uuid,
    pub target_id: Uuid,
    pub event_type: String,
    pub payload_summary: Option<String>,
    pub status: String,
    pub retry_count: i64,
    pub message_id: Option<String>,
    pub error_message: Option<String>,
    #[ts(type = "Date")]
    pub sent_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateFeishuDeliveryLog {
    pub workspace_id: Option<Uuid>,
    pub bot_id: Uuid,
    pub target_id: Uuid,
    pub event_type: String,
    pub payload_summary: Option<String>,
    pub status: String,
    pub retry_count: i64,
    pub message_id: Option<String>,
    pub error_message: Option<String>,
}

impl FeishuDeliveryLog {
    pub async fn list_by_workspace_id(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"SELECT id,
                      workspace_id,
                      bot_id,
                      target_id,
                      event_type,
                      payload_summary,
                      status,
                      retry_count,
                      message_id,
                      error_message,
                      sent_at
               FROM feishu_delivery_logs
               WHERE workspace_id = ?
               ORDER BY sent_at DESC"#,
        )
        .bind(workspace_id)
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateFeishuDeliveryLog,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query_as::<_, Self>(
            r#"INSERT INTO feishu_delivery_logs (
                    id,
                    workspace_id,
                    bot_id,
                    target_id,
                    event_type,
                    payload_summary,
                    status,
                    retry_count,
                    message_id,
                    error_message
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING id,
                          workspace_id,
                          bot_id,
                          target_id,
                          event_type,
                          payload_summary,
                          status,
                          retry_count,
                          message_id,
                          error_message,
                          sent_at"#,
        )
        .bind(id)
        .bind(data.workspace_id)
        .bind(data.bot_id)
        .bind(data.target_id)
        .bind(&data.event_type)
        .bind(&data.payload_summary)
        .bind(&data.status)
        .bind(data.retry_count)
        .bind(&data.message_id)
        .bind(&data.error_message)
        .fetch_one(pool)
        .await
    }
}
