use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::fmt;

/// The three roles in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Sportive,
    Admin,
    Pending,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Sportive => write!(f, "sportive"),
            Role::Admin => write!(f, "admin"),
            Role::Pending => write!(f, "pending"),
        }
    }
}

impl TryFrom<String> for Role {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        match s.as_str() {
            "sportive" => Ok(Role::Sportive),
            "admin" => Ok(Role::Admin),
            "pending" => Ok(Role::Pending),
            other => Err(anyhow!("unknown role: {other}")),
        }
    }
}

use crate::i18n::Lang;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub telegram_user_id: i64,
    pub role: Role,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub language_code: Option<String>,
    pub description: Option<String>,
    pub timezone: String,
    pub web_search_enabled: bool,
    pub daily_recap_enabled: bool,
    pub daily_recap_hour: i64,
    pub daily_recap_minute: i64,
    pub last_daily_recap_at: Option<String>,
    pub nudges_enabled: bool,
    pub nudge_inactivity_days: i64,
    pub last_nudge_at: Option<String>,
}

/// Treat blank Telegram strings as missing so COALESCE does not wipe known values.
fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl User {
    pub fn lang(&self) -> Lang {
        Lang::from_iso_code(self.language_code.as_deref())
    }

    /// Suggest a default timezone from the user's language code.
    pub fn default_timezone(language_code: Option<&str>) -> &'static str {
        match language_code {
            Some("fr") => "Europe/Paris",
            _ => "UTC",
        }
    }

    /// Suffix for `/userlist` entries (` @alice` or ` (Alice)`).
    pub fn list_display_suffix(&self) -> String {
        if let Some(username) = self
            .username
            .as_ref()
            .map(|u| u.trim())
            .filter(|u| !u.is_empty())
        {
            format!(" @{username}")
        } else if let Some(first_name) = self
            .first_name
            .as_ref()
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
        {
            format!(" ({first_name})")
        } else {
            String::new()
        }
    }
}

/// Look up a registered user by their Telegram user ID.
pub async fn find_by_telegram_id(pool: &SqlitePool, telegram_user_id: i64) -> Result<Option<User>> {
    let row = sqlx::query!(
        r#"
        SELECT u.id, u.telegram_user_id, u.username, u.first_name, u.language_code, u.description,
               u.timezone,
               u.web_search_enabled, u.daily_recap_enabled, u.daily_recap_hour,
               u.daily_recap_minute, u.last_daily_recap_at,
               u.nudges_enabled, u.nudge_inactivity_days, u.last_nudge_at,
               r.name AS role_name
        FROM users u
        JOIN roles r ON r.id = u.role_id
        WHERE u.telegram_user_id = ?
        "#,
        telegram_user_id,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(User {
            id: r.id.expect("users.id is NOT NULL"),
            telegram_user_id: r.telegram_user_id,
            role: Role::try_from(r.role_name)?,
            username: r.username,
            first_name: r.first_name,
            language_code: r.language_code,
            description: r.description,
            timezone: r.timezone.unwrap_or_else(|| "UTC".to_string()),
            web_search_enabled: r.web_search_enabled != 0,
            daily_recap_enabled: r.daily_recap_enabled != 0,
            daily_recap_hour: r.daily_recap_hour,
            daily_recap_minute: r.daily_recap_minute,
            last_daily_recap_at: r.last_daily_recap_at,
            nudges_enabled: r.nudges_enabled != 0,
            nudge_inactivity_days: r.nudge_inactivity_days,
            last_nudge_at: r.last_nudge_at,
        })),
    }
}

