use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub enum BotMode {
    Webhook,
    Polling,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: String,
    /// REST admin API key (`X-API-Key`). Defaults to `bot_token` when `ADMIN_API_KEY` is unset.
    pub admin_api_key: String,
    pub webhook_url: Option<String>,
    pub bot_mode: BotMode,
    pub database_url: String,
    pub port: u16,
    /// If set, this Telegram user ID is auto-seeded as admin on startup.
    pub admin_telegram_user_id: Option<i64>,
    /// Google Gemini API token for AI features.
    pub gemini_api_token: Option<String>,
    /// Google Gemini model ID for the main answer (default: gemini-2.5-flash).
    pub gemini_model: String,
    /// Cheaper model for conversation summarization (default: gemini-2.5-flash-lite).
    pub gemini_summarize_model: String,
    /// Google Gemini embedding model ID for RAG (default: gemini-embedding-2).
    pub gemini_embedding_model: String,
    /// Gemini model for rare coach image generate/edit (default: gemini-2.5-flash-image).
    pub gemini_image_model: String,
    /// Max coach images per user per UTC day (default: 2).
    pub image_max_per_user_day: i64,
    /// Enable RAG retrieval (default: false).
    pub rag_enabled: bool,
    /// Number of chunks to retrieve for RAG (default: 5).
    pub rag_top_k: usize,
    /// Interval in seconds between reminder scheduler checks (default: 60).
    pub reminder_check_interval_secs: u64,
    /// Interval in seconds between daily recap scheduler checks (default: 300).
    pub daily_recap_check_interval_secs: u64,
    /// Interval in seconds between nudge scheduler checks (default: 21600 = 6 hours).
    pub nudge_check_interval_secs: u64,
    /// Max persisted /logs lines (default: 5000).
    pub log_max_entries: usize,
    /// Max persisted /ai/logs traces (default: 200).
    pub ai_log_max_entries: usize,
    /// Bot persona injected into all AI system prompts.
    pub bot_persona: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let admin_telegram_user_id = match std::env::var("ADMIN_TELEGRAM_USER_ID") {
            Ok(val) => Some(
                val.parse::<i64>()
                    .context("ADMIN_TELEGRAM_USER_ID must be a valid integer")?,
            ),
            Err(_) => None,
        };

        let bot_mode: BotMode = match std::env::var("BOT_MODE")
            .unwrap_or_else(|_| "webhook".to_string())
            .to_lowercase()
            .as_str()
        {
            "webhook" => BotMode::Webhook,
            "polling" => BotMode::Polling,
            other => anyhow::bail!("BOT_MODE must be 'webhook' or 'polling', got '{other}'"),
        };

        let webhook_url = std::env::var("WEBHOOK_URL").ok();

        if matches!(bot_mode, BotMode::Webhook) && webhook_url.is_none() {
            anyhow::bail!(
                "WEBHOOK_URL is required when BOT_MODE=webhook (or use BOT_MODE=polling for local dev)"
            );
        }

        let bot_token = require_env("BOT_TOKEN")?;
        let admin_api_key = std::env::var("ADMIN_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| bot_token.clone());

        Ok(Self {
            bot_token,
            admin_api_key,
            webhook_url,
            bot_mode,
            database_url: require_env("DATABASE_URL")?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse::<u16>()
                .context("PORT must be a valid port number")?,
            admin_telegram_user_id,
            gemini_api_token: std::env::var("GEMINI_API_TOKEN").ok(),
            gemini_model: std::env::var("GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-2.5-flash".to_string()),
            gemini_summarize_model: std::env::var("GEMINI_SUMMARIZE_MODEL")
                .unwrap_or_else(|_| "gemini-2.5-flash-lite".to_string()),
            gemini_embedding_model: std::env::var("GEMINI_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "gemini-embedding-2".to_string()),
            gemini_image_model: std::env::var("GEMINI_IMAGE_MODEL")
                .unwrap_or_else(|_| "gemini-2.5-flash-image".to_string()),
            image_max_per_user_day: std::env::var("GEMINI_IMAGE_MAX_PER_USER_DAY")
                .unwrap_or_else(|_| "2".to_string())
                .parse::<i64>()
                .context("GEMINI_IMAGE_MAX_PER_USER_DAY must be a valid integer")?,
            rag_enabled: std::env::var("RAG_ENABLED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            rag_top_k: std::env::var("RAG_TOP_K")
                .unwrap_or_else(|_| "5".to_string())
                .parse::<usize>()
                .context("RAG_TOP_K must be a valid usize")?,
            reminder_check_interval_secs: std::env::var("REMINDER_CHECK_INTERVAL_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse::<u64>()
                .context("REMINDER_CHECK_INTERVAL_SECS must be a valid number")?,
            daily_recap_check_interval_secs: std::env::var("DAILY_RECAP_CHECK_INTERVAL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse::<u64>()
                .context("DAILY_RECAP_CHECK_INTERVAL_SECS must be a valid number")?,
            nudge_check_interval_secs: std::env::var("NUDGE_CHECK_INTERVAL_SECS")
                .unwrap_or_else(|_| "21600".to_string())
                .parse::<u64>()
                .context("NUDGE_CHECK_INTERVAL_SECS must be a valid number")?,
            log_max_entries: crate::logs::max_entries_from_env(),
            ai_log_max_entries: crate::ai::log::max_entries_from_env(),
            bot_persona: load_bot_persona(),
        })
    }

    /// The path Telegram will POST updates to.
    pub fn webhook_path(&self) -> String {
        format!("/webhook/{}", self.bot_token)
    }

    /// Full URL Telegram will call. Only valid in webhook mode.
    pub fn webhook_full_url(&self) -> String {
        let base = self
            .webhook_url
            .as_ref()
            .expect("WEBHOOK_URL should be set in webhook mode");
        format!("{}{}", base, self.webhook_path())
    }
}

fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}

/// Docker image: `/prompt`. Local `cargo run` / tests: `prompt/` at crate root.
const PROMPT_DIRS: &[&str] = &["/prompt", "prompt"];
const PERSONA_FILES: &[&str] = &["PERSONA.md", "VOICE.md"];

pub fn load_bot_persona() -> String {
    PERSONA_FILES
        .iter()
        .filter_map(|name| read_prompt_file(name))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn read_prompt_file(name: &str) -> Option<String> {
    for dir in PROMPT_DIRS {
        let path = format!("{dir}/{name}");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if !contents.trim().is_empty() {
                return Some(contents);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persona_files_load_and_concatenate() {
        let persona = load_bot_persona();
        assert!(
            !persona.trim().is_empty(),
            "PERSONA.md + VOICE.md should load from prompt/"
        );
        assert!(
            persona.contains("personal sports coach") || persona.contains("Gregobot"),
            "identity should be present"
        );
        assert!(
            persona.contains("LES FAITS") || persona.contains("RAPPORT"),
            "voice examples should be concatenated"
        );
        assert!(
            persona.contains("send_coach_image"),
            "persona should mention the rare image tool"
        );
        assert!(
            persona.contains("get_image_quota"),
            "persona should mention the image quota tool"
        );
        assert!(
            persona.contains("search_photos"),
            "persona should mention photo search"
        );
        assert!(
            persona.contains("Never ask the user to photograph their body"),
            "persona should forbid physique progress photos"
        );
        assert!(
            persona.to_lowercase().contains("quota"),
            "persona should mention the daily image quota"
        );
        assert!(
            persona.contains("In a group, be shorter"),
            "persona should tell the model to be shorter in groups"
        );
    }

    #[test]
    fn persona_does_not_label_philosophy() {
        let persona = load_bot_persona().to_lowercase();
        for forbidden in [
            "epictetus",
            "marcus aurelius",
            "seneca",
            "stoic philosopher",
            "stoicism",
        ] {
            assert!(
                !persona.contains(forbidden),
                "persona must not name {forbidden}"
            );
        }
    }

    #[test]
    fn admin_api_key_falls_back_to_bot_token() {
        // SAFETY: test process; vars restored at end of scope.
        let prev_bot = std::env::var("BOT_TOKEN").ok();
        let prev_admin = std::env::var("ADMIN_API_KEY").ok();
        let prev_db = std::env::var("DATABASE_URL").ok();
        let prev_mode = std::env::var("BOT_MODE").ok();
        let prev_webhook = std::env::var("WEBHOOK_URL").ok();

        unsafe {
            std::env::set_var("BOT_TOKEN", "bot-secret");
            std::env::remove_var("ADMIN_API_KEY");
            std::env::set_var("DATABASE_URL", "sqlite::memory:");
            std::env::set_var("BOT_MODE", "polling");
            std::env::remove_var("WEBHOOK_URL");
        }
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.admin_api_key, "bot-secret");

        unsafe {
            std::env::set_var("ADMIN_API_KEY", "admin-secret");
        }
        let cfg = Config::from_env().expect("config");
        assert_eq!(cfg.admin_api_key, "admin-secret");
        assert_eq!(cfg.bot_token, "bot-secret");

        unsafe {
            match prev_bot {
                Some(v) => std::env::set_var("BOT_TOKEN", v),
                None => std::env::remove_var("BOT_TOKEN"),
            }
            match prev_admin {
                Some(v) => std::env::set_var("ADMIN_API_KEY", v),
                None => std::env::remove_var("ADMIN_API_KEY"),
            }
            match prev_db {
                Some(v) => std::env::set_var("DATABASE_URL", v),
                None => std::env::remove_var("DATABASE_URL"),
            }
            match prev_mode {
                Some(v) => std::env::set_var("BOT_MODE", v),
                None => std::env::remove_var("BOT_MODE"),
            }
            match prev_webhook {
                Some(v) => std::env::set_var("WEBHOOK_URL", v),
                None => std::env::remove_var("WEBHOOK_URL"),
            }
        }
    }
}
