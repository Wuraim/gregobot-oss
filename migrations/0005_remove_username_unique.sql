-- No-op: migration 4 no longer adds a UNIQUE constraint on username,
-- so no removal is needed. This migration exists only to preserve
-- sequence numbering.
SELECT 1;
