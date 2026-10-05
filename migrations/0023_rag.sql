-- RAG: long-term memory and document storage with vector search.

CREATE TABLE IF NOT EXISTS chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_type TEXT    NOT NULL,
    source_id   INTEGER,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chat_id     BIGINT,
    role        TEXT,
    content     TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chunks_user ON chunks(user_id);

-- The sqlite-vec virtual table `chunks_vec` is created at application startup
-- (see src/db/mod.rs) because it requires the `vec0` extension to be loaded,
-- which is not available to sqlx-cli during migration.
