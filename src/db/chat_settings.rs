use anyhow::Result;
use sqlx::SqlitePool;

const DEFAULT_SPORT_REACTION_EMOJI: &str = "🏆";

pub async fn get_sport_emoji(pool: &SqlitePool, chat_id: i64) -> String {
    sqlx::query_scalar!(
        "SELECT sport_reaction_emoji FROM chat_settings WHERE chat_id = ?",
        chat_id
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| DEFAULT_SPORT_REACTION_EMOJI.to_string())
}

pub async fn set_sport_emoji(pool: &SqlitePool, chat_id: i64, emoji: &str) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO chat_settings (chat_id, sport_reaction_emoji)
        VALUES (?, ?)
        ON CONFLICT(chat_id) DO UPDATE SET sport_reaction_emoji = excluded.sport_reaction_emoji
        "#,
        chat_id,
        emoji
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_monthly_training_graph_enabled(pool: &SqlitePool, chat_id: i64) -> bool {
    sqlx::query_scalar!(
        "SELECT monthly_training_graph_enabled FROM chat_settings WHERE chat_id = ?",
        chat_id
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|v| v != 0)
    .unwrap_or(false)
}

pub async fn set_monthly_training_graph_enabled(
    pool: &SqlitePool,
    chat_id: i64,
    enabled: bool,
) -> Result<()> {
    let flag = if enabled { 1 } else { 0 };
    sqlx::query!(
        r#"
        INSERT INTO chat_settings (chat_id, monthly_training_graph_enabled)
        VALUES (?, ?)
        ON CONFLICT(chat_id) DO UPDATE SET
            monthly_training_graph_enabled = excluded.monthly_training_graph_enabled
        "#,
        chat_id,
        flag
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_monthly_training_graph_sent(pool: &SqlitePool, chat_id: i64) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE chat_settings
        SET last_monthly_training_graph_at = datetime('now')
        WHERE chat_id = ?
        "#,
        chat_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Chats with monthly graph enabled that have not been sent yet this calendar month (UTC).
pub async fn list_monthly_training_graph_due(pool: &SqlitePool) -> Result<Vec<i64>> {
    let rows = sqlx::query_scalar!(
        r#"
        SELECT chat_id
        FROM chat_settings
        WHERE monthly_training_graph_enabled = 1
          AND (
            last_monthly_training_graph_at IS NULL
            OR strftime('%Y-%m', last_monthly_training_graph_at) < strftime('%Y-%m', 'now')
          )
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
