-- Hard food constraints and habits (allergy / dislike / like / habit).
-- Qualitative taste stays in conversation RAG; this table is the safety layer
-- retrieval can miss. Embeddings follow the same JSON pattern as food_embeddings.

CREATE TABLE IF NOT EXISTS food_preferences (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL,
    text        TEXT    NOT NULL,
    food_id     INTEGER REFERENCES foods(id) ON DELETE SET NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_food_preferences_user ON food_preferences(user_id);

CREATE TABLE IF NOT EXISTS food_preference_embeddings (
    preference_id INTEGER PRIMARY KEY REFERENCES food_preferences(id) ON DELETE CASCADE,
    embed_text    TEXT    NOT NULL,
    embedding     TEXT    NOT NULL,
    created_at    TEXT    NOT NULL DEFAULT (datetime('now'))
);
