use anyhow::{Context, Result};
use sqlx::FromRow;

use crate::db::Db;
use crate::logs::LogEntry;

#[derive(Debug, FromRow)]
struct AppLogRow {
    timestamp: String,
    level: String,
    target: String,
    span_name: Option<String>,
    span_id: Option<String>,
    fields: String,
    message: String,
}

impl From<AppLogRow> for LogEntry {
    fn from(row: AppLogRow) -> Self {
        let fields = serde_json::from_str(&row.fields).unwrap_or_default();
        LogEntry {
            timestamp: row.timestamp,
            level: row.level,
            target: row.target,
            span_name: row.span_name,
            span_id: row.span_id,
            fields,
            message: row.message,
        }
    }
}

pub async fn insert_many(pool: &Db, entries: &[LogEntry]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin app_logs insert")?;
    for entry in entries {
        let fields = serde_json::to_string(&entry.fields).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            r#"
            INSERT INTO app_logs (timestamp, level, target, span_name, span_id, fields, message)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry.timestamp)
        .bind(&entry.level)
        .bind(&entry.target)
        .bind(&entry.span_name)
        .bind(&entry.span_id)
        .bind(&fields)
        .bind(&entry.message)
        .execute(&mut *tx)
        .await
        .context("failed to insert app log")?;
    }
    tx.commit()
        .await
        .context("failed to commit app_logs insert")?;
    Ok(())
}

/// Keep only the newest `max` rows. Returns how many were deleted.
pub async fn prune_to_max(pool: &Db, max: usize) -> Result<u64> {
    let max = max.max(1) as i64;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_logs")
        .fetch_one(pool)
        .await
        .context("failed to count app_logs")?;
    if count <= max {
        return Ok(0);
    }
    let extra = count - max;
    let result = sqlx::query(
        "DELETE FROM app_logs WHERE id IN (SELECT id FROM app_logs ORDER BY id ASC LIMIT ?)",
    )
    .bind(extra)
    .execute(pool)
    .await
    .context("failed to prune app_logs")?;
    Ok(result.rows_affected())
}

/// Oldest first, at most `max` newest rows.
pub async fn load_latest(pool: &Db, max: usize) -> Result<Vec<LogEntry>> {
    let max = max.max(1) as i64;
    let rows = sqlx::query_as::<_, AppLogRow>(
        r#"
        SELECT timestamp, level, target, span_name, span_id, fields, message
        FROM (
            SELECT timestamp, level, target, span_name, span_id, fields, message, id
            FROM app_logs
            ORDER BY id DESC
            LIMIT ?
        )
        ORDER BY id ASC
        "#,
    )
    .bind(max)
    .fetch_all(pool)
    .await
    .context("failed to load app_logs")?;
    Ok(rows.into_iter().map(LogEntry::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::logs::LogEntry;

    fn entry(ts: &str, message: &str) -> LogEntry {
        LogEntry {
            timestamp: ts.into(),
            level: "INFO".into(),
            target: "gregobot::bot".into(),
            span_name: None,
            span_id: None,
            fields: vec![("k".into(), "v".into())],
            message: message.into(),
        }
    }

    #[tokio::test]
    async fn insert_load_and_prune_keeps_newest() {
        let (pool, path) = test_db().await;
        insert_many(
            &pool,
            &[
                entry("2026-09-15T00:00:00.000Z", "one"),
                entry("2026-09-15T00:00:01.000Z", "two"),
                entry("2026-09-15T00:00:02.000Z", "three"),
                entry("2026-09-15T00:00:03.000Z", "four"),
            ],
        )
        .await
        .unwrap();

        let deleted = prune_to_max(&pool, 2).await.unwrap();
        assert_eq!(deleted, 2);

        let loaded = load_latest(&pool, 10).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].message, "three");
        assert_eq!(loaded[1].message, "four");
        assert_eq!(loaded[0].fields, vec![("k".into(), "v".into())]);

        let _ = std::fs::remove_file(path);
    }
}
