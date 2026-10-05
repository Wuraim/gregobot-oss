-- Add per-user web search permission for the AI.
-- Default is 1 (enabled) so authorized users get it automatically.
ALTER TABLE users ADD COLUMN web_search_enabled INTEGER NOT NULL DEFAULT 1;
