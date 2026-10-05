-- Add recurrence support to reminders.
ALTER TABLE reminders ADD COLUMN recurrence TEXT NOT NULL DEFAULT 'none'
  CHECK (recurrence IN ('none', 'daily', 'weekly', 'monthly'));
