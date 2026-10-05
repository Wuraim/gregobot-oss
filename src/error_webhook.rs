//! Fire-and-forget Cursor Automations webhook on every ERROR log.
//!
//! Enabled only when both secrets are set:
//! - `ERROR_CURSOR_WEBHOOK_URL` — Automations webhook URL
//! - `ERROR_CURSOR_WEBHOOK_BEARER` — auth token (`crsr_…` or `Bearer crsr_…`)
//!
//! Payloads are redacted before enqueue so Telegram bot tokens and Gemini
//! `?key=` values never leave the host. The worker uses a bounded queue and
//! HTTP timeouts so a hung Cursor endpoint cannot stall or grow memory forever.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc;
use tracing::Subscriber;
use tracing::field::Visit;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

pub const URL_ENV: &str = "ERROR_CURSOR_WEBHOOK_URL";
pub const BEARER_ENV: &str = "ERROR_CURSOR_WEBHOOK_BEARER";

/// Drop new ERROR events when the worker is this far behind.
pub const QUEUE_CAPACITY: usize = 32;

const SELF_TARGET_PREFIX: &str = "gregobot::error_webhook";
const REDACTED: &str = "[REDACTED]";
const MIN_SECRET_LEN: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Trimmed non-empty env value, or `None` if unset/blank.
fn optional_secret(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Pair of webhook secrets. Both must be present to enable the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorWebhookSecrets {
    pub url: String,
    pub bearer: String,
}

impl ErrorWebhookSecrets {
    pub fn from_env() -> Option<Self> {
        let url = std::env::var(URL_ENV).ok();
        let bearer = std::env::var(BEARER_ENV).ok();
        Self::from_raw(url.as_deref(), bearer.as_deref())
    }

    pub fn from_raw(url: Option<&str>, bearer: Option<&str>) -> Option<Self> {
        Some(Self {
            url: optional_secret(url)?,
            bearer: optional_secret(bearer)?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEvent {
    pub source: &'static str,
    pub level: String,
    pub timestamp: String,
    pub target: String,
    pub message: String,
    pub span_name: Option<String>,
    pub fields: HashMap<String, String>,
}

/// Normalize `ERROR_CURSOR_WEBHOOK_BEARER` into a full `Authorization` header value.
pub fn authorization_header(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.to_ascii_lowercase().starts_with("bearer ") {
        trimmed.to_string()
    } else {
        format!("Bearer {trimmed}")
    }
}

fn known_secrets_from_env() -> Vec<String> {
    const KEYS: &[&str] = &[
        "BOT_TOKEN",
        "GEMINI_API_TOKEN",
        "ERROR_CURSOR_WEBHOOK_BEARER",
    ];
    let mut secrets = Vec::new();
    for key in KEYS {
        let Ok(raw) = std::env::var(key) else {
            continue;
        };
        let Some(value) = optional_secret(Some(&raw)) else {
            continue;
        };
        push_secret(&mut secrets, &value);
        for prefix in ["Bearer ", "bearer ", "BEARER "] {
            if let Some(rest) = value.strip_prefix(prefix) {
                push_secret(&mut secrets, rest.trim());
            }
        }
    }
    secrets.sort_by(|a, b| b.len().cmp(&a.len()));
    secrets.dedup();
    secrets
}

fn push_secret(secrets: &mut Vec<String>, value: &str) {
    if value.len() >= MIN_SECRET_LEN {
        secrets.push(value.to_string());
    }
}

fn is_url_stop(c: char) -> bool {
    c.is_ascii_whitespace()
        || matches!(
            c,
            '/' | '"'
                | '\''
                | ')'
                | '('
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '&'
                | '?'
                | '#'
                | ','
                | ';'
                | '\\'
        )
}

fn is_telegram_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Strip Telegram bot tokens, Gemini `key=` query values, Bearer tokens, and
/// known env secrets from text destined for the off-host webhook.
fn redact_sensitive(text: &str, extra_secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in extra_secrets {
        if secret.len() >= MIN_SECRET_LEN {
            out = out.replace(secret, REDACTED);
        }
    }
    out = redact_after_bot_prefix(&out);
    out = redact_colon_bot_tokens(&out);
    out = redact_query_key_params(&out);
    redact_bearer_tokens(&out)
}

/// `https://api.telegram.org/botTOKEN/...` and Debug `path: "/botTOKEN"`.
fn redact_after_bot_prefix(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let prefix = "/bot";
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(rel) = lower[i..].find(prefix) {
        let start = i + rel;
        let secret_start = start + prefix.len();
        out.push_str(&input[i..secret_start]);
        let rest = &input[secret_start..];
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let end_rel = rest
                .char_indices()
                .find(|(_, c)| is_url_stop(*c))
                .map(|(idx, _)| idx)
                .unwrap_or(rest.len());
            if end_rel > 0 {
                out.push_str(REDACTED);
                i = secret_start + end_rel;
                continue;
            }
        }
        i = secret_start;
    }
    out.push_str(&input[i..]);
    out
}

/// Teloxide Debug splits host/path, so also match `123456:AAH…` token shape.
fn redact_colon_bot_tokens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let mut left = i;
            while left > 0 && bytes[left - 1].is_ascii_digit() {
                left -= 1;
            }
            let digit_len = i - left;
            let mut right = i + 1;
            while right < bytes.len() && is_telegram_token_char(bytes[right]) {
                right += 1;
            }
            let hash_len = right - (i + 1);
            if digit_len >= 5 && hash_len >= 20 {
                out.truncate(out.len().saturating_sub(digit_len));
                out.push_str(REDACTED);
                i = right;
                continue;
            }
        }
        let ch = input[i..].chars().next().expect("index on char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn redact_query_key_params(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let needle = "key=";
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(rel) = lower[i..].find(needle) {
        let start = i + rel;
        let prev_ok = start == 0
            || matches!(
                input.as_bytes()[start - 1],
                b'?' | b'&'
                    | b'"'
                    | b'\''
                    | b'('
                    | b'['
                    | b'{'
                    | b','
                    | b';'
                    | b' '
                    | b'\n'
                    | b'\r'
                    | b'\t'
            );
        if !prev_ok {
            let skip_to = start + needle.len();
            out.push_str(&input[i..skip_to]);
            i = skip_to;
            continue;
        }
        let secret_start = start + needle.len();
        out.push_str(&input[i..secret_start]);
        let rest = &input[secret_start..];
        let end_rel = rest
            .char_indices()
            .find(|(_, c)| is_url_stop(*c))
            .map(|(idx, _)| idx)
            .unwrap_or(rest.len());
        if end_rel > 0 {
            out.push_str(REDACTED);
        }
        i = secret_start + end_rel;
    }
    out.push_str(&input[i..]);
    out
}

fn redact_bearer_tokens(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let prefix = "bearer ";
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(rel) = lower[i..].find(prefix) {
        let start = i + rel;
        let secret_start = start + prefix.len();
        out.push_str(&input[i..secret_start]);
        let rest = &input[secret_start..];
        let end_rel = rest
            .char_indices()
            .find(|(_, c)| {
                c.is_ascii_whitespace() || matches!(*c, '"' | '\'' | ',' | ';' | ')' | ']' | '}')
            })
            .map(|(idx, _)| idx)
            .unwrap_or(rest.len());
        if end_rel > 0 {
            out.push_str(REDACTED);
        }
        i = secret_start + end_rel;
    }
    out.push_str(&input[i..]);
    out
}

pub fn spawn_worker(mut rx: mpsc::Receiver<ErrorEvent>, auth_token: String, url: String) {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .pool_max_idle_per_host(1)
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                eprintln!("error_webhook: failed to build HTTP client: {e}");
                return;
            }
        };
        let auth = authorization_header(&auth_token);
        let secrets = known_secrets_from_env();
        tracing::info!("Error Cursor webhook worker started");
        while let Some(event) = rx.recv().await {
            if let Err(e) = post_error(&client, &url, &auth, &event).await {
                let err = redact_sensitive(&format!("{e:#}"), &secrets);
                eprintln!(
                    "error_webhook: failed to POST to Cursor automation: {err} (target={})",
                    event.target
                );
            }
        }
    });
}

