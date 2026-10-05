-- Calendar connections for external calendar sync (CalDAV, Google)
CREATE TABLE IF NOT EXISTS calendar_connections (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL CHECK (provider IN ('caldav','google')),
    label           TEXT NOT NULL DEFAULT 'Default',
    -- CalDAV: server URL. Google: calendar ID (or 'primary')
    connection_url  TEXT,
    -- CalDAV: username for Basic auth. Google: not used (OAuth2).
    username        TEXT,
    -- CalDAV: password. Google: encrypted refresh token.
    encrypted_secret TEXT,
    -- OAuth2 access token + expiry (Google only, JSON blob)
    access_token    TEXT,
    token_expiry    TEXT,
    -- Delta sync token (opaque string from upstream provider)
    sync_token      TEXT,
    -- Whether this connection participates in auto-sync
    is_active       INTEGER NOT NULL DEFAULT 1,
    last_synced_at  TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_calendar_connections_user ON calendar_connections(user_id, is_active);
