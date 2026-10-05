ALTER TABLE conversations ADD COLUMN chat_id BIGINT;

CREATE INDEX IF NOT EXISTS idx_conversations_chat_id ON conversations(chat_id);
