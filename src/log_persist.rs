//! Background writer that stores app logs and AI traces in SQLite.
//!
//! The tracing layer and AI buffer stay synchronous; they `try_send` onto a
//! channel. This worker is the only place that touches the persist tables, and
//! it must not `tracing::error` on write failure (that would recurse into the
//! same channel). Disk-full uses the existing SQLITE_FULL circuit.

use tokio::sync::mpsc;

use crate::ai::log::AiTrace;
use crate::db::{self, Db};
use crate::logs::LogEntry;

pub fn spawn(
    pool: Db,
    mut app_rx: mpsc::Receiver<LogEntry>,
    mut ai_rx: mpsc::Receiver<AiTrace>,
    log_max: usize,
    ai_max: usize,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                item = app_rx.recv() => {
                    let Some(first) = item else {
                        while let Some(trace) = ai_rx.recv().await {
                            persist_ai(&pool, trace, ai_max).await;
                        }
                        break;
                    };
                    let mut batch = vec![first];
                    while batch.len() < 64 {
                        match app_rx.try_recv() {
                            Ok(entry) => batch.push(entry),
                            Err(_) => break,
                        }
                    }
                    persist_app_batch(&pool, &batch, log_max).await;
                }
                item = ai_rx.recv() => {
                    let Some(trace) = item else {
                        while let Some(entry) = app_rx.recv().await {
                            persist_app_batch(&pool, &[entry], log_max).await;
                        }
                        break;
                    };
                    persist_ai(&pool, trace, ai_max).await;
                }
            }
        }
    });
}

fn should_persist(entry: &LogEntry) -> bool {
    !entry.target.starts_with("sqlx")
}

async fn persist_app_batch(pool: &Db, entries: &[LogEntry], max: usize) {
    if db::sqlite_full::is_open() {
        return;
    }
    let entries: Vec<LogEntry> = entries
        .iter()
        .filter(|e| should_persist(e))
        .cloned()
        .collect();
    if entries.is_empty() {
        return;
    }
    if let Err(e) = db::app_logs::insert_many(pool, &entries).await {
        if !db::sqlite_full::silence_if_full(pool, &e).await {
            eprintln!("gregobot: persist app logs failed: {e}");
        }
        return;
    }
    if let Err(e) = db::app_logs::prune_to_max(pool, max).await
        && !db::sqlite_full::silence_if_full(pool, &e).await
    {
        eprintln!("gregobot: prune app logs failed: {e}");
    }
}

async fn persist_ai(pool: &Db, trace: AiTrace, max: usize) {
    if db::sqlite_full::is_open() {
        return;
    }
    if let Err(e) = db::ai_traces::upsert(pool, &trace).await {
        if !db::sqlite_full::silence_if_full(pool, &e).await {
            eprintln!("gregobot: persist AI trace failed: {e}");
        }
        return;
    }
    if let Err(e) = db::ai_traces::prune_to_max(pool, max).await
        && !db::sqlite_full::silence_if_full(pool, &e).await
    {
        eprintln!("gregobot: prune AI traces failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::log::{SharedAiLogBuffer, TraceBuilder};
    use crate::db::tests::test_db;
    use crate::logs::{LogEntry, LogFilter, SharedLogBuffer};
    use std::time::Duration;

    fn log_entry(message: &str) -> LogEntry {
        LogEntry {
            timestamp: "2026-09-15T00:00:00.000Z".into(),
            level: "INFO".into(),
            target: "gregobot::bot".into(),
            span_name: None,
            span_id: None,
            fields: Vec::new(),
            message: message.into(),
        }
    }

    async fn wait_app_logs(pool: &Db, pred: impl Fn(&[LogEntry]) -> bool) -> Vec<LogEntry> {
        for _ in 0..100 {
            let rows = crate::db::app_logs::load_latest(pool, 20).await.unwrap();
            if pred(&rows) {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        crate::db::app_logs::load_latest(pool, 20).await.unwrap()
    }

    #[tokio::test]
    async fn persist_worker_keeps_newest_app_logs_across_reload() {
        let (pool, path) = test_db().await;
        let (app_tx, app_rx) = mpsc::channel(16);
        let (ai_tx, ai_rx) = mpsc::channel(16);
        let buf = SharedLogBuffer::new(10);
        buf.attach_persist(app_tx);
        spawn(pool.clone(), app_rx, ai_rx, 2, 10);

        buf.push(log_entry("one"));
        buf.push(log_entry("two"));
        buf.push(log_entry("three"));

        let loaded = wait_app_logs(&pool, |rows| {
            rows.len() == 2 && rows.last().is_some_and(|e| e.message == "three")
        })
        .await;
        assert_eq!(
            loaded
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>(),
            vec!["two", "three"]
        );

        let reloaded = SharedLogBuffer::new(10);
        reloaded.prepend_persisted(loaded);
        let snap = reloaded.snapshot_filtered(10, &LogFilter::default());
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].message, "two");
        assert_eq!(snap[1].message, "three");

        drop(ai_tx);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn persist_worker_upserts_ai_traces_and_reloads() {
        let (pool, path) = test_db().await;
        let (_app_tx, app_rx) = mpsc::channel(16);
        let (ai_tx, ai_rx) = mpsc::channel(16);
        let buf = SharedAiLogBuffer::new(10);
        buf.attach_persist(ai_tx);
        spawn(pool.clone(), app_rx, ai_rx, 10, 2);

        buf.push(
            TraceBuilder::new("ask", 1, 10, "m", "sys", Vec::new())
                .with_request_id("req-1")
                .finish("one".into(), None),
        );
        buf.push(
            TraceBuilder::new("nudge", 1, 11, "m", "sys", Vec::new())
                .with_request_id("req-2")
                .finish("two".into(), None),
        );
        buf.push(
            TraceBuilder::new("ask", 1, 12, "m", "sys", Vec::new())
                .with_request_id("req-3")
                .finish("three".into(), None),
        );

        let loaded = {
            let mut rows = Vec::new();
            for _ in 0..100 {
                rows = crate::db::ai_traces::load_latest(&pool, 20).await.unwrap();
                if rows.len() == 2 && rows.last().is_some_and(|t| t.request_id == "req-3") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            rows
        };
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].request_id, "req-2");
        assert_eq!(loaded[1].request_id, "req-3");

        let reloaded = SharedAiLogBuffer::new(10);
        reloaded.load_persisted(loaded);
        let summaries = reloaded.summaries_filtered(10, &crate::ai::log::AiTraceFilter::default());
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].kind, "ask");
        assert_eq!(summaries[1].kind, "nudge");

        let _ = std::fs::remove_file(path);
    }
}
