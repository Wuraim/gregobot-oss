CREATE TABLE IF NOT EXISTS chat_settings (
    chat_id BIGINT NOT NULL PRIMARY KEY,
    sport_reaction_emoji TEXT NOT NULL DEFAULT '🏆'
);
