//! Parse Gemini HTTP error payloads and DM every admin on Telegram.

use teloxide::prelude::*;
use tracing::warn;

use crate::db::Db;
use crate::db::users::Role;

const MAX_BODY_CHARS: usize = 1500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiApiError {
    pub http_status: u16,
    pub code: Option<i64>,
    pub status: Option<String>,
    pub message: String,
    pub model: String,
    pub kind: &'static str,
    pub user_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct GeminiErrorBody {
    error: GeminiErrorInner,
}

#[derive(Debug, serde::Deserialize)]
struct GeminiErrorInner {
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

impl GeminiApiError {
    pub fn from_http(
        http_status: u16,
        body: &str,
        model: &str,
        kind: &'static str,
        user_id: Option<i64>,
    ) -> Self {
        let parsed = serde_json::from_str::<GeminiErrorBody>(body).ok();
        let (code, status, message) = match parsed {
            Some(parsed) => (
                json_to_i64(parsed.error.code),
                nonempty(parsed.error.status),
                parsed
                    .error
                    .message
                    .map(|m| truncate(&m, MAX_BODY_CHARS))
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| truncate(body, MAX_BODY_CHARS)),
            ),
            None => (None, None, truncate(body, MAX_BODY_CHARS)),
        };

        Self {
            http_status,
            code,
            status,
            message,
            model: model.to_string(),
            kind,
            user_id,
        }
    }

    /// Gemini returned a machine-readable error code (JSON `error.code` or HTTP status).
    pub fn has_error_code(&self) -> bool {
        self.code.is_some() || self.http_status >= 400
    }

    pub fn telegram_text(&self) -> String {
        let mut lines = vec![
            "Gemini API error".to_string(),
            format!("kind: {}", self.kind),
            format!("model: {}", self.model),
            format!("http: {}", self.http_status),
        ];
        if let Some(code) = self.code {
            lines.push(format!("code: {code}"));
        }
        if let Some(status) = &self.status {
            lines.push(format!("status: {status}"));
        }
        if let Some(user_id) = self.user_id {
            lines.push(format!("user_id: {user_id}"));
        }
        if !self.message.is_empty() {
            lines.push(String::new());
            lines.push(self.message.clone());
        }
        lines.join("\n")
    }
}

fn json_to_i64(value: Option<serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_u64().map(|u| u as i64)),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn truncate(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Parse a failed Gemini HTTP response, trip the quota circuit, and DM admins
/// unless the circuit was already open for a quota error.
pub fn record_and_maybe_alert(
    pool: &Db,
    bot_token: &str,
    model: &str,
    kind: &'static str,
    user_id: Option<i64>,
    http_status: reqwest::StatusCode,
    body: &str,
) {
    let error = GeminiApiError::from_http(http_status.as_u16(), body, model, kind, user_id);
    if crate::ai::gemini_circuit::record_failure(&error) {
        spawn_admin_alert(pool, bot_token, model, kind, user_id, http_status, body);
    }
}

/// Parse a failed Gemini HTTP response and DM every admin. Never fails the caller.
pub fn spawn_admin_alert(
    pool: &Db,
    bot_token: &str,
    model: &str,
    kind: &'static str,
    user_id: Option<i64>,
    http_status: reqwest::StatusCode,
    body: &str,
) {
    if bot_token.trim().is_empty() {
        return;
    }
    let error = GeminiApiError::from_http(http_status.as_u16(), body, model, kind, user_id);
    if !error.has_error_code() {
        return;
    }

    let pool = pool.clone();
    let bot_token = bot_token.to_string();
    tokio::spawn(async move {
        if let Err(e) = notify_admins(&pool, &bot_token, &error).await {
            warn!(
                kind = error.kind,
                http_status = error.http_status,
                "Failed to notify admins of Gemini error: {e}"
            );
        }
    });
}

async fn notify_admins(pool: &Db, bot_token: &str, error: &GeminiApiError) -> anyhow::Result<()> {
    let admins: Vec<_> = crate::db::users::list(pool)
        .await?
        .into_iter()
        .filter(|u| u.role == Role::Admin)
        .collect();
    if admins.is_empty() {
        warn!(
            kind = error.kind,
            http_status = error.http_status,
            "Gemini error received but no admin users to notify"
        );
        return Ok(());
    }

    let bot = Bot::new(bot_token);
    let text = error.telegram_text();
    for admin in admins {
        if let Err(e) = bot
            .send_message(ChatId(admin.telegram_user_id), text.clone())
            .await
        {
            warn!(
                admin_user_id = admin.id,
                telegram_user_id = admin.telegram_user_id,
                "Failed to DM admin about Gemini error: {e}"
            );
        }
    }
    Ok(())
}

/// Build the usual `anyhow` error after alerting admins.
/// Quota / billing `429`s trip the process-wide circuit and notify admins once.
pub fn fail_http(
    pool: &Db,
    bot_token: &str,
    model: &str,
    kind: &'static str,
    user_id: Option<i64>,
    status: reqwest::StatusCode,
    body: String,
    prefix: &str,
) -> anyhow::Error {
    let error = GeminiApiError::from_http(status.as_u16(), &body, model, kind, user_id);
    if crate::ai::gemini_circuit::record_failure(&error) {
        spawn_admin_alert(pool, bot_token, model, kind, user_id, status, &body);
    }
    anyhow::anyhow!("{prefix}: {status} - {body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert};

    #[test]
    fn reqwest_status_codes_match_http_ints_we_persist() {
        assert_eq!(reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16(), 429);
        assert_eq!(reqwest::StatusCode::UNAUTHORIZED.as_u16(), 401);
        assert_eq!(reqwest::StatusCode::CREATED.as_u16(), 201);
        assert_eq!(reqwest::StatusCode::NO_CONTENT.as_u16(), 204);
        assert_eq!(reqwest::StatusCode::OK.as_u16(), 200);
        let err = GeminiApiError::from_http(
            reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16(),
            r#"{"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}}"#,
            "m",
            "ask",
            None,
        );
        assert_eq!(err.http_status, 429);
        assert_eq!(err.code, Some(429));
    }

    #[test]
    fn parses_standard_gemini_error_json() {
        let body = r#"{
            "error": {
                "code": 429,
                "message": "Resource has been exhausted (e.g. check quota).",
                "status": "RESOURCE_EXHAUSTED"
            }
        }"#;
        let err = GeminiApiError::from_http(429, body, "gemini-2.5-flash", "ask", Some(7));
        assert_eq!(err.http_status, 429);
        assert_eq!(err.code, Some(429));
        assert_eq!(err.status.as_deref(), Some("RESOURCE_EXHAUSTED"));
        assert!(err.message.contains("exhausted"));
        assert!(err.has_error_code());
        let text = err.telegram_text();
        assert!(text.contains("code: 429"));
        assert!(text.contains("status: RESOURCE_EXHAUSTED"));
        assert!(text.contains("user_id: 7"));
        assert!(text.contains("kind: ask"));
    }

    #[test]
    fn parses_string_error_code() {
        let body = r#"{"error":{"code":"400","message":"Invalid","status":"INVALID_ARGUMENT"}}"#;
        let err = GeminiApiError::from_http(400, body, "m", "image", None);
        assert_eq!(err.code, Some(400));
        assert_eq!(err.status.as_deref(), Some("INVALID_ARGUMENT"));
    }

    #[test]
    fn falls_back_to_raw_body_without_json() {
        let err = GeminiApiError::from_http(503, "upstream overloaded", "m", "summarize", None);
        assert_eq!(err.code, None);
        assert_eq!(err.message, "upstream overloaded");
        assert!(err.has_error_code());
        assert!(err.telegram_text().contains("http: 503"));
    }

    #[test]
    fn truncates_long_error_bodies() {
        let long = "x".repeat(4000);
        let body = format!(r#"{{"error":{{"code":500,"message":"{long}"}}}}"#);
        let err = GeminiApiError::from_http(500, &body, "m", "ask", None);
        assert!(err.message.chars().count() <= MAX_BODY_CHARS);
        assert!(err.message.ends_with('…'));
    }

    #[tokio::test]
    async fn list_users_includes_admins_for_alerts() {
        let (pool, path) = test_db().await;
        insert(&pool, 111, Role::Admin, Some("admin".into()), None, None)
            .await
            .unwrap();
        insert(&pool, 222, Role::Sportive, Some("sport".into()), None, None)
            .await
            .unwrap();
        insert(&pool, 333, Role::Pending, Some("pend".into()), None, None)
            .await
            .unwrap();

        let admins: Vec<_> = crate::db::users::list(&pool)
            .await
            .unwrap()
            .into_iter()
            .filter(|u| u.role == Role::Admin)
            .collect();
        assert_eq!(admins.len(), 1);
        assert_eq!(admins[0].telegram_user_id, 111);
        let _ = std::fs::remove_file(&path);
    }
}
