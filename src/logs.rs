use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::Subscriber;
use tracing::field::Visit;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Default cap for the in-memory buffer and the SQLite persist table.
pub const DEFAULT_MAX_ENTRIES: usize = 5_000;

pub fn max_entries_from_env() -> usize {
    parse_positive_usize("LOG_MAX_ENTRIES", DEFAULT_MAX_ENTRIES)
}

pub(crate) fn parse_positive_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub span_name: Option<String>,
    pub span_id: Option<String>,
    pub fields: Vec<(String, String)>,
    pub message: String,
}

pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    max_entries: usize,
}

impl LogBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn prepend_persisted(&mut self, old: Vec<LogEntry>) {
        let current: Vec<LogEntry> = self.entries.drain(..).collect();
        self.entries.extend(old);
        self.entries.extend(current);
        while self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    #[cfg(test)]
    fn snapshot_all(&self) -> Vec<LogEntry> {
        self.entries.iter().cloned().collect()
    }

    pub fn snapshot_filtered(&self, lines: usize, filter: &LogFilter) -> Vec<LogEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| filter.matches(e))
            .take(lines)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

/// Server-side filters for `GET /api/logs`. Empty fields match everything.
#[derive(Clone, Debug, Default)]
pub struct LogFilter {
    pub levels: Vec<String>,
    pub target: Option<String>,
    pub q: Option<String>,
}

impl LogFilter {
    pub fn from_query(level: Option<&str>, target: Option<&str>, q: Option<&str>) -> Self {
        let levels = level
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self {
            levels,
            target: nonempty(target),
            q: nonempty(q),
        }
    }

    pub fn matches(&self, entry: &LogEntry) -> bool {
        if !self.levels.is_empty()
            && !self
                .levels
                .iter()
                .any(|lvl| lvl.eq_ignore_ascii_case(&entry.level))
        {
            return false;
        }
        if let Some(ref needle) = self.target
            && !entry
                .target
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        {
            return false;
        }
        if let Some(ref needle) = self.q {
            let needle = needle.to_ascii_lowercase();
            let in_message = entry.message.to_ascii_lowercase().contains(&needle);
            let in_target = entry.target.to_ascii_lowercase().contains(&needle);
            let in_fields = entry
                .fields
                .iter()
                .any(|(_, v)| v.to_ascii_lowercase().contains(&needle));
            if !(in_message || in_target || in_fields) {
                return false;
            }
        }
        true
    }
}

fn nonempty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A thread-safe handle to the in-memory ring buffer.
#[derive(Clone)]
pub struct SharedLogBuffer {
    inner: Arc<Mutex<LogBuffer>>,
    persist: Arc<Mutex<Option<mpsc::Sender<LogEntry>>>>,
    max_entries: usize,
}

impl SharedLogBuffer {
    pub fn new(max_entries: usize) -> Self {
        let max_entries = max_entries.max(1);
        Self {
            inner: Arc::new(Mutex::new(LogBuffer::new(max_entries))),
            persist: Arc::new(Mutex::new(None)),
            max_entries,
        }
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn attach_persist(&self, tx: mpsc::Sender<LogEntry>) {
        if let Ok(mut persist) = self.persist.lock() {
            *persist = Some(tx);
        }
    }

    pub fn prepend_persisted(&self, old: Vec<LogEntry>) {
        if let Ok(mut buf) = self.inner.lock() {
            buf.prepend_persisted(old);
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let persist = self
            .persist
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().cloned());
        if let Ok(mut buf) = self.inner.lock() {
            buf.push(entry.clone());
        }
        if let Some(tx) = persist {
            let _ = tx.try_send(entry);
        }
    }

    pub fn snapshot_filtered(&self, lines: usize, filter: &LogFilter) -> Vec<LogEntry> {
        self.inner
            .lock()
            .map(|buf| buf.snapshot_filtered(lines, filter))
            .unwrap_or_default()
    }
}

/// A `tracing` layer that captures structured log events into the shared ring buffer.
pub struct StructuredLogLayer {
    buffer: SharedLogBuffer,
}

impl StructuredLogLayer {
    pub fn new(buffer: SharedLogBuffer) -> Self {
        Self { buffer }
    }
}

impl<S> Layer<S> for StructuredLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let timestamp = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        let level = event.metadata().level().to_string();
        let target = event.metadata().target().to_string();

