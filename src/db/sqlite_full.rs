//! Process-wide SQLITE_FULL (code 13) circuit breaker.
//!
//! Production filled the VPS volume and then logged the same disk-full error
//! on every `ai_usage` insert and proactive record. Once tripped, background
//! ticks skip non-essential writes, we prune old rows + checkpoint the WAL,
//! and we stay open until a probe write succeeds.

use std::sync::Mutex;

use anyhow::Result;
use sqlx::SqlitePool;

const AI_USAGE_RETENTION_DAYS: i64 = 90;
const PROBE_MODEL: &str = "sqlite_full_probe";

#[derive(Debug)]
struct State {
    open: bool,
}

impl State {
    const fn new() -> Self {
        Self { open: false }
    }
}

static STATE: Mutex<State> = Mutex::new(State::new());

fn lock_state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// True when non-essential SQLite writes should be skipped.
pub fn is_open() -> bool {
    lock_state().open
}

fn trip() -> bool {
    let mut state = lock_state();
    if state.open {
        return false;
    }
    state.open = true;
    true
}

fn close() {
    lock_state().open = false;
}

pub fn looks_full(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("(code: 13)")
        || lower.contains("database or disk is full")
        || lower.contains("sqlite_full")
}

pub fn is_sqlite_full(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => db.code().as_deref() == Some("13") || looks_full(db.message()),
        other => looks_full(&other.to_string()),
    }
}

/// Trip the circuit on SQLITE_FULL, reclaim once, and return `true` so the
/// caller can skip a per-row WARN/ERROR.
pub async fn silence_if_full(pool: &SqlitePool, err: &impl std::fmt::Display) -> bool {
    if !looks_full(&err.to_string()) {
        return false;
    }
    let first = trip();
    if first {
        tracing::error!(
            error = %err,
            "SQLITE_FULL: database or disk is full — skipping non-essential writes until space is reclaimed"
        );
        reclaim(pool).await;
    }
    true
}

/// If the circuit is open, try to reclaim space. Returns `true` when the
/// caller should skip the rest of a background tick.
pub async fn skip_background_tick(pool: &SqlitePool, name: &str) -> bool {
    if !is_open() {
        return false;
    }
    reclaim(pool).await;
    if is_open() {
        tracing::debug!("{name}: skipping tick — SQLite disk-full circuit open");
        true
    } else {
        false
    }
}

pub async fn prune_old_ai_usage(pool: &SqlitePool) -> Result<u64> {
    crate::db::ai_usage::prune_older_than_days(pool, AI_USAGE_RETENTION_DAYS).await
}

pub async fn checkpoint_wal(pool: &SqlitePool) -> Result<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;
    Ok(())
}

async fn reclaim(pool: &SqlitePool) {
    match prune_old_ai_usage(pool).await {
        Ok(deleted) => {
            if deleted > 0 {
                tracing::warn!(
                    deleted,
                    days = AI_USAGE_RETENTION_DAYS,
                    "SQLITE_FULL: pruned old ai_usage rows"
                );
            }
        }
        Err(e) => {
            if !looks_full(&e.to_string()) {
                tracing::warn!(error = %e, "SQLITE_FULL: ai_usage prune failed");
            }
        }
    }

    if let Err(e) = crate::db::conversations::prune(pool).await
        && !looks_full(&e.to_string())
    {
        tracing::warn!(error = %e, "SQLITE_FULL: conversation prune failed");
    }

    match crate::db::app_logs::prune_to_max(pool, 200).await {
        Ok(deleted) if deleted > 0 => {
            tracing::warn!(deleted, "SQLITE_FULL: pruned app_logs down to 200");
        }
        Ok(_) => {}
        Err(e) if !looks_full(&e.to_string()) => {
            tracing::warn!(error = %e, "SQLITE_FULL: app_logs prune failed");
        }
        Err(_) => {}
    }

    match crate::db::ai_traces::prune_to_max(pool, 20).await {
        Ok(deleted) if deleted > 0 => {
            tracing::warn!(deleted, "SQLITE_FULL: pruned ai_traces down to 20");
        }
        Ok(_) => {}
        Err(e) if !looks_full(&e.to_string()) => {
            tracing::warn!(error = %e, "SQLITE_FULL: ai_traces prune failed");
        }
        Err(_) => {}
    }

    if let Err(e) = checkpoint_wal(pool).await
        && !looks_full(&e.to_string())
    {
        tracing::warn!(error = %e, "SQLITE_FULL: WAL checkpoint failed");
    }

    if probe_writable(pool).await {
        if is_open() {
            tracing::warn!("SQLITE_FULL circuit closed after reclaim");
        }
        close();
    }
}