/// Look up a user by their Telegram username (case-insensitive).
pub async fn find_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let username = username.trim_start_matches('@').to_lowercase();
    let row = sqlx::query!(
        r#"
        SELECT u.id, u.telegram_user_id, u.username, u.first_name, u.language_code, u.description,
               u.timezone,
               u.web_search_enabled, u.daily_recap_enabled, u.daily_recap_hour,
               u.daily_recap_minute, u.last_daily_recap_at,
               u.nudges_enabled, u.nudge_inactivity_days, u.last_nudge_at,
               r.name AS role_name
        FROM users u
        JOIN roles r ON r.id = u.role_id
        WHERE LOWER(u.username) = ?
        ORDER BY u.id DESC
        LIMIT 1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(User {
            id: r.id,
            telegram_user_id: r.telegram_user_id,
            role: Role::try_from(r.role_name)?,
            username: r.username,
            first_name: r.first_name,
            language_code: r.language_code,
            description: r.description,
            timezone: r.timezone.unwrap_or_else(|| "UTC".to_string()),
            web_search_enabled: r.web_search_enabled != 0,
            daily_recap_enabled: r.daily_recap_enabled != 0,
            daily_recap_hour: r.daily_recap_hour,
            daily_recap_minute: r.daily_recap_minute,
            last_daily_recap_at: r.last_daily_recap_at,
            nudges_enabled: r.nudges_enabled != 0,
            nudge_inactivity_days: r.nudge_inactivity_days,
            last_nudge_at: r.last_nudge_at,
        })),
    }
}

/// Look up a registered user by their internal users.id.
pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<User>> {
    let row = sqlx::query!(
        r#"
        SELECT u.id, u.telegram_user_id, u.username, u.first_name, u.language_code, u.description,
               u.timezone,
               u.web_search_enabled, u.daily_recap_enabled, u.daily_recap_hour,
               u.daily_recap_minute, u.last_daily_recap_at,
               u.nudges_enabled, u.nudge_inactivity_days, u.last_nudge_at,
               r.name AS role_name
        FROM users u
        JOIN roles r ON r.id = u.role_id
        WHERE u.id = ?
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(User {
            id: r.id,
            telegram_user_id: r.telegram_user_id,
            role: Role::try_from(r.role_name)?,
            username: r.username,
            first_name: r.first_name,
            language_code: r.language_code,
            description: r.description,
            timezone: r.timezone.unwrap_or_else(|| "UTC".to_string()),
            web_search_enabled: r.web_search_enabled != 0,
            daily_recap_enabled: r.daily_recap_enabled != 0,
            daily_recap_hour: r.daily_recap_hour,
            daily_recap_minute: r.daily_recap_minute,
            last_daily_recap_at: r.last_daily_recap_at,
            nudges_enabled: r.nudges_enabled != 0,
            nudge_inactivity_days: r.nudge_inactivity_days,
            last_nudge_at: r.last_nudge_at,
        })),
    }
}

/// List all registered users joined with their role name.
pub async fn list(pool: &SqlitePool) -> Result<Vec<User>> {
    let rows = sqlx::query!(
        r#"
        SELECT u.id, u.telegram_user_id, u.username, u.first_name, u.language_code, u.description,
               u.timezone,
               u.web_search_enabled, u.daily_recap_enabled, u.daily_recap_hour,
               u.daily_recap_minute, u.last_daily_recap_at,
               u.nudges_enabled, u.nudge_inactivity_days, u.last_nudge_at,
               r.name AS role_name
        FROM users u
        JOIN roles r ON r.id = u.role_id
        ORDER BY u.id
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(User {
                id: r.id,
                telegram_user_id: r.telegram_user_id,
                role: Role::try_from(r.role_name)?,
                username: r.username,
                first_name: r.first_name,
                language_code: r.language_code,
                description: r.description,
                timezone: r.timezone.unwrap_or_else(|| "UTC".to_string()),
                web_search_enabled: r.web_search_enabled != 0,
                daily_recap_enabled: r.daily_recap_enabled != 0,
                daily_recap_hour: r.daily_recap_hour,
                daily_recap_minute: r.daily_recap_minute,
                last_daily_recap_at: r.last_daily_recap_at,
                nudges_enabled: r.nudges_enabled != 0,
                nudge_inactivity_days: r.nudge_inactivity_days,
                last_nudge_at: r.last_nudge_at,
            })
        })
        .collect()
}

