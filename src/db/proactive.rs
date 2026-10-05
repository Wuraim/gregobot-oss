use anyhow::Result;
use sqlx::SqlitePool;

pub async fn is_enabled(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let enabled: i64 = sqlx::query_scalar("SELECT proactive_enabled FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(1);
    Ok(enabled != 0)
}

pub async fn disable(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let result = sqlx::query("UPDATE users SET proactive_enabled = 0 WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn can_send_today(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let row = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM proactive_interactions
        WHERE user_id = ?
          AND sent_at > datetime('now', '-24 hours')
        "#,
        user_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(row.count == 0)
}

pub async fn record(pool: &SqlitePool, user_id: i64, message: Option<String>) -> Result<()> {
    sqlx::query!(
        "INSERT INTO proactive_interactions (user_id, message) VALUES (?, ?)",
        user_id,
        message,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert};

    #[tokio::test]
    async fn new_users_have_proactive_enabled() {
        let (pool, path) = test_db().await;
        let user = insert(&pool, 55001, Role::Sportive, Some("p".into()), None, None)
            .await
            .unwrap();
        assert!(is_enabled(&pool, user.id).await.unwrap());
        assert!(disable(&pool, user.id).await.unwrap());
        assert!(!is_enabled(&pool, user.id).await.unwrap());
        let _ = std::fs::remove_file(&path);
    }
}
