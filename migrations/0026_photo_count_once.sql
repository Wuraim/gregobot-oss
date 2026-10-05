-- Count each photo at most once as a training entry, and keep the Telegram
-- file_id so the photo can be downloaded later (e.g. for AI description).

ALTER TABLE photo_messages ADD COLUMN training_counted INTEGER NOT NULL DEFAULT 0;

ALTER TABLE photo_messages ADD COLUMN file_id TEXT;

-- Photos that already produced a training entry (any recorded reaction) must
-- not be counted again.
UPDATE photo_messages
SET training_counted = 1
WHERE EXISTS (
    SELECT 1
    FROM photo_reactions pr
    WHERE pr.chat_id = photo_messages.chat_id
      AND pr.message_id = photo_messages.message_id
);
