CREATE TABLE feishu_bots (
    id                       BLOB PRIMARY KEY,
    name                     TEXT NOT NULL UNIQUE,
    app_id                   TEXT NOT NULL,
    app_secret_ref           TEXT NOT NULL,
    encrypt_key_ref          TEXT,
    verification_token_ref   TEXT,
    tenant_mode              TEXT NOT NULL,
    enabled                  BOOLEAN NOT NULL DEFAULT TRUE,
    last_health_status       TEXT NOT NULL DEFAULT 'starting'
                                CHECK (last_health_status IN (
                                    'starting',
                                    'healthy',
                                    'degraded',
                                    'reconnecting',
                                    'disabled',
                                    'failed'
                                )),
    last_error               TEXT,
    created_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at               TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);

CREATE INDEX idx_feishu_bots_enabled
    ON feishu_bots (enabled)
    WHERE enabled = TRUE;

CREATE TABLE feishu_bot_targets (
    id            BLOB PRIMARY KEY,
    bot_id        BLOB NOT NULL,
    target_type   TEXT NOT NULL,
    open_chat_id  TEXT,
    chat_id       TEXT,
    name          TEXT NOT NULL,
    source        TEXT NOT NULL,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    CHECK (open_chat_id IS NOT NULL OR chat_id IS NOT NULL),
    FOREIGN KEY (bot_id) REFERENCES feishu_bots(id),
    UNIQUE(id, bot_id)
);

CREATE INDEX idx_feishu_bot_targets_bot_id
    ON feishu_bot_targets (bot_id);

CREATE UNIQUE INDEX idx_feishu_bot_targets_bot_open_chat_id
    ON feishu_bot_targets (bot_id, open_chat_id)
    WHERE open_chat_id IS NOT NULL;

CREATE UNIQUE INDEX idx_feishu_bot_targets_bot_chat_id
    ON feishu_bot_targets (bot_id, chat_id)
    WHERE chat_id IS NOT NULL;

CREATE TABLE workspace_feishu_bindings (
    id                     BLOB PRIMARY KEY,
    workspace_id           BLOB NOT NULL,
    bot_id                 BLOB NOT NULL,
    target_id              BLOB NOT NULL,
    enabled                BOOLEAN NOT NULL DEFAULT TRUE,
    notify_on_status       BOOLEAN NOT NULL DEFAULT TRUE,
    notify_on_agent_reply  BOOLEAN NOT NULL DEFAULT TRUE,
    notify_on_pr           BOOLEAN NOT NULL DEFAULT TRUE,
    allow_commands         BOOLEAN NOT NULL DEFAULT FALSE,
    allow_cards            BOOLEAN NOT NULL DEFAULT TRUE,
    created_at             TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at             TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (bot_id) REFERENCES feishu_bots(id),
    FOREIGN KEY (target_id, bot_id) REFERENCES feishu_bot_targets(id, bot_id),
    UNIQUE(workspace_id, target_id)
);

CREATE INDEX idx_workspace_feishu_bindings_workspace_id
    ON workspace_feishu_bindings (workspace_id);

CREATE INDEX idx_workspace_feishu_bindings_bot_id
    ON workspace_feishu_bindings (bot_id);

CREATE TABLE feishu_conversations (
    id                  BLOB PRIMARY KEY,
    bot_id              BLOB NOT NULL,
    target_id           BLOB NOT NULL,
    workspace_id        BLOB NOT NULL,
    feishu_user_id      TEXT,
    last_message_at     TEXT,
    last_card_context   TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (bot_id) REFERENCES feishu_bots(id),
    FOREIGN KEY (target_id, bot_id) REFERENCES feishu_bot_targets(id, bot_id)
);

CREATE INDEX idx_feishu_conversations_workspace_target
    ON feishu_conversations (workspace_id, target_id);

CREATE INDEX idx_feishu_conversations_bot_id
    ON feishu_conversations (bot_id);

CREATE TABLE feishu_delivery_logs (
    id               BLOB PRIMARY KEY,
    workspace_id     BLOB,
    bot_id           BLOB NOT NULL,
    target_id        BLOB NOT NULL,
    event_type       TEXT NOT NULL,
    payload_summary  TEXT,
    status           TEXT NOT NULL,
    retry_count      INTEGER NOT NULL DEFAULT 0,
    message_id       TEXT,
    error_message    TEXT,
    sent_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
    FOREIGN KEY (bot_id) REFERENCES feishu_bots(id),
    FOREIGN KEY (target_id, bot_id) REFERENCES feishu_bot_targets(id, bot_id)
);

CREATE INDEX idx_feishu_delivery_logs_workspace_id
    ON feishu_delivery_logs (workspace_id);

CREATE INDEX idx_feishu_delivery_logs_bot_target
    ON feishu_delivery_logs (bot_id, target_id);
