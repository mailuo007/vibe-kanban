ALTER TABLE workspace_feishu_bindings
ADD COLUMN ack_reaction_enabled BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE workspace_feishu_bindings
ADD COLUMN ack_reaction_emoji_type TEXT NOT NULL DEFAULT 'Typing';

ALTER TABLE workspace_feishu_bindings
ADD COLUMN sync_group_announcement BOOLEAN NOT NULL DEFAULT TRUE;
