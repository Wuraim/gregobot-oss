use std::time::Duration;

use chrono::Timelike;
use tracing::Instrument;

use crate::ai::context::{AiContext, UserContext};
use crate::bot::markdown;
use crate::db::Db;
use crate::i18n::Lang;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

pub fn spawn(pool: Db, bot_token: String, ai_ctx: AiContext, interval_secs: u64) {
    tokio::spawn(async move {
        tracing::info!(
            "Daily recap scheduler started (interval={interval_secs}s, max=1/day per user)"
        );

        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;

            let run_span = tracing::info_span!("daily_recap_check");
            async {
                if crate::db::sqlite_full::skip_background_tick(&pool, "Daily recap").await {
                    return;
                }
                if crate::ai::gemini_circuit::is_open() {
                    tracing::warn!("Daily recap: skipping tick — Gemini circuit open");
                    return;
                }

                let users = match crate::db::daily_recap::list_daily_recap_due(&pool).await {
                    Ok(u) => u,
                    Err(e) => {
                        if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                            tracing::error!("Daily recap: failed to list users: {e}");
                        }
                        return;
                    }
                };

                let now = chrono::Utc::now();
                let now_minutes = now.hour() as i64 * 60 + now.minute() as i64;

                for user in &users {
                    let user_span = tracing::info_span!("daily_recap_user", user_id = user.id);
                    async {
                        let user_minutes = user.daily_recap_hour * 60 + user.daily_recap_minute;
                        if now_minutes < user_minutes {
                            return;
                        }

                        let user_ctx = UserContext {
                            user_id: user.id,
                            telegram_user_id: user.telegram_user_id,
                            lang: Lang::from_iso_code(user.language_code.as_deref()),
                            web_search_enabled: user.web_search_enabled,
                            is_admin: user.role == crate::db::users::Role::Admin,
                        };
                        let bot = Bot::new(&bot_token);

                        match crate::ai::daily_recap_message(&pool, &ai_ctx, &user_ctx).await {
                            Ok(message) => {
                                let chat_id = ChatId(user.telegram_user_id);
                                let html = markdown::md_to_html(&message);
                                if html.trim().is_empty() {
                                    return;
                                }
                                match bot
                                    .send_message(chat_id, html)
                                    .parse_mode(ParseMode::Html)
                                    .await
                                {
                                    Ok(_) => {
                                        tracing::info!("Daily recap sent to user {}", user.id);
                                        if let Err(e) =
                                            crate::db::daily_recap::mark_daily_recap_sent(
                                                &pool, user.id,
                                            )
                                            .await
                                        {
                                            if !crate::db::sqlite_full::silence_if_full(&pool, &e)
                                                .await
                                            {
                                                tracing::error!(
                                                    "Daily recap: failed to mark sent for user {}: {e}",
                                                    user.id
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Daily recap: failed to send to user {}: {e}",
                                            user.id
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Daily recap: AI call failed for user {}: {e}",
                                    user.id
                                );
                            }
                        }
                    }
                    .instrument(user_span)
                    .await;
                }
            }
            .instrument(run_span)
            .await;
        }
    });
}
