-- Tool embeddings for semantic tool selection.
-- Each tool has an enriched multilingual description that is embedded once
-- and reused across restarts. The embed_text column is used to detect when
-- a tool's description has changed and needs to be re-embedded.

CREATE TABLE IF NOT EXISTS tool_embeddings (
    tool_name   TEXT PRIMARY KEY,
    embed_text  TEXT    NOT NULL,
    embedding   TEXT    NOT NULL,  -- JSON array of f32 values
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);
