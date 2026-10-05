use crate::config::Config;
use crate::db::Db;
use crate::db::group_members::GroupContext;
use crate::i18n::Lang;

#[derive(Debug, Clone)]
pub struct AiContext {
    pub api_key: Option<String>,
    pub model: String,
    pub summarize_model: String,
    pub embedding_model: String,
    pub image_model: String,
    pub image_max_per_user_day: i64,
    pub persona: String,
    pub rag_enabled: bool,
    pub rag_top_k: usize,
    /// Telegram bot token used to DM admins when Gemini returns an error code.
    pub bot_token: String,
}

impl AiContext {
    pub fn from_config(config: &Config) -> Self {
        Self {
            api_key: config.gemini_api_token.clone(),
            model: config.gemini_model.clone(),
            summarize_model: config.gemini_summarize_model.clone(),
            embedding_model: config.gemini_embedding_model.clone(),
            image_model: config.gemini_image_model.clone(),
            image_max_per_user_day: config.image_max_per_user_day,
            persona: config.bot_persona.clone(),
            rag_enabled: config.rag_enabled,
            rag_top_k: config.rag_top_k,
            bot_token: config.bot_token.clone(),
        }
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UserContext {
    pub user_id: i64,
    pub telegram_user_id: i64,
    pub lang: Lang,
    pub web_search_enabled: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AskImage<'a> {
    pub bytes: &'a [u8],
    pub mime_type: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct AskInput<'a> {
    pub prompt: &'a str,
    /// Display name of the Telegram sender (e.g. `@alice` or first name).
    /// Prefixed onto every user turn so the model knows who is speaking.
    pub speaker: &'a str,
    pub group_context: Option<&'a GroupContext>,
    pub image: Option<AskImage<'a>>,
    /// Telegram delivery target for tools that send media (charts, etc.).
    pub bot: Option<&'a teloxide::Bot>,
    pub chat_id: Option<i64>,
    /// When set, tool-sent media replies to this user message.
    pub reply_to_message_id: Option<teloxide::types::MessageId>,
}

#[derive(Debug, Clone, Copy)]
pub struct GenerationInput<'a> {
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolCtx<'a> {
    pub pool: &'a Db,
    pub user_id: i64,
    pub timezone: &'a str,
    pub lang: Lang,
    pub is_admin: bool,
    /// When set, tools may send media (e.g. charts) to this Telegram chat.
    pub bot: Option<&'a teloxide::Bot>,
    pub bot_token: &'a str,
    pub chat_id: Option<i64>,
    pub reply_to_message_id: Option<teloxide::types::MessageId>,
    pub api_key: Option<&'a str>,
    pub embedding_model: &'a str,
    pub image_model: &'a str,
    pub image_max_per_user_day: i64,
    /// Photo attached to the current /ask turn, if any.
    pub user_photo: Option<AskImage<'a>>,
    /// Embedding of the current user prompt, when already computed.
    pub prompt_embedding: Option<&'a [f32]>,
    /// Parent AI trace id, when this tool runs inside `/ask` (or similar).
    pub request_id: Option<&'a str>,
    /// When true, tools may run conversation-chunk KNN (envie memories).
    pub rag_enabled: bool,
}
