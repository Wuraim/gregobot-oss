-- French accounts created before the language default still sit on UTC.
-- Leave an explicit non-UTC zone, and non-French accounts, unchanged.
UPDATE users
SET timezone = 'Europe/Paris'
WHERE language_code = 'fr' AND timezone = 'UTC';
