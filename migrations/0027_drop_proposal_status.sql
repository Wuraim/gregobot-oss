-- Proposals no longer have a status: they simply exist until deleted.
DROP INDEX IF EXISTS idx_proposals_status;
ALTER TABLE feature_proposals DROP COLUMN status;
