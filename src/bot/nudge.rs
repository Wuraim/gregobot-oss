use std::time::Duration;

use chrono::Utc;
use tracing::Instrument;

use crate::ai::context::{AiContext, UserContext};
use crate::bot::markdown;
use crate::db::Db;
use crate::i18n::Lang;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

pub fn spawn(pool: Db, bot_token: String, ai_ctx: AiContext, interval_secs: u64) {
    tokio::spawn(async move {
        tracing::info!("Nudge scheduler started (interval={interval_secs}s)");

        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;

            let run_span = tracing::info_span!("nudge_check");
            async {
                if crate::db::sqlite_full::skip_background_tick(&pool, "Nudge").await {
                    return;
                }
                if crate::ai::gemini_circuit::is_open() {
                    tracing::warn!("Nudge: skipping tick — Gemini circuit open");
                    return;
                }

                let users = match crate::db::users::list(&pool).await {
                    Ok(u) => u,
                    Err(e) => {
                        if !crate::db::sqlite_full::silence_if_full(&pool, &e).await {
                            tracing::error!("Nudge scheduler: failed to list users: {e}");
                        }
                        return;
                    }
                };

                let today = Utc::now().date_naive();

                for user in &users {
                    let user_span = tracing::info_span!("nudge_user", user_id = user.id);
                    async {
                        if user.role == crate::db::users::Role::Pending {
                            return;
                        }

                        if !user.nudges_enabled {
                            return;
                        }

                        let already_sent_today = user
                            .last_nudge_at
                            .as_deref()
                            .and_then(|s| s.get(..10))
                            .and_then(|d| d.parse::<chrono::NaiveDate>().ok())
                            .is_some_and(|d| d >= today);
                        if already_sent_today {
                            return;
                        }

                        let latest_weight = crate::db::weight::latest_for_user(&pool, user.id)
                            .await
                            .ok()
                            .flatten()
                            .map(|e| e.recorded_at.date_naive());
                        let latest_training = crate::db::training::latest_for_user(&pool, user.id)
                            .await
                            .ok()
                            .flatten()
                            .map(|e| e.recorded_at.date_naive());

                        let has_any_history = latest_weight.is_some() || latest_training.is_some();
                        if !has_any_history {
                            return;
                        }

                        let threshold = user.nudge_inactivity_days.max(1);
                        let weight_inactive =
                            latest_weight.is_none_or(|d| (today - d).num_days() >= threshold);
                        let training_inactive =
                            latest_training.is_none_or(|d| (today - d).num_days() >= threshold);

                        if !weight_inactive || !training_inactive {
                            return;
                        }

                        let user_ctx = UserContext {
                            user_id: user.id,
                            telegram_user_id: user.telegram_user_id,
                            lang: Lang::from_iso_code(user.language_code.as_deref()),
                            web_search_enabled: user.web_search_enabled,
                            is_admin: user.role == crate::db::users::Role::Admin,
                        };

                        match crate::ai::nudge_message(&pool, &ai_ctx, &user_ctx, threshold).await {
                            Ok(message) => {
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
                                        tracing::info!("Nudge sent to user {}", user.id);
                                        if let Err(e) =
                                            crate::db::users::mark_nudge_sent(&pool, user.id).await
                                        {
                                            if !crate::db::sqlite_full::silence_if_full(&pool, &e)
                                                .await
                                            {
                                                tracing::warn!(
                                                    "Nudge: failed to mark sent for user {}: {e}",
                                                    user.id
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        if crate::bot::telegram_errors::is_unreachable_chat(&e) {
                                            tracing::warn!(
                                                "Nudge: unreachable chat for user {} — disabling nudges and proactive: {e}",
                                                user.id
                                            );
                                            let _ = crate::db::users::set_nudge_settings(
                                                &pool,
                                                user.telegram_user_id,
                                                Some(false),
                                                None,
                                            )
                                            .await;
                                            let _ = crate::db::proactive::disable(&pool, user.id).await;
                                            let _ = crate::db::users::mark_nudge_sent(&pool, user.id).await;
                                        } else {
                                            tracing::warn!(
                                                "Nudge: failed to send to user {}: {e}",
                                                user.id
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Nudge: AI call failed for user {}: {e}", user.id);
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
