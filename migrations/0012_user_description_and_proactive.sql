ALTER TABLE users ADD COLUMN description TEXT;

CREATE TABLE IF NOT EXISTS proactive_interactions (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id  INTEGER NOT NULL REFERENCES users(id),
    sent_at  TEXT NOT NULL DEFAULT (datetime('now')),
    message  TEXT
);
