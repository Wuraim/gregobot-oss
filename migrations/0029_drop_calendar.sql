-- Remove calendar feature: external sync connections and local events.
-- Cancel any reminders that targeted calendar events first.
DELETE FROM reminders WHERE target_type = 'event';
DROP TABLE IF EXISTS calendar_events;
DROP TABLE IF EXISTS calendar_connections;
