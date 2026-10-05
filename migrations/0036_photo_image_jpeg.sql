-- Store a standardized JPEG copy of each tracked photo (lossy, max edge 1280).
-- file_id remains so we can retry a download if the blob is missing.
ALTER TABLE photo_messages ADD COLUMN image_jpeg BLOB;
