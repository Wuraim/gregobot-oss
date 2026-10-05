CREATE TABLE IF NOT EXISTS photo_messages (
    chat_id                   INTEGER NOT NULL,
    message_id                INTEGER NOT NULL,
    sender_telegram_user_id   INTEGER NOT NULL,
    caption                   TEXT,
    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (chat_id, message_id)
);

CREATE TABLE IF NOT EXISTS photo_reactions (
    chat_id                   INTEGER NOT NULL,
    message_id                INTEGER NOT NULL,
    reactor_telegram_user_id  INTEGER NOT NULL,
    processed_at              TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (chat_id, message_id, reactor_telegram_user_id)
);
