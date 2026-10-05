CREATE TABLE IF NOT EXISTS group_members (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id BIGINT NOT NULL,
    telegram_user_id BIGINT NOT NULL,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(chat_id, telegram_user_id)
);

CREATE INDEX IF NOT EXISTS idx_group_members_chat_id ON group_members(chat_id);
