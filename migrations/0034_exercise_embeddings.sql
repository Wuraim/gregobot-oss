-- Cached Gemini embeddings for the strength-exercise catalog (KNN resolve).
CREATE TABLE IF NOT EXISTS exercise_embeddings (
    slug        TEXT PRIMARY KEY,
    embed_text  TEXT NOT NULL,
    embedding   TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