        let (span_name, span_id) = ctx
            .current_span()
            .id()
            .and_then(|id| {
                ctx.span(id).map(|span| {
                    let name = span.name().to_string();
                    let id_str = format!("{:x}", (*id).into_u64());
                    (Some(name), Some(id_str))
                })
            })
            .unwrap_or((None, None));

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp,
            level,
            target,
            span_name,
            span_id,
            fields: visitor.fields,
            message: visitor.message,
        };

        self.buffer.push(entry);
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Vec<(String, String)>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn Debug) {
        let value_str = format!("{:?}", value);
        self.store(field.name(), value_str);
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.store(field.name(), value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.store(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.store(field.name(), value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.store(field.name(), value.to_string());
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.store(field.name(), value.to_string());
    }
}

impl FieldVisitor {
    fn store(&mut self, name: &str, value: String) {
        if name == "message" {
            self.message = value;
        } else {
            self.fields.push((name.to_string(), value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: &str, target: &str, message: &str) -> LogEntry {
        LogEntry {
            timestamp: "2026-08-25T00:00:00.000Z".into(),
            level: level.into(),
            target: target.into(),
            span_name: None,
            span_id: None,
            fields: vec![("user_id".into(), "42".into())],
            message: message.into(),
        }
    }

    #[test]
    fn snapshot_filtered_keeps_last_matching_lines() {
        let mut buf = LogBuffer::new(10);
        buf.push(entry("INFO", "gregobot::bot", "hello"));
        buf.push(entry("ERROR", "gregobot::bot", "webhook 401"));
        buf.push(entry("WARN", "gregobot::scheduler", "retry"));
        buf.push(entry("ERROR", "sqlx::query", "db down"));

        let all = buf.snapshot_filtered(10, &LogFilter::default());
        assert_eq!(all.len(), 4);

        let errors =
            buf.snapshot_filtered(10, &LogFilter::from_query(Some("ERROR,WARN"), None, None));
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].message, "webhook 401");
        assert_eq!(errors[2].message, "db down");

        let bot = buf.snapshot_filtered(10, &LogFilter::from_query(None, Some("::bot"), None));
        assert_eq!(bot.len(), 2);

        let q = buf.snapshot_filtered(10, &LogFilter::from_query(None, None, Some("401")));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].level, "ERROR");

        let by_field = buf.snapshot_filtered(10, &LogFilter::from_query(None, None, Some("42")));
        assert_eq!(by_field.len(), 4);

        let last_error =
            buf.snapshot_filtered(1, &LogFilter::from_query(Some("error"), None, None));
        assert_eq!(last_error.len(), 1);
        assert_eq!(last_error[0].message, "db down");
    }

    #[test]
    fn prepend_persisted_keeps_startup_logs_and_trims_oldest() {
        let mut buf = LogBuffer::new(3);
        buf.push(entry("INFO", "gregobot::bot", "startup"));
        buf.prepend_persisted(vec![
            entry("INFO", "gregobot::bot", "old-1"),
            entry("INFO", "gregobot::bot", "old-2"),
            entry("INFO", "gregobot::bot", "old-3"),
        ]);
        let all = buf.snapshot_all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].message, "old-2");
        assert_eq!(all[1].message, "old-3");
        assert_eq!(all[2].message, "startup");
    }

    #[test]
    fn max_entries_from_env_ignores_zero_and_invalid() {
        assert_eq!(parse_positive_usize("UNSET_LOG_MAX_TEST", 7), 7);
    }
}