async fn post_error(
    client: &reqwest::Client,
    url: &str,
    authorization: &str,
    event: &ErrorEvent,
) -> anyhow::Result<()> {
    let send = async {
        let response = client
            .post(url)
            .header("Authorization", authorization)
            .header("Content-Type", "application/json")
            .json(event)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {status}: {body}");
        }
        Ok(())
    };
    tokio::time::timeout(REQUEST_TIMEOUT, send)
        .await
        .map_err(|_| anyhow::anyhow!("webhook POST timed out"))?
}

/// Tracing layer that enqueues ERROR events for the webhook worker.
pub struct ErrorWebhookLayer {
    tx: mpsc::Sender<ErrorEvent>,
    secrets: Vec<String>,
}

impl ErrorWebhookLayer {
    pub fn new(tx: mpsc::Sender<ErrorEvent>) -> Self {
        Self {
            tx,
            secrets: known_secrets_from_env(),
        }
    }
}

impl<S> Layer<S> for ErrorWebhookLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if *event.metadata().level() != tracing::Level::ERROR {
            return;
        }

        let target = event.metadata().target();
        if target.starts_with(SELF_TARGET_PREFIX) {
            return;
        }

        let timestamp = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let span_name = ctx
            .current_span()
            .id()
            .and_then(|id| ctx.span(id).map(|span| span.name().to_string()));

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let fields = visitor
            .fields
            .into_iter()
            .map(|(k, v)| (k, redact_sensitive(&v, &self.secrets)))
            .collect();

        let payload = ErrorEvent {
            source: "gregobot",
            level: "ERROR".into(),
            timestamp,
            target: target.to_string(),
            message: redact_sensitive(&visitor.message, &self.secrets),
            span_name,
            fields,
        };

        // Never block the logger: drop when the worker is busy or gone.
        if let Err(e) = self.tx.try_send(payload) {
            match e {
                mpsc::error::TrySendError::Full(_) => {
                    eprintln!("error_webhook: queue full, dropping ERROR event");
                }
                mpsc::error::TrySendError::Closed(_) => {}
            }
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: HashMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.store(field.name(), format!("{value:?}"));
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
            self.fields.insert(name.to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TG_TOKEN: &str = "123456789:AAH-_exampleTelegramTokenValue99";
    const GEMINI_KEY: &str = "AIzaSyD-exampleGeminiKeyValue0000001";

    fn sample_event() -> ErrorEvent {
        ErrorEvent {
            source: "gregobot",
            level: "ERROR".into(),
            timestamp: "2026-08-26T00:00:00.000Z".into(),
            target: "gregobot::bot::handlers".into(),
            message: "AI ask failed".into(),
            span_name: None,
            fields: HashMap::new(),
        }
    }

    #[test]
    fn optional_secret_trims_and_rejects_blank() {
        assert_eq!(optional_secret(None), None);
        assert_eq!(optional_secret(Some("")), None);
        assert_eq!(optional_secret(Some("   ")), None);
        assert_eq!(
            optional_secret(Some("  https://example.com/hook  ")).as_deref(),
            Some("https://example.com/hook")
        );
    }

    #[test]
    fn secrets_require_both_url_and_bearer() {
        assert!(ErrorWebhookSecrets::from_raw(None, Some("crsr_abc")).is_none());
        assert!(ErrorWebhookSecrets::from_raw(Some("https://hook"), None).is_none());
        assert!(ErrorWebhookSecrets::from_raw(Some("  "), Some("crsr_abc")).is_none());
        let secrets = ErrorWebhookSecrets::from_raw(
            Some(" https://hook.example/cursor "),
            Some("  crsr_abc  "),
        )
        .expect("both secrets set");
        assert_eq!(secrets.url, "https://hook.example/cursor");
        assert_eq!(secrets.bearer, "crsr_abc");
    }

    #[test]
    fn authorization_header_adds_bearer_prefix() {
        assert_eq!(authorization_header("crsr_abc"), "Bearer crsr_abc");
    }

    #[test]
    fn authorization_header_keeps_existing_bearer() {
        assert_eq!(authorization_header("Bearer crsr_abc"), "Bearer crsr_abc");
        assert_eq!(
            authorization_header("  bearer crsr_abc  "),
            "bearer crsr_abc"
        );
    }

    #[test]
    fn error_event_serializes_for_webhook() {
        let mut fields = HashMap::new();
        fields.insert("user_id".into(), "1".into());
        let event = ErrorEvent {
            source: "gregobot",
            level: "ERROR".into(),
            timestamp: "2026-08-26T00:00:00.000Z".into(),
            target: "gregobot::bot::handlers".into(),
            message: "AI ask failed: Too many tool call iterations".into(),
            span_name: Some("llm_call".into()),
            fields,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["source"], "gregobot");
        assert_eq!(json["level"], "ERROR");
        assert_eq!(json["fields"]["user_id"], "1");
        assert!(json["message"].as_str().unwrap().contains("tool call"));
    }

    #[test]
    fn redacts_telegram_bot_url_in_reqwest_display() {
        let msg = format!(
            "Reminder #3: failed to send: error sending request for url (https://api.telegram.org/bot{TG_TOKEN}/sendMessage)"
        );
        let redacted = redact_sensitive(&msg, &[]);
        assert!(
            !redacted.contains(TG_TOKEN),
            "telegram token leaked: {redacted}"
        );
        assert!(redacted.contains("api.telegram.org/bot[REDACTED]"));
    }

    #[test]
    fn redacts_telegram_file_url_and_debug_path() {
        let file = format!("https://api.telegram.org/file/bot{TG_TOKEN}/photos/x.jpg");
        let debug = format!(
            "reqwest::Error {{ url: Url {{ host: Some(\"api.telegram.org\"), path: \"/bot{TG_TOKEN}/sendMessage\" }} }}"
        );
        for msg in [file, debug] {
            let redacted = redact_sensitive(&msg, &[]);
            assert!(
                !redacted.contains(TG_TOKEN),
                "telegram token leaked: {redacted}"
            );
        }
    }

    #[test]
    fn redacts_gemini_key_query_param() {
        let msg = format!(
            "error sending request for url (https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={GEMINI_KEY}): connection timed out"
        );
        let redacted = redact_sensitive(&msg, &[]);
        assert!(
            !redacted.contains(GEMINI_KEY),
            "gemini key leaked: {redacted}"
        );
        assert!(redacted.contains("key=[REDACTED]"));
    }

    #[test]
    fn redacts_gemini_key_in_debug_query_and_fields() {
        let debug = format!("query: Some(\"key={GEMINI_KEY}\")");
        let redacted = redact_sensitive(&debug, &[]);
        assert!(!redacted.contains(GEMINI_KEY));
        assert!(redacted.contains("key=[REDACTED]"));

        let extra = vec![GEMINI_KEY.to_string()];
        let field = redact_sensitive(&format!("alt=json&key={GEMINI_KEY}"), &extra);
        assert!(!field.contains(GEMINI_KEY));
    }

    #[test]
    fn redacts_known_env_secret_and_bearer() {
        let secrets = vec!["super-secret-bot-token-value".to_string()];
        let msg = "Failed to set webhook: super-secret-bot-token-value";
        let redacted = redact_sensitive(msg, &secrets);
        assert!(!redacted.contains("super-secret-bot-token-value"));
        assert!(redacted.contains(REDACTED));

        let bearer = redact_sensitive(
            "Authorization: Bearer crsr_live_abcdefghijklmnopqrstuv",
            &[],
        );
        assert!(!bearer.contains("crsr_live_abcdefghijklmnopqrstuv"));
        assert!(bearer.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn does_not_redact_user_ids_or_botfather() {
        let msg = "AI ask failed for user 42 in /botfather docs";
        let redacted = redact_sensitive(msg, &[]);
        assert_eq!(redacted, msg);
    }

    #[test]
    fn redacted_payload_serializes_without_secrets() {
        let message = redact_sensitive(
            &format!(
                "AI ask failed: error sending request for url (https://generativelanguage.googleapis.com/v1beta/models/x:generateContent?key={GEMINI_KEY})"
            ),
            &[],
        );
        let mut fields = HashMap::new();
        fields.insert(
            "err".into(),
            redact_sensitive(
                &format!("https://api.telegram.org/bot{TG_TOKEN}/sendMessage"),
                &[],
            ),
        );
        fields.insert("user_id".into(), "99".into());
        let event = ErrorEvent {
            source: "gregobot",
            level: "ERROR".into(),
            timestamp: "2026-08-26T00:00:00.000Z".into(),
            target: "gregobot::bot::handlers".into(),
            message,
            span_name: None,
            fields,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains(TG_TOKEN));
        assert!(!json.contains(GEMINI_KEY));
        assert!(json.contains("user_id"));
        assert!(json.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn post_error_times_out_instead_of_hanging() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .connect_timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            post_error(
                &client,
                "http://192.0.2.1:81/hang",
                "Bearer x",
                &sample_event(),
            ),
        )
        .await;
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "POST hung for {:?}",
            start.elapsed()
        );
        let inner = result.expect("outer timeout");
        assert!(inner.is_err(), "expected timeout/connect error");
    }
}
