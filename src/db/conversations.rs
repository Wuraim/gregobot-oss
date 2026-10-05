use anyhow::Result;
use sqlx::SqlitePool;

/// A single message in a conversation.
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    /// Speaker label derived from the authoring user (`@username`, first name, or `User <id>`).
    /// Empty for assistant turns.
    pub speaker: String,
    /// SQLite `datetime('now')` UTC timestamp.
    pub created_at: String,
}

/// Save a message in the conversation history.
/// `chat_id` is `Some` for group chats (shared history) and `None` for private chats.
pub async fn insert(
    pool: &SqlitePool,
    user_id: i64,
    role: &str,
    content: &str,
    chat_id: Option<i64>,
) -> Result<i64> {
    let id = sqlx::query!(
        r#"
        INSERT INTO conversations (user_id, role, content, chat_id)
        VALUES (?, ?, ?, ?)
        "#,
        user_id,
        role,
        content,
        chat_id,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok(id)
}

fn speaker_from_profile(
    username: Option<&str>,
    first_name: Option<&str>,
    telegram_user_id: Option<i64>,
) -> String {
    if let Some(username) = username.map(str::trim).filter(|u| !u.is_empty()) {
        format!("@{username}")
    } else if let Some(first_name) = first_name.map(str::trim).filter(|n| !n.is_empty()) {
        first_name.to_string()
    } else if let Some(tg_id) = telegram_user_id {
        format!("User {tg_id}")
    } else {
        String::new()
    }
}

/// If a user message is missing a `[speaker]:` prefix (legacy rows), add one from the author profile.
pub fn content_with_speaker(role: &str, content: &str, speaker: &str) -> String {
    if role != "user" || speaker.trim().is_empty() {
        return content.to_string();
    }
    if content.starts_with('[') && content.contains("]: ") {
        content.to_string()
    } else {
        format!("[{}]: {content}", speaker.trim())
    }
}

/// Prefix the message with a relative timestamp so the model can tell today from last week.
pub fn content_for_model(
    msg: &ConversationMessage,
    tz: &chrono_tz::Tz,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let body = content_with_speaker(&msg.role, &msg.content, &msg.speaker);
    match crate::datetime::parse_sqlite_datetime(&msg.created_at) {
        Some(at) => format!(
            "({}) {body}",
            crate::datetime::format_relative_when(at, tz, now)
        ),
        None => body,
    }
}

/// Fetch the most recent `limit` messages for a private chat, ordered oldest-first.
pub async fn recent_for_user(
    pool: &SqlitePool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<ConversationMessage>> {
    let rows = sqlx::query!(
        r#"
        SELECT c.role, c.content, c.created_at,
               u.username AS username,
               u.first_name AS first_name,
               u.telegram_user_id AS telegram_user_id
        FROM conversations c
        LEFT JOIN users u ON u.id = c.user_id
        WHERE c.user_id = ? AND c.chat_id IS NULL
        ORDER BY c.id DESC
        LIMIT ?
        "#,
        user_id,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let mut messages: Vec<ConversationMessage> = rows
        .into_iter()
        .map(|r| {
            let speaker = if r.role == "user" {
                speaker_from_profile(
                    r.username.as_deref(),
                    r.first_name.as_deref(),
                    r.telegram_user_id,
                )
            } else {
                String::new()
            };
            ConversationMessage {
                role: r.role,
                content: r.content,
                speaker,
                created_at: r.created_at,
            }
        })
        .collect();
    messages.reverse();

    Ok(messages)
}

/// Fetch the most recent `limit` messages for a group chat, ordered oldest-first.
pub async fn recent_for_chat(
    pool: &SqlitePool,
    chat_id: i64,
    limit: i64,
) -> Result<Vec<ConversationMessage>> {
    let rows = sqlx::query!(
        r#"
        SELECT c.role, c.content, c.created_at,
               u.username AS username,
               u.first_name AS first_name,
               u.telegram_user_id AS telegram_user_id
        FROM conversations c
        LEFT JOIN users u ON u.id = c.user_id
        WHERE c.chat_id = ?
        ORDER BY c.id DESC
        LIMIT ?
        "#,
        chat_id,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let mut messages: Vec<ConversationMessage> = rows
        .into_iter()
        .map(|r| {
            let speaker = if r.role == "user" {
                speaker_from_profile(
                    r.username.as_deref(),
                    r.first_name.as_deref(),
                    Some(r.telegram_user_id),
                )
            } else {
                String::new()
            };
            ConversationMessage {
                role: r.role,
                content: r.content,
                speaker,
                created_at: r.created_at,
            }
        })
        .collect();
    messages.reverse();

    Ok(messages)
}

/// Delete messages older than 7 days globally, then cap retained messages:
/// - 50 latest per user for private chats (chat_id IS NULL)
/// - 100 latest per chat for group chats (chat_id IS NOT NULL)
pub async fn prune(pool: &SqlitePool) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM conversations
        WHERE created_at < datetime('now', '-7 days')
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"
        DELETE FROM conversations
        WHERE chat_id IS NULL
          AND id NOT IN (
            SELECT id FROM (
              SELECT id FROM conversations AS c2
              WHERE c2.user_id = conversations.user_id AND c2.chat_id IS NULL
              ORDER BY c2.id DESC
              LIMIT 50
            )
          )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"
        DELETE FROM conversations
        WHERE chat_id IS NOT NULL
          AND id NOT IN (
            SELECT id FROM (
              SELECT id FROM conversations AS c2
              WHERE c2.chat_id = conversations.chat_id
              ORDER BY c2.id DESC
              LIMIT 100
            )
          )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConversationMessage, content_for_model, content_with_speaker};

    #[test]
    fn keeps_existing_speaker_prefix() {
        assert_eq!(
            content_with_speaker("user", "[@Alice]: hi", "@Bob"),
            "[@Alice]: hi"
        );
    }

    #[test]
    fn adds_missing_speaker_prefix() {
        assert_eq!(
            content_with_speaker("user", "hi there", "@Alice"),
            "[@Alice]: hi there"
        );
    }

    #[test]
    fn leaves_assistant_untouched() {
        assert_eq!(
            content_with_speaker("assistant", "hello", "@Alice"),
            "hello"
        );
    }

    #[test]
    fn content_for_model_prefixes_relative_time() {
        use chrono::{TimeZone, Utc};
        use chrono_tz::Europe::Paris;

        let msg = ConversationMessage {
            role: "user".into(),
            content: "hi".into(),
            speaker: "@Alice".into(),
            created_at: "2026-08-17 09:12:00".into(),
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 20, 0, 0).unwrap();
        let text = content_for_model(&msg, &Paris, now);
        assert!(text.starts_with("(yesterday 11:12) "));
        assert!(text.contains("[@Alice]: hi"));
    }
}
