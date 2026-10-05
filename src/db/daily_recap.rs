use anyhow::Result;
use sqlx::SqlitePool;

use crate::db::users::Role;
use crate::db::users::User;

pub async fn set_daily_recap(
    pool: &SqlitePool,
    telegram_user_id: i64,
    enabled: bool,
    hour: i64,
    minute: i64,
) -> Result<bool> {
    let enabled_i64: i64 = if enabled { 1 } else { 0 };
    let result = sqlx::query!(
        "UPDATE users SET daily_recap_enabled = ?, daily_recap_hour = ?, daily_recap_minute = ? WHERE telegram_user_id = ?",
        enabled_i64,
        hour,
        minute,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_daily_recap_enabled(
    pool: &SqlitePool,
    telegram_user_id: i64,
    enabled: bool,
) -> Result<bool> {
    let enabled_i64: i64 = if enabled { 1 } else { 0 };
    let result = sqlx::query!(
        "UPDATE users SET daily_recap_enabled = ? WHERE telegram_user_id = ?",
        enabled_i64,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_daily_recap_sent(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET last_daily_recap_at = datetime('now') WHERE id = ?",
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_daily_recap_due(pool: &SqlitePool) -> Result<Vec<User>> {
    let rows = sqlx::query!(
        r#"
        SELECT u.id, u.telegram_user_id, u.username, u.first_name, u.language_code, u.description,
               u.timezone,
               u.web_search_enabled, u.daily_recap_enabled, u.daily_recap_hour,
               u.daily_recap_minute, u.last_daily_recap_at,
               u.nudges_enabled, u.nudge_inactivity_days, u.last_nudge_at,
               r.name AS role_name
        FROM users u
        JOIN roles r ON r.id = u.role_id
        WHERE u.daily_recap_enabled = 1
          AND r.name != 'pending'
          AND (u.last_daily_recap_at IS NULL OR date(u.last_daily_recap_at) < date('now'))
        ORDER BY u.id
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(User {
                id: r.id,
                telegram_user_id: r.telegram_user_id,
                role: Role::try_from(r.role_name)?,
                username: r.username,
                first_name: r.first_name,
                language_code: r.language_code,
                description: r.description,
                timezone: r.timezone.unwrap_or_else(|| "UTC".to_string()),
                web_search_enabled: r.web_search_enabled != 0,
                daily_recap_enabled: r.daily_recap_enabled != 0,
                daily_recap_hour: r.daily_recap_hour,
                daily_recap_minute: r.daily_recap_minute,
                last_daily_recap_at: r.last_daily_recap_at,
                nudges_enabled: r.nudges_enabled != 0,
                nudge_inactivity_days: r.nudge_inactivity_days,
                last_nudge_at: r.last_nudge_at,
            })
        })
        .collect()
}
