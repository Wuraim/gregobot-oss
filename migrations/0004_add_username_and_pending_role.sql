-- Add 'pending' role for users who have interacted with the bot but are not yet authorized
INSERT OR IGNORE INTO roles (name) VALUES ('pending');

-- Add username column to users table (nullable, no UNIQUE constraint)
ALTER TABLE users ADD COLUMN username TEXT;
