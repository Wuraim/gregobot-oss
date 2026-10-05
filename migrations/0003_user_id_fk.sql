-- Recreate weight_entries with user_id FK instead of chat_id
DROP TABLE IF EXISTS weight_entries;
CREATE TABLE weight_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id),
    weight_kg   REAL    NOT NULL,
    recorded_at TEXT    NOT NULL  -- ISO-8601, e.g. "2026-05-30T09:00:00Z"
);

-- Recreate training_entries with user_id FK instead of chat_id
DROP TABLE IF EXISTS training_entries;
CREATE TABLE training_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id),
    description TEXT    NOT NULL,
    recorded_at TEXT    NOT NULL  -- ISO-8601
);
