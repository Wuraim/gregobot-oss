use teloxide::prelude::*;
use teloxide::types::ReplyMarkup;

use crate::ai::context::AiContext;
use crate::db::{self, Db, users::User};

/// Tracks one slash-command exchange for AI conversation history.
pub struct CommandHistory<'a> {
    pool: &'a Db,
    user_id: i64,
    chat_id: Option<i64>,
    command_text: String,
    user_recorded: bool,
    rag: Option<RagEmbed>,
}

struct RagEmbed {
    api_key: String,
    embedding_model: String,
}

impl<'a> CommandHistory<'a> {
    pub fn new(
        pool: &'a Db,
        user: &User,
        msg: &Message,
        speaker: String,
        ai_ctx: &AiContext,
    ) -> Self {
        let raw = msg.text().unwrap_or("/command").to_string();
        let labeled = db::conversations::content_with_speaker("user", &raw, &speaker);
        let rag = if ai_ctx.rag_enabled {
            ai_ctx.api_key().map(|key| RagEmbed {
                api_key: key.to_string(),
                embedding_model: ai_ctx.embedding_model.clone(),
            })
        } else {
            None
        };
        Self {
            pool,
            user_id: user.id,
            chat_id: if msg.chat.is_private() {
                None
            } else {
                Some(msg.chat.id.0)
            },
            command_text: labeled,
            user_recorded: false,
            rag,
        }
    }

    /// Persist the command (once) and this bot reply into conversation history.
    pub async fn note_reply(&mut self, reply: &str) {
        if reply.trim().is_empty() {
            return;
        }

        if !self.user_recorded {
            match db::conversations::insert(
                self.pool,
                self.user_id,
                "user",
                &self.command_text,
                self.chat_id,
            )
            .await
            {
                Ok(id) => {
                    self.user_recorded = true;
                    self.maybe_embed(id, "user", &self.command_text);
                }
                Err(e) => {
                    tracing::warn!(
                        user_id = %self.user_id,
                        "Failed to save command to conversation history: {e}"
                    );
                }
            }
        }

        match db::conversations::insert(self.pool, self.user_id, "assistant", reply, self.chat_id)
            .await
        {
            Ok(id) => self.maybe_embed(id, "assistant", reply),
            Err(e) => {
                tracing::warn!(
                    user_id = %self.user_id,
                    "Failed to save command reply to conversation history: {e}"
                );
            }
        }
    }

    fn maybe_embed(&self, source_id: i64, role: &'static str, content: &str) {
        let Some(rag) = &self.rag else {
            return;
        };
        crate::ai::spawn_embed_conversation_turn(
            self.pool.clone(),
            rag.api_key.clone(),
            rag.embedding_model.clone(),
            self.user_id,
            self.chat_id,
            role,
            content.to_string(),
            source_id,
        );
    }
}

/// Send a plain text command reply and record it in chat history.
pub async fn cmd_reply(
    bot: &Bot,
    msg: &Message,
    hist: &mut CommandHistory<'_>,
    text: impl Into<String>,
) -> ResponseResult<Message> {
    let text = text.into();
    let sent = bot.send_message(msg.chat.id, text.clone()).await?;
    hist.note_reply(&text).await;
    Ok(sent)
}

/// Send a command reply with reply markup and record the text in chat history.
pub async fn cmd_reply_markup(
    bot: &Bot,
    msg: &Message,
    hist: &mut CommandHistory<'_>,
    text: impl Into<String>,
    markup: impl Into<ReplyMarkup>,
) -> ResponseResult<Message> {
    let text = text.into();
    let sent = bot
        .send_message(msg.chat.id, text.clone())
        .reply_markup(markup)
        .await?;
    hist.note_reply(&text).await;
    Ok(sent)
}
