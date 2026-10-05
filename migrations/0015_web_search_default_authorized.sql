-- Enable web search by default for all currently authorized users.
-- Pending users keep their current value (they cannot interact with the bot yet).
UPDATE users
SET web_search_enabled = 1
WHERE role_id IN (SELECT id FROM roles WHERE name IN ('admin', 'sportive'));
