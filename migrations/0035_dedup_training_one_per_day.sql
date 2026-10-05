-- One-shot cleanup of historical duplicates (old photo-reaction bug, double
-- commands, LLM logging twice). Keep at most one session per user per UTC day.
-- Prefer the row with endurance/sets, then the longer description, then latest id.

CREATE TABLE _training_keep AS
SELECT id FROM (
    SELECT
        te.id,
        ROW_NUMBER() OVER (
            PARTITION BY te.user_id, date(te.recorded_at)
            ORDER BY
                CASE WHEN e.training_id IS NOT NULL THEN 1 ELSE 0 END DESC,
                CASE WHEN EXISTS (
                    SELECT 1 FROM training_strength_sets s WHERE s.training_id = te.id
                ) THEN 1 ELSE 0 END DESC,
                length(te.description) DESC,
                te.id DESC
        ) AS rn
    FROM training_entries te
    LEFT JOIN training_endurance e ON e.training_id = te.id
)
WHERE rn = 1;

DELETE FROM training_endurance
WHERE training_id NOT IN (SELECT id FROM _training_keep);

DELETE FROM training_strength_sets
WHERE training_id NOT IN (SELECT id FROM _training_keep);

DELETE FROM training_entries
WHERE id NOT IN (SELECT id FROM _training_keep);

DROP TABLE _training_keep;
