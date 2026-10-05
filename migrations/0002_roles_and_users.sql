-- Roles lookup table (fixed values, seeded here)
CREATE TABLE IF NOT EXISTS roles (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT    NOT NULL UNIQUE
);

INSERT OR IGNORE INTO roles (name) VALUES ('sportive');
INSERT OR IGNORE INTO roles (name) VALUES ('admin');

-- Registered bot users
CREATE TABLE IF NOT EXISTS users (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    telegram_user_id  INTEGER NOT NULL UNIQUE,
    role_id           INTEGER NOT NULL REFERENCES roles(id)
);
