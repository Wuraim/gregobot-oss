-- Persist the /logs ring buffer and /ai/logs traces on the SQLite volume
-- so they survive container/image restarts. Rows are pruned to a cap in
-- application code (LOG_MAX_ENTRIES / AI_LOG_MAX_ENTRIES).

CREATE TABLE IF NOT EXISTS app_logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT    NOT NULL,
    level       TEXT    NOT NULL,
    target      TEXT    NOT NULL,
    span_name   TEXT,
    span_id     TEXT,
    fields      TEXT    NOT NULL DEFAULT '[]',
    message     TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_traces (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id          TEXT    NOT NULL UNIQUE,
    timestamp           TEXT    NOT NULL,
    kind                TEXT    NOT NULL,
    telegram_user_id    INTEGER NOT NULL,
    payload             TEXT    NOT NULL
);
