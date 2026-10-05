-- Telegram display name fallback when @username is unset / private.
ALTER TABLE users ADD COLUMN first_name TEXT;
