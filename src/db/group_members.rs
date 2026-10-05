use anyhow::Result;
use sqlx::SqlitePool;

/// A participant detected in a group chat.
#[derive(Debug, Clone)]
pub struct Participant {
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
}

impl Participant {
    /// Human-readable display name for this participant.
    pub fn display_name(&self) -> String {
        if let Some(username) = self
            .username
            .as_ref()
            .map(|u| u.trim())
            .filter(|u| !u.is_empty())
        {
            format!("@{username}")
        } else if let Some(first_name) = self
            .first_name
            .as_ref()
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
        {
            first_name.to_string()
        } else {
            format!("User {}", self.telegram_user_id)
        }
    }
}

/// Rich context about a group chat, built from tracked participants.
#[derive(Debug, Clone)]
pub struct GroupContext {
    pub chat_id: i64,
    pub participants: Vec<Participant>,
}

impl GroupContext {
    /// Format the participant list for inclusion in an LLM system prompt.
    pub fn prompt_text(&self) -> String {
        if self.participants.is_empty() {
            return String::new();
        }

        let names: Vec<String> = self.participants.iter().map(|p| p.display_name()).collect();

        format!(
            "Group chat context: {} tracked participant(s): {}.\n",
            self.participants.len(),
            names.join(", ")
        )
    }
}

/// Track a participant in a group chat, updating `last_seen_at` if already known.
pub async fn upsert(pool: &SqlitePool, chat_id: i64, telegram_user_id: i64) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO group_members (chat_id, telegram_user_id)
        VALUES (?, ?)
        ON CONFLICT(chat_id, telegram_user_id) DO UPDATE SET
            last_seen_at = datetime('now')
        "#,
        chat_id,
        telegram_user_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// List all tracked participants for a given chat, most recently seen first.
pub async fn list_for_chat(pool: &SqlitePool, chat_id: i64) -> Result<Vec<Participant>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            gm.telegram_user_id,
            u.username,
            u.first_name
        FROM group_members gm
        LEFT JOIN users u ON u.telegram_user_id = gm.telegram_user_id
        WHERE gm.chat_id = ?
        ORDER BY gm.last_seen_at DESC
        "#,
        chat_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Participant {
            telegram_user_id: r.telegram_user_id,
            username: r.username,
            first_name: r.first_name,
        })
        .collect())
}

/// Build a `GroupContext` for the AI from a chat id.
pub async fn context_for_chat(pool: &SqlitePool, chat_id: i64) -> Result<GroupContext> {
    let participants = list_for_chat(pool, chat_id).await?;
    Ok(GroupContext {
        chat_id,
        participants,
    })
}
