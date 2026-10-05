use std::time::Duration;

use tracing::Instrument;

use crate::ai::context::{AiContext, UserContext};
use crate::bot::markdown;
use crate::db::Db;
use crate::i18n::Lang;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

const CHECK_INTERVAL_MINS: u64 = 60;

pub fn spawn(pool: Db, bot_token: String, ai_ctx: AiContext) {
    tokio::spawn(async move {
        tracing::info!(
            "Proactive scheduler started (interval={CHECK_INTERVAL_MINS}m, max=1/day per user)"
        );

        loop {
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_MINS * 60)).await;

            let run_span = tracing::info_span!("proactive_check");
            async {
                if crate::db::sqlite_full::skip_background_tick(&pool, "Proactive").await {
                    return;
                }
                if crate::ai::gemini_circuit::is_open() {
                    tracing::warn!("Proactive: skipping tick — Gemini circuit open");
                    return;
                }

                let users = match crate::db::users::list(&pool).await {
                    Ok(u) => u,
                    Err(e) => {
                        if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                            tracing::error!("Proactive scheduler: failed to list users: {e}");
                        }
                        return;
                    }
                };

                for user in &users {
                    if crate::ai::gemini_circuit::is_open() {
                        tracing::warn!(
                            "Proactive: stopping remaining users — Gemini circuit open"
                        );
                        break;
                    }

                    let user_span = tracing::info_span!("proactive_user", user_id = user.id);
                    async {
                        if user.role == crate::db::users::Role::Pending {
                            return;
                        }

                        match crate::db::proactive::is_enabled(&pool, user.id).await {
                            Ok(true) => {}
                            Ok(false) => return,
                            Err(e) => {
                                if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                                    tracing::error!(
                                        "Proactive: is_enabled({}) failed: {e}",
                                        user.id
                                    );
                                }
                                return;
                            }
                        }

                        let can_send = match crate::db::proactive::can_send_today(&pool, user.id).await {
                            Ok(can) => can,
                            Err(e) => {
                                if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                                    tracing::error!(
                                        "Proactive: can_send_today({}) failed: {e}",
                                        user.id
                                    );
                                }
                                return;
                            }
                        };

                        if !can_send {
                            return;
                        }

                        let user_ctx = UserContext {
                            user_id: user.id,
                            telegram_user_id: user.telegram_user_id,
                            lang: Lang::from_iso_code(user.language_code.as_deref()),
                            web_search_enabled: user.web_search_enabled,
                            is_admin: user.role == crate::db::users::Role::Admin,
                        };

                        match crate::ai::proactive_message(&pool, &ai_ctx, &user_ctx).await {
                            Ok(Some(message)) => {
                                let bot = Bot::new(&bot_token);
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
                                        tracing::info!("Proactive message sent to user {}", user.id);
                                        if let Err(e) =
                                            crate::db::proactive::record(&pool, user.id, Some(message))
                                                .await
                                        {
                                            if !crate::db::sqlite_full::silence_if_full(&pool, &e)
                                                .await
                                            {
                                                tracing::error!(
                                                    "Proactive: failed to record message for user {}: {e}",
                                                    user.id
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if crate::bot::telegram_errors::is_unreachable_chat(&e) {
                                            tracing::warn!(
                                                "Proactive: unreachable chat for user {} — disabling proactive and nudges: {e}",
                                                user.id
                                            );
                                            if let Err(e2) =
                                                crate::db::proactive::disable(&pool, user.id).await
                                            {
                                                tracing::error!(
                                                    "Proactive: failed to disable user {}: {e2}",
                                                    user.id
                                                );
                                            }
                                            let _ = crate::db::users::set_nudge_settings(
                                                &pool,
                                                user.telegram_user_id,
                                                Some(false),
                                                None,
                                            )
                                            .await;
                                        } else {
                                            tracing::warn!(
                                                "Proactive: failed to send to user {}: {e}",
                                                user.id
                                            );
                                        }
                                        if let Err(e2) =
                                            crate::db::proactive::record(&pool, user.id, None).await
                                        {
                                            if !crate::db::sqlite_full::silence_if_full(&pool, &e2)
                                                .await
                                            {
                                                tracing::error!(
                                                    "Proactive: failed to record attempt for user {}: {e2}",
                                                    user.id
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::info!("Proactive: AI skipped for user {}", user.id);
                                if let Err(e) =
                                    crate::db::proactive::record(&pool, user.id, None).await
                                {
                                    if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                                        tracing::error!(
                                            "Proactive: failed to record skip for user {}: {e}",
                                            user.id
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Proactive: AI call failed for user {}: {e}", user.id);
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
