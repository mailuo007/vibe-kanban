ALTER TABLE workspace_feishu_bindings
ADD COLUMN allow_chat_messages BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE feishu_conversations
ADD COLUMN session_id BLOB;

CREATE INDEX idx_workspace_feishu_bindings_chat_mode
    ON workspace_feishu_bindings (allow_chat_messages);

CREATE INDEX idx_feishu_conversations_session_id
    ON feishu_conversations (session_id);

CREATE UNIQUE INDEX idx_feishu_conversations_workspace_bot_target
    ON feishu_conversations (workspace_id, bot_id, target_id);
