use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderTargetType {
    Task,
    Custom,
}

impl ReminderTargetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub user_id: i64,
    pub target_type: String,
    pub target_id: Option<i64>,
    pub custom_message: Option<String>,
    pub scheduled_at: String,
    pub status: String,
    pub created_at: String,
    pub fired_at: Option<String>,
    pub attempts: i64,
    pub recurrence: String,
}

#[derive(Debug, Deserialize)]
pub struct NewReminder {
    pub user_id: i64,
    pub target_type: ReminderTargetType,
    pub target_id: Option<i64>,
    pub custom_message: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub recurrence: String,
}

pub async fn insert(pool: &SqlitePool, new: &NewReminder) -> Result<Reminder> {
    let scheduled_at_str = new.scheduled_at.to_rfc3339();
    let target_type_str = new.target_type.as_str();

    let recurrence = new.recurrence.as_str();
    let id = sqlx::query!(
        r#"
        INSERT INTO reminders (user_id, target_type, target_id, custom_message, scheduled_at, recurrence)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        new.user_id,
        target_type_str,
        new.target_id,
        new.custom_message,
        scheduled_at_str,
        recurrence,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    let row = sqlx::query!(
        r#"
        SELECT id, user_id, target_type, target_id, custom_message, scheduled_at, status, created_at, fired_at, attempts, recurrence
        FROM reminders WHERE id = ?
        "#,
        id,
    )
    .fetch_one(pool)
    .await?;

    Ok(Reminder {
        id: row.id,
        user_id: row.user_id,
        target_type: row.target_type,
        target_id: row.target_id,
        custom_message: row.custom_message,
        scheduled_at: row.scheduled_at,
        status: row.status,
        created_at: row.created_at,
        fired_at: row.fired_at,
        attempts: row.attempts,
        recurrence: row.recurrence,
    })
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Reminder>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, target_type, target_id, custom_message, scheduled_at, status, created_at, fired_at, attempts, recurrence
        FROM reminders WHERE id = ?
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Reminder {
        id: r.id,
        user_id: r.user_id,
        target_type: r.target_type,
        target_id: r.target_id,
        custom_message: r.custom_message,
        scheduled_at: r.scheduled_at,
        status: r.status,
        created_at: r.created_at,
        fired_at: r.fired_at,
        attempts: r.attempts,
        recurrence: r.recurrence,
    }))
}

pub async fn list_for_user(
    pool: &SqlitePool,
    user_id: i64,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<Reminder>> {
    if let Some(status) = status {
        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, target_type, target_id, custom_message, scheduled_at, status, created_at, fired_at, attempts, recurrence
            FROM reminders
            WHERE user_id = ? AND status = ?
            ORDER BY scheduled_at DESC
            LIMIT ?
            "#,
            user_id,
            status,
            limit,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Reminder {
                id: r.id,
                user_id: r.user_id,
                target_type: r.target_type,
                target_id: r.target_id,
                custom_message: r.custom_message,
                scheduled_at: r.scheduled_at,
                status: r.status,
                created_at: r.created_at,
                fired_at: r.fired_at,
                attempts: r.attempts,
                recurrence: r.recurrence,
            })
            .collect())
    } else {
        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, target_type, target_id, custom_message, scheduled_at, status, created_at, fired_at, attempts, recurrence
            FROM reminders
            WHERE user_id = ?
            ORDER BY scheduled_at DESC
            LIMIT ?
            "#,
            user_id,
            limit,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Reminder {
                id: r.id,
                user_id: r.user_id,
                target_type: r.target_type,
                target_id: r.target_id,
                custom_message: r.custom_message,
                scheduled_at: r.scheduled_at,
                status: r.status,
                created_at: r.created_at,
                fired_at: r.fired_at,
                attempts: r.attempts,
                recurrence: r.recurrence,
            })
            .collect())
    }
}

pub async fn list_due(pool: &SqlitePool, now: &str, limit: i64) -> Result<Vec<Reminder>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, target_type, target_id, custom_message, scheduled_at, status, created_at, fired_at, attempts, recurrence
        FROM reminders
        WHERE status = 'pending' AND scheduled_at <= ?
        ORDER BY scheduled_at ASC
        LIMIT ?
        "#,
        now,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Reminder {
            id: r.id.expect("id"),
            user_id: r.user_id,
            target_type: r.target_type,
            target_id: r.target_id,
            custom_message: r.custom_message,
            scheduled_at: r.scheduled_at,
            status: r.status,
            created_at: r.created_at,
            fired_at: r.fired_at,
            attempts: r.attempts,
            recurrence: r.recurrence,
        })
        .collect())
}

pub async fn mark_fired(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE reminders
        SET status = 'fired', fired_at = datetime('now')
        WHERE id = ? AND status = 'pending'
        "#,
        id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update_scheduled_at(
    pool: &SqlitePool,
    id: i64,
    user_id: i64,
    scheduled_at: DateTime<Utc>,
) -> Result<bool> {
    let scheduled_at_str = scheduled_at.to_rfc3339();
    let result = sqlx::query!(
        r#"
        UPDATE reminders
        SET scheduled_at = ?
        WHERE id = ? AND user_id = ? AND status = 'pending'
        "#,
        scheduled_at_str,
        id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn mark_failed(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query!("UPDATE reminders SET status = 'failed' WHERE id = ?", id,)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn cancel(pool: &SqlitePool, id: i64, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE reminders SET status = 'cancelled' WHERE id = ? AND user_id = ?",
        id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn cancel_for_target(
    pool: &SqlitePool,
    target_type: &str,
    target_id: i64,
) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE reminders
        SET status = 'cancelled'
        WHERE target_type = ? AND target_id = ? AND status = 'pending'
        "#,
        target_type,
        target_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn increment_attempts(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE reminders SET attempts = attempts + 1 WHERE id = ?",
        id,
    )
    .execute(pool)
    .await?;

    Ok(())
}