async fn probe_writable(pool: &SqlitePool) -> bool {
    let insert = sqlx::query(
        "INSERT INTO ai_usage (user_id, model, prompt_tokens, completion_tokens, total_tokens, cached_tokens, image_tokens, kind) \
         SELECT id, ?, 0, 0, 0, 0, 0, 'unknown' FROM users ORDER BY id LIMIT 1",
    )
    .bind(PROBE_MODEL)
    .execute(pool)
    .await;

    match insert {
        Ok(res) if res.rows_affected() > 0 => {
            let _ = sqlx::query("DELETE FROM ai_usage WHERE model = ?")
                .bind(PROBE_MODEL)
                .execute(pool)
                .await;
            true
        }
        Ok(_) => {
            // No users yet — try a tiny side table so we still detect a full volume.
            probe_side_table(pool).await
        }
        Err(e) => !looks_full(&e.to_string()) && !is_sqlite_full(&e),
    }
}

async fn probe_side_table(pool: &SqlitePool) -> bool {
    if let Err(e) = sqlx::query("CREATE TABLE IF NOT EXISTS sqlite_full_probe (x INTEGER NOT NULL)")
        .execute(pool)
        .await
    {
        return !looks_full(&e.to_string());
    }
    match sqlx::query("INSERT INTO sqlite_full_probe (x) VALUES (1)")
        .execute(pool)
        .await
    {
        Ok(_) => {
            let _ = sqlx::query("DELETE FROM sqlite_full_probe")
                .execute(pool)
                .await;
            true
        }
        Err(e) => !looks_full(&e.to_string()),
    }
}

#[cfg(test)]
pub fn reset_for_tests() {
    *lock_state() = State::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ai_usage::{self, UsageTokens};
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn detects_code_13_message() {
        assert!(looks_full(
            "error returned from database: (code: 13) database or disk is full"
        ));
        assert!(looks_full("SQLITE_FULL"));
        assert!(!looks_full(
            "error returned from database: (code: 5) database is locked"
        ));
    }

    #[test]
    fn trip_alerts_only_once() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        assert!(!is_open());
        assert!(trip());
        assert!(is_open());
        assert!(!trip());
        assert!(is_open());
        close();
        assert!(!is_open());
        reset_for_tests();
    }

    #[tokio::test]
    async fn silence_ignores_other_errors() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        let (pool, path) = test_db().await;
        assert!(!silence_if_full(&pool, &"UNIQUE constraint failed").await);
        assert!(!is_open());
        let _ = std::fs::remove_file(path);
        reset_for_tests();
    }

    #[tokio::test]
    async fn reclaim_prunes_old_usage_and_closes() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_tests();
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            88001,
            Role::Sportive,
            Some("full".into()),
            None,
            None,
        )
        .await
        .unwrap();
        ai_usage::insert_detailed(
            &pool,
            user.id,
            "old-model",
            UsageTokens {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                cached_tokens: 0,
                image_tokens: 0,
            },
            ai_usage::KIND_ASK,
            None,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE ai_usage SET created_at = datetime('now', '-120 days') WHERE user_id = ?",
        )
        .bind(user.id)
        .execute(&pool)
        .await
        .unwrap();

        assert!(trip());
        reclaim(&pool).await;
        assert!(!is_open());

        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage WHERE model = 'old-model'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(left, 0);

        let _ = std::fs::remove_file(path);
        reset_for_tests();
    }
}
