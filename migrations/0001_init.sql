-- Weight body measurements
CREATE TABLE IF NOT EXISTS weight_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id     INTEGER NOT NULL,
    weight_kg   REAL    NOT NULL,
    recorded_at TEXT    NOT NULL  -- ISO-8601, e.g. "2026-05-30T09:00:00Z"
);

-- Sport / training sessions
CREATE TABLE IF NOT EXISTS training_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id     INTEGER NOT NULL,
    description TEXT    NOT NULL,
    recorded_at TEXT    NOT NULL  -- ISO-8601
);
