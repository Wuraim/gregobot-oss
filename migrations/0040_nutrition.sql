-- Personal food catalog + daily meal diary (snapshotted macros per line).

CREATE TABLE IF NOT EXISTS foods (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id             INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name                TEXT    NOT NULL,
    brand               TEXT,
    barcode             TEXT,
    notes               TEXT,
    kcal_per_100        REAL    NOT NULL,
    protein_g_per_100   REAL    NOT NULL,
    carbs_g_per_100     REAL    NOT NULL,
    fat_g_per_100       REAL    NOT NULL,
    serving_qty         REAL    NOT NULL DEFAULT 100,
    serving_unit        TEXT    NOT NULL DEFAULT 'g',
    serving_label       TEXT,
    source              TEXT    NOT NULL DEFAULT 'manual',
    created_at          TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_foods_user ON foods(user_id);
CREATE INDEX IF NOT EXISTS idx_foods_user_name ON foods(user_id, name);

CREATE TABLE IF NOT EXISTS food_embeddings (
    food_id     INTEGER PRIMARY KEY REFERENCES foods(id) ON DELETE CASCADE,
    embed_text  TEXT    NOT NULL,
    embedding   TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS meals (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL,
    label       TEXT,
    eaten_at    TEXT    NOT NULL,
    notes       TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_meals_user_eaten ON meals(user_id, eaten_at);

CREATE TABLE IF NOT EXISTS meal_items (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    meal_id     INTEGER NOT NULL REFERENCES meals(id) ON DELETE CASCADE,
    food_id     INTEGER REFERENCES foods(id) ON DELETE SET NULL,
    description TEXT    NOT NULL,
    quantity    REAL    NOT NULL,
    unit        TEXT    NOT NULL,
    kcal        REAL    NOT NULL,
    protein_g   REAL    NOT NULL,
    carbs_g     REAL    NOT NULL,
    fat_g       REAL    NOT NULL,
    source      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_meal_items_meal ON meal_items(meal_id);
