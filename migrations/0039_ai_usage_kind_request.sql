ALTER TABLE ai_usage ADD COLUMN kind TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE ai_usage ADD COLUMN request_id TEXT;
CREATE INDEX IF NOT EXISTS idx_ai_usage_user_created ON ai_usage(user_id, created_at);
CREATE INDEX IF NOT EXISTS idx_ai_usage_request ON ai_usage(request_id);
