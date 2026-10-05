use anyhow::{Context, Result};
use sqlx::FromRow;

use crate::ai::log::AiTrace;
use crate::db::Db;

#[derive(Debug, FromRow)]
struct AiTraceRow {
    payload: String,
}

pub async fn upsert(pool: &Db, trace: &AiTrace) -> Result<()> {
    let payload = serde_json::to_string(trace).context("failed to serialize AI trace")?;
    sqlx::query(
        r#"
        INSERT INTO ai_traces (request_id, timestamp, kind, telegram_user_id, payload)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(request_id) DO UPDATE SET
            timestamp = excluded.timestamp,
            kind = excluded.kind,
            telegram_user_id = excluded.telegram_user_id,
            payload = excluded.payload
        "#,
    )
    .bind(&trace.request_id)
    .bind(&trace.timestamp)
    .bind(&trace.kind)
    .bind(trace.telegram_user_id)
    .bind(&payload)
    .execute(pool)
    .await
    .context("failed to upsert AI trace")?;
    Ok(())
}

/// Keep only the newest `max` rows. Returns how many were deleted.
pub async fn prune_to_max(pool: &Db, max: usize) -> Result<u64> {
    let max = max.max(1) as i64;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_traces")
        .fetch_one(pool)
        .await
        .context("failed to count ai_traces")?;
    if count <= max {
        return Ok(0);
    }
    let extra = count - max;
    let result = sqlx::query(
        "DELETE FROM ai_traces WHERE id IN (SELECT id FROM ai_traces ORDER BY id ASC LIMIT ?)",
    )
    .bind(extra)
    .execute(pool)
    .await
    .context("failed to prune ai_traces")?;
    Ok(result.rows_affected())
}

/// Oldest first, at most `max` newest rows.
pub async fn load_latest(pool: &Db, max: usize) -> Result<Vec<AiTrace>> {
    let max = max.max(1) as i64;
    let rows = sqlx::query_as::<_, AiTraceRow>(
        r#"
        SELECT payload
        FROM (
            SELECT payload, id
            FROM ai_traces
            ORDER BY id DESC
            LIMIT ?
        )
        ORDER BY id ASC
        "#,
    )
    .bind(max)
    .fetch_all(pool)
    .await
    .context("failed to load ai_traces")?;

    let mut traces = Vec::with_capacity(rows.len());
    for row in rows {
        match serde_json::from_str::<AiTrace>(&row.payload) {
            Ok(trace) => traces.push(trace),
            Err(e) => {
                tracing::warn!("Skipping unreadable persisted AI trace: {e}");
            }
        }
    }
    Ok(traces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::log::TraceBuilder;
    use crate::db::tests::test_db;

    fn trace(request_id: &str, kind: &'static str) -> AiTrace {
        TraceBuilder::new(kind, 1, 100, "m", "sys", Vec::new())
            .with_request_id(request_id)
            .finish(format!("{kind}-ok"), None)
    }

    #[tokio::test]
    async fn upsert_updates_same_request_and_prune_keeps_newest() {
        let (pool, path) = test_db().await;

        let mut first = trace("req-1", "ask");
        first.id = 1;
        upsert(&pool, &first).await.unwrap();
        first.final_text = "updated".into();
        upsert(&pool, &first).await.unwrap();

        upsert(&pool, &{
            let mut t = trace("req-2", "nudge");
            t.id = 2;
            t
        })
        .await
        .unwrap();
        upsert(&pool, &{
            let mut t = trace("req-3", "proactive");
            t.id = 3;
            t
        })
        .await
        .unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_traces")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 3);

        let deleted = prune_to_max(&pool, 2).await.unwrap();
        assert_eq!(deleted, 1);

        let loaded = load_latest(&pool, 10).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].request_id, "req-2");
        assert_eq!(loaded[1].request_id, "req-3");
        assert_eq!(loaded[0].kind, "nudge");

        let _ = std::fs::remove_file(path);
    }
}
