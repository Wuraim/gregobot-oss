-- Telegram message date (UTC). created_at stays the ingest time.
-- Explicit INTEGER PRIMARY KEY so photo_vec.rowid stays stable across VACUUM.
-- Cached gemini-embedding-2 vector (JSON float array) lives in `embedding`.

CREATE TABLE photo_messages_new (
    id INTEGER PRIMARY KEY,
    chat_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    sender_telegram_user_id INTEGER NOT NULL,
    caption TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    training_counted INTEGER NOT NULL DEFAULT 0,
    file_id TEXT,
    image_jpeg BLOB,
    taken_at TEXT,
    embedding TEXT,
    UNIQUE (chat_id, message_id)
);

INSERT INTO photo_messages_new (
    id,
    chat_id,
    message_id,
    sender_telegram_user_id,
    caption,
    created_at,
    training_counted,
    file_id,
    image_jpeg,
    taken_at,
    embedding
)
SELECT
    rowid,
    chat_id,
    message_id,
    sender_telegram_user_id,
    caption,
    created_at,
    training_counted,
    file_id,
    image_jpeg,
    created_at,
    NULL
FROM photo_messages;

DROP TABLE photo_messages;
ALTER TABLE photo_messages_new RENAME TO photo_messages;
