CREATE TABLE IF NOT EXISTS strength_exercises (
    id   INTEGER PRIMARY KEY,
    slug TEXT    NOT NULL UNIQUE
);

INSERT INTO strength_exercises (id, slug) VALUES
    (1,  'squat'),
    (2,  'front_squat'),
    (3,  'bench_press'),
    (4,  'incline_bench_press'),
    (5,  'decline_bench_press'),
    (6,  'overhead_press'),
    (7,  'deadlift'),
    (8,  'romanian_deadlift'),
    (9,  'barbell_row'),
    (10, 'pull_up'),
    (11, 'chin_up'),
    (12, 'dip'),
    (13, 'lat_pulldown'),
    (14, 'hip_thrust'),
    (15, 'lunge'),
    (16, 'bulgarian_split_squat'),
    (17, 'leg_press'),
    (18, 'leg_curl'),
    (19, 'leg_extension'),
    (20, 'calf_raise'),
    (21, 'biceps_curl'),
    (22, 'dumbbell_biceps_curl'),
    (23, 'hammer_curl'),
    (24, 'triceps_extension'),
    (25, 'triceps_pushdown'),
    (26, 'lateral_raise'),
    (27, 'face_pull'),
    (28, 'pec_deck'),
    (29, 'cable_fly'),
    (30, 'other');

-- SQLite forbids ADD COLUMN with REFERENCES + a non-NULL default when FKs are on.
ALTER TABLE training_strength_sets
    ADD COLUMN exercise_id INTEGER NOT NULL DEFAULT 30;
