-- Opt-in monthly training graph per group chat.
ALTER TABLE chat_settings ADD COLUMN monthly_training_graph_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_settings ADD COLUMN last_monthly_training_graph_at TEXT;