/// Insert a new user. Returns the created User.
pub async fn insert(
    pool: &SqlitePool,
    telegram_user_id: i64,
    role: Role,
    username: Option<String>,
    language_code: Option<String>,
    description: Option<String>,
) -> Result<User> {
    let role_name = role.to_string();
    let timezone = User::default_timezone(language_code.as_deref()).to_string();

    let id = sqlx::query!(
        r#"
        INSERT INTO users (telegram_user_id, role_id, username, language_code, description, timezone)
        SELECT ?, r.id, ?, ?, ?, ? FROM roles r WHERE r.name = ?
        "#,
        telegram_user_id,
        username,
        language_code,
        description,
        timezone,
        role_name,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok(User {
        id,
        telegram_user_id,
        role,
        username,
        first_name: None,
        language_code,
        description,
        timezone,
        web_search_enabled: false,
        daily_recap_enabled: false,
        daily_recap_hour: 20,
        daily_recap_minute: 0,
        last_daily_recap_at: None,
        nudges_enabled: true,
        nudge_inactivity_days: 3,
        last_nudge_at: None,
    })
}

/// Delete a user by telegram_user_id. Returns true if a row was deleted.
pub async fn delete(pool: &SqlitePool, telegram_user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM users WHERE telegram_user_id = ?",
        telegram_user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Idempotent: insert the given telegram user as admin if they don't exist yet.
/// Admin users are also granted web search by default.
pub async fn seed_admin(pool: &SqlitePool, telegram_user_id: i64) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT OR IGNORE INTO users (telegram_user_id, role_id, web_search_enabled, daily_recap_enabled, nudges_enabled)
        SELECT ?, r.id, 1, 1, 1 FROM roles r WHERE r.name = 'admin'
        "#,
        telegram_user_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Change the role of a user by telegram_user_id.
/// When a user is promoted from pending to an authorized role, web search is enabled by default.
pub async fn set_role(pool: &SqlitePool, telegram_user_id: i64, role: Role) -> Result<bool> {
    let role_name = role.to_string();
    let web_search_enabled = if role == Role::Pending {
        None
    } else {
        Some(1i64)
    };
    let result = sqlx::query!(
        r#"
        UPDATE users SET
            role_id = (SELECT id FROM roles WHERE name = ?),
            web_search_enabled = COALESCE(?, web_search_enabled)
        WHERE telegram_user_id = ?
        "#,
        role_name,
        web_search_enabled,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Upsert a user's profile by telegram_user_id.
/// If the user does not exist yet, inserts them with role 'pending'.
/// Existing username / first_name / language_code are kept when the new value
/// is NULL (Telegram sometimes omits them depending on the update type).
pub async fn upsert_pending(
    pool: &SqlitePool,
    telegram_user_id: i64,
    username: Option<String>,
    first_name: Option<String>,
    language_code: Option<String>,
) -> Result<()> {
    let username = nonempty(username);
    let first_name = nonempty(first_name);
    let language_code = nonempty(language_code);
    let timezone = User::default_timezone(language_code.as_deref()).to_string();

    sqlx::query!(
        r#"
        INSERT INTO users (telegram_user_id, role_id, username, first_name, language_code, timezone)
        SELECT ?, r.id, ?, ?, ?, ?
        FROM roles r WHERE r.name = 'pending'
        ON CONFLICT(telegram_user_id) DO UPDATE SET
            username = COALESCE(excluded.username, users.username),
            first_name = COALESCE(excluded.first_name, users.first_name),
            timezone = COALESCE(users.timezone, excluded.timezone)
        "#,
        telegram_user_id,
        username,
        first_name,
        language_code,
        timezone,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Update display-name profile fields for an existing user.
/// NULL arguments leave the existing column value unchanged.
/// Language is not touched here — use [`set_language_code`] so Telegram
/// client locale does not overwrite an explicit `/lang` preference.
pub async fn update_profile(
    pool: &SqlitePool,
    telegram_user_id: i64,
    username: Option<String>,
    first_name: Option<String>,
    _language_code: Option<String>,
) -> Result<bool> {
    let username = nonempty(username);
    let first_name = nonempty(first_name);
    let result = sqlx::query!(
        r#"
        UPDATE users SET
            username = COALESCE(?, username),
            first_name = COALESCE(?, first_name)
        WHERE telegram_user_id = ?
        "#,
        username,
        first_name,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_timezone(
    pool: &SqlitePool,
    telegram_user_id: i64,
    timezone: &str,
) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET timezone = ? WHERE telegram_user_id = ?",
        timezone,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_language_code(
    pool: &SqlitePool,
    telegram_user_id: i64,
    language_code: &str,
) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET language_code = ? WHERE telegram_user_id = ?",
        language_code,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_description(
    pool: &SqlitePool,
    telegram_user_id: i64,
    description: Option<String>,
) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET description = ? WHERE telegram_user_id = ?",
        description,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_web_search(
    pool: &SqlitePool,
    telegram_user_id: i64,
    enabled: bool,
) -> Result<bool> {
    let enabled_i64: i64 = if enabled { 1 } else { 0 };
    let result = sqlx::query!(
        "UPDATE users SET web_search_enabled = ? WHERE telegram_user_id = ?",
        enabled_i64,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_nudge_settings(
    pool: &SqlitePool,
    telegram_user_id: i64,
    enabled: Option<bool>,
    inactivity_days: Option<i64>,
) -> Result<bool> {
    let enabled_i64 = enabled.map(|e| if e { 1i64 } else { 0i64 });
    let result = sqlx::query!(
        r#"
        UPDATE users SET
            nudges_enabled = COALESCE(?, nudges_enabled),
            nudge_inactivity_days = COALESCE(?, nudge_inactivity_days)
        WHERE telegram_user_id = ?
        "#,
        enabled_i64,
        inactivity_days,
        telegram_user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_nudge_sent(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET last_nudge_at = datetime('now') WHERE id = ?",
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;

    async fn timezone_of(pool: &SqlitePool, telegram_id: i64) -> String {
        find_by_telegram_id(pool, telegram_id)
            .await
            .unwrap()
            .unwrap()
            .timezone
    }

    #[test]
    fn role_try_from_valid() {
        assert_eq!(
            Role::try_from("sportive".to_string()).unwrap(),
            Role::Sportive
        );
        assert_eq!(Role::try_from("admin".to_string()).unwrap(), Role::Admin);
        assert_eq!(
            Role::try_from("pending".to_string()).unwrap(),
            Role::Pending
        );
    }

    #[test]
    fn role_try_from_invalid() {
        assert!(Role::try_from("superadmin".to_string()).is_err());
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::Sportive.to_string(), "sportive");
        assert_eq!(Role::Admin.to_string(), "admin");
        assert_eq!(Role::Pending.to_string(), "pending");
    }

    #[test]
    fn default_timezone() {
        assert_eq!(User::default_timezone(Some("fr")), "Europe/Paris");
        assert_eq!(User::default_timezone(Some("en")), "UTC");
        assert_eq!(User::default_timezone(None), "UTC");
    }

    #[tokio::test]
    async fn user_crud() {
        let (pool, path) = test_db().await;
        let user = insert(
            &pool,
            123_456,
            Role::Sportive,
            Some("alice".to_string()),
            Some("fr".to_string()),
            None,
        )
        .await
        .unwrap();

        assert_eq!(user.telegram_user_id, 123_456);
        assert_eq!(user.username, Some("alice".to_string()));
        assert_eq!(user.role, Role::Sportive);
        assert_eq!(user.timezone, "Europe/Paris");

        let found = find_by_telegram_id(&pool, 123_456).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, user.id);
        assert_eq!(found.lang(), Lang::Fr);

        let by_id = find_by_id(&pool, user.id).await.unwrap();
        assert!(by_id.is_some());

        let updated = set_role(&pool, 123_456, Role::Admin).await.unwrap();
        assert!(updated);
        let found = find_by_telegram_id(&pool, 123_456).await.unwrap().unwrap();
        assert_eq!(found.role, Role::Admin);
        assert!(found.web_search_enabled);

        let deleted = delete(&pool, 123_456).await.unwrap();
        assert!(deleted);
        assert!(find_by_telegram_id(&pool, 123_456).await.unwrap().is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn upsert_pending_updates_existing_user() {
        let (pool, path) = test_db().await;
        insert(
            &pool,
            999,
            Role::Sportive,
            Some("bob".to_string()),
            Some("en".to_string()),
            None,
        )
        .await
        .unwrap();

        upsert_pending(
            &pool,
            999,
            Some("bobby".to_string()),
            Some("Bob".to_string()),
            Some("fr".to_string()),
        )
        .await
        .unwrap();

        let user = find_by_telegram_id(&pool, 999).await.unwrap().unwrap();
        assert_eq!(user.username, Some("bobby".to_string()));
        assert_eq!(user.first_name, Some("Bob".to_string()));
        // Language is set on insert only; later Telegram locale sync must not overwrite.
        assert_eq!(user.language_code, Some("en".to_string()));
        assert_eq!(user.role, Role::Sportive);

        set_language_code(&pool, 999, "fr").await.unwrap();
        let user = find_by_telegram_id(&pool, 999).await.unwrap().unwrap();
        assert_eq!(user.language_code, Some("fr".to_string()));

        // NULL profile fields must not wipe previously discovered values.
        upsert_pending(&pool, 999, None, None, None).await.unwrap();
        let user = find_by_telegram_id(&pool, 999).await.unwrap().unwrap();
        assert_eq!(user.username, Some("bobby".to_string()));
        assert_eq!(user.first_name, Some("Bob".to_string()));
        assert_eq!(user.language_code, Some("fr".to_string()));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn list_users() {
        let (pool, path) = test_db().await;
        insert(&pool, 100, Role::Sportive, None, None, None)
            .await
            .unwrap();
        insert(&pool, 101, Role::Admin, None, None, None)
            .await
            .unwrap();

        let users = list(&pool).await.unwrap();
        assert_eq!(users.len(), 2);
        assert!(users.iter().any(|u| u.telegram_user_id == 100));
        assert!(users.iter().any(|u| u.telegram_user_id == 101));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn french_users_stuck_on_utc_move_to_paris() {
        let (pool, path) = test_db().await;
        let stuck = insert(&pool, 201, Role::Sportive, None, Some("fr".into()), None)
            .await
            .unwrap();
        set_timezone(&pool, stuck.telegram_user_id, "UTC")
            .await
            .unwrap();
        let english = insert(&pool, 202, Role::Sportive, None, Some("en".into()), None)
            .await
            .unwrap();
        let paris = insert(&pool, 203, Role::Sportive, None, Some("fr".into()), None)
            .await
            .unwrap();
        let explicit = insert(&pool, 204, Role::Sportive, None, Some("fr".into()), None)
            .await
            .unwrap();
        set_timezone(&pool, explicit.telegram_user_id, "America/New_York")
            .await
            .unwrap();
        let unknown_lang = insert(&pool, 205, Role::Pending, None, None, None)
            .await
            .unwrap();

        let sql = include_str!("../../migrations/0044_fr_timezone_paris.sql");
        let updated = sqlx::query(sql)
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected();
        assert_eq!(updated, 1);

        assert_eq!(
            timezone_of(&pool, stuck.telegram_user_id).await,
            "Europe/Paris"
        );
        assert_eq!(timezone_of(&pool, english.telegram_user_id).await, "UTC");
        assert_eq!(
            timezone_of(&pool, paris.telegram_user_id).await,
            "Europe/Paris"
        );
        assert_eq!(
            timezone_of(&pool, explicit.telegram_user_id).await,
            "America/New_York"
        );
        assert_eq!(
            timezone_of(&pool, unknown_lang.telegram_user_id).await,
            "UTC"
        );

        let _ = std::fs::remove_file(&path);
    }
}
