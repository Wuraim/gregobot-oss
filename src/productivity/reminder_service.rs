use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::db;
use crate::productivity::domain::{
    self, Reminder, ReminderRecurrence, ReminderStatus, ReminderTarget,
};

pub async fn create_reminder(
    pool: &SqlitePool,
    user_id: i64,
    target: ReminderTarget,
    scheduled_at: DateTime<Utc>,
    recurrence: ReminderRecurrence,
) -> Result<Reminder> {
    if scheduled_at <= Utc::now() {
        return Err(anyhow!("Reminder time must be in the future"));
    }

    let (target_type, target_id, custom_message) = match &target {
        ReminderTarget::Task { task_id } => (
            db::reminders::ReminderTargetType::Task,
            Some(*task_id),
            None,
        ),
        ReminderTarget::Custom { message } => (
            db::reminders::ReminderTargetType::Custom,
            None,
            Some(message.clone()),
        ),
    };

    let new = db::reminders::NewReminder {
        user_id,
        target_type,
        target_id,
        custom_message,
        scheduled_at,
        recurrence: recurrence.as_str().to_string(),
    };

    let row = db::reminders::insert(pool, &new).await?;
    Ok(domain::parse_reminder(&row))
}

pub async fn list_reminders(
    pool: &SqlitePool,
    user_id: i64,
    status: Option<ReminderStatus>,
) -> Result<Vec<Reminder>> {
    let status_str = status.map(|s| s.as_str());
    let rows = db::reminders::list_for_user(pool, user_id, status_str, 100).await?;
    Ok(domain::parse_reminders(&rows))
}

pub async fn cancel_reminder(pool: &SqlitePool, reminder_id: i64, user_id: i64) -> Result<bool> {
    let reminder = match db::reminders::find_by_id(pool, reminder_id).await? {
        Some(r) => r,
        None => return Ok(false),
    };
    if reminder.user_id != user_id {
        return Err(anyhow!("Not your reminder"));
    }
    db::reminders::cancel(pool, reminder_id, user_id).await
}

pub async fn update_reminder_time(
    pool: &SqlitePool,
    reminder_id: i64,
    user_id: i64,
    scheduled_at: DateTime<Utc>,
) -> Result<bool> {
    if scheduled_at <= Utc::now() {
        return Err(anyhow!("Reminder time must be in the future"));
    }

    db::reminders::update_scheduled_at(pool, reminder_id, user_id, scheduled_at).await
}

pub async fn list_due(pool: &SqlitePool, limit: i64) -> Result<Vec<Reminder>> {
    let now = Utc::now().to_rfc3339();
    let rows = db::reminders::list_due(pool, &now, limit).await?;
    Ok(domain::parse_reminders(&rows))
}

pub async fn mark_fired(pool: &SqlitePool, reminder_id: i64) -> Result<bool> {
    let reminder = match db::reminders::find_by_id(pool, reminder_id).await? {
        Some(r) => domain::parse_reminder(&r),
        None => return Ok(false),
    };

    let fired = db::reminders::mark_fired(pool, reminder_id).await?;
    if !fired {
        return Ok(false);
    }

    if reminder.recurrence != ReminderRecurrence::None {
        let should_reschedule = match &reminder.target {
            ReminderTarget::Task { task_id } => crate::db::tasks::find_by_id(pool, *task_id)
                .await?
                .is_some(),
            ReminderTarget::Custom { .. } => true,
        };

        if should_reschedule {
            let mut next_at = reminder.recurrence.next_occurrence(reminder.scheduled_at);
            while next_at <= Utc::now() {
                next_at = reminder.recurrence.next_occurrence(next_at);
            }
            if let Err(e) = create_reminder(
                pool,
                reminder.user_id,
                reminder.target.clone(),
                next_at,
                reminder.recurrence,
            )
            .await
            {
                tracing::error!(
                    "Failed to reschedule recurring reminder {}: {e}",
                    reminder.id
                );
            }
        }
    }

    Ok(true)
}

pub async fn increment_attempts(pool: &SqlitePool, reminder_id: i64) -> Result<()> {
    db::reminders::increment_attempts(pool, reminder_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::users::Role;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect test db");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    #[tokio::test]
    async fn update_reminder_time_moves_pending_reminder() {
        let pool = test_pool().await;
        let user = crate::db::users::insert(&pool, 42, Role::Sportive, None, None, None)
            .await
            .expect("insert user");
        let now = Utc::now();

        let reminder = create_reminder(
            &pool,
            user.id,
            ReminderTarget::Custom {
                message: "Carte vitale".to_string(),
            },
            now + chrono::Duration::hours(1),
            ReminderRecurrence::None,
        )
        .await
        .expect("create reminder");

        let new_time = now + chrono::Duration::hours(2);
        let updated = update_reminder_time(&pool, reminder.id, user.id, new_time)
            .await
            .expect("update reminder");

        assert!(updated);

        let rows = db::reminders::find_by_id(&pool, reminder.id)
            .await
            .expect("reload reminder")
            .expect("reminder exists");
        let parsed = domain::parse_reminder(&rows);
        assert_eq!(parsed.scheduled_at.timestamp(), new_time.timestamp());
    }
}
