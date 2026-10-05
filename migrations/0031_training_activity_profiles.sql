-- Catalogue of activity types. Flags decide which satellite tables a session may use.
CREATE TABLE IF NOT EXISTS activity_types (
    id              INTEGER PRIMARY KEY,
    slug            TEXT    NOT NULL UNIQUE,
    uses_endurance  INTEGER NOT NULL DEFAULT 0,
    uses_strength   INTEGER NOT NULL DEFAULT 0
);

INSERT INTO activity_types (id, slug, uses_endurance, uses_strength) VALUES
    (1, 'run',      1, 0),
    (2, 'ride',     1, 0),
    (3, 'walk',     1, 0),
    (4, 'hike',     1, 0),
    (5, 'swim',     1, 0),
    (6, 'gym',      0, 1),
    (7, 'crossfit', 1, 1),
    (8, 'other',    0, 0);

-- SQLite forbids ADD COLUMN with REFERENCES + a non-NULL default when FKs are on.
ALTER TABLE training_entries
    ADD COLUMN activity_type_id INTEGER NOT NULL DEFAULT 8;

-- Shared metrics for run / ride / walk / hike / swim / hybrid (crossfit).
CREATE TABLE IF NOT EXISTS training_endurance (
    training_id  INTEGER PRIMARY KEY REFERENCES training_entries(id) ON DELETE CASCADE,
    duration_sec INTEGER,
    distance_m   REAL,
    elevation_m  REAL,
    avg_hr       INTEGER,
    calories     INTEGER
);

-- One row per set for gym / hybrid.
CREATE TABLE IF NOT EXISTS training_strength_sets (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    training_id   INTEGER NOT NULL REFERENCES training_entries(id) ON DELETE CASCADE,
    exercise_name TEXT    NOT NULL,
    set_index     INTEGER NOT NULL,
    reps          INTEGER,
    weight_kg     REAL
);

CREATE INDEX IF NOT EXISTS idx_strength_sets_training
    ON training_strength_sets (training_id, set_index);
