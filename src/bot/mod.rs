use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use axum::Router;
use teloxide::prelude::*;
use teloxide::types::{AllowedUpdate, MessageEntityKind};
use teloxide::update_listeners::webhooks;

use crate::ai::context::AiContext;
use crate::config::{BotMode, Config};
use crate::db::Db;

pub mod command_history;
pub mod commands;

/// Passively track the sender of a group message as a participant of that chat,
/// and sync their Telegram username into the users table (as pending if new).
pub(crate) async fn track_group_participant(pool: &Db, msg: &Message) {
    if msg.chat.is_private() {
        return;
    }

    let chat_id = msg.chat.id.0;
    if let Some(user) = msg.from.as_ref() {
        let telegram_user_id = user.id.0 as i64;
        if let Err(e) = crate::db::group_members::upsert(pool, chat_id, telegram_user_id).await {
            tracing::warn!(
                chat_id = %chat_id,
                telegram_user_id = %telegram_user_id,
                error = %e,
                "Failed to track group participant"
            );
        }
        // Discover / refresh @username and first_name even when the user only talks in groups.
        if let Err(e) = crate::db::users::upsert_pending(
            pool,
            telegram_user_id,
            user.username.clone(),
            Some(user.first_name.clone()),
            user.language_code.clone(),
        )
        .await
        {
            tracing::warn!(
                telegram_user_id = %telegram_user_id,
                username = ?user.username,
                error = %e,
                "Failed to sync group participant profile"
            );
        }
    }
}
pub mod daily_recap;
pub mod handlers;
pub mod keyboard;
pub mod markdown;
pub mod monthly_graph;
pub mod nudge;
pub mod nutrition_table;
pub mod proactive;
pub mod reactions;
pub mod scheduler;
pub mod telegram_errors;
pub mod training_graph;
pub mod weight_graph;

pub(crate) fn mentions_bot(msg: &Message, bot_username: &str, bot_id: i64) -> bool {
    let text = msg.text().unwrap_or("");
    let caption = msg.caption().unwrap_or("");

    if text.starts_with('/') {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            "mentions_bot: dropped message starting with '/'"
        );
        return false;
    }

    if msg.chat.is_private() {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            is_private = true,
            "mentions_bot: accepted private chat message"
        );
        return true;
    }

    if msg
        .reply_to_message()
        .and_then(|replied| replied.from.as_ref())
        .is_some_and(|user| user.id.0 as i64 == bot_id)
    {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            bot_id = %bot_id,
            "mentions_bot: accepted reply to bot message"
        );
        return true;
    }

    let bot_username_lower = bot_username.to_lowercase();

    if !text.is_empty() && text.to_lowercase().contains(&bot_username_lower) {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            bot_username = %bot_username,
            "mentions_bot: accepted text mention"
        );
        return true;
    }

    if !caption.is_empty() && caption.to_lowercase().contains(&bot_username_lower) {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            bot_username = %bot_username,
            "mentions_bot: accepted caption mention"
        );
        return true;
    }

    let has_text_mention = msg
        .entities()
        .map(|entities| {
            entities.iter().any(|entity| {
                matches!(
                    &entity.kind,
                    MessageEntityKind::TextMention { user } if user.id.0 as i64 == bot_id
                )
            })
        })
        .unwrap_or(false);

    if has_text_mention {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            bot_id = %bot_id,
            "mentions_bot: accepted text_mention entity"
        );
        return true;
    }

    let has_caption_mention = msg
        .caption_entities()
        .map(|entities| {
            entities.iter().any(|entity| {
                matches!(
                    &entity.kind,
                    MessageEntityKind::TextMention { user } if user.id.0 as i64 == bot_id
                )
            })
        })
        .unwrap_or(false);

    if has_caption_mention {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            bot_id = %bot_id,
            "mentions_bot: accepted caption text_mention entity"
        );
        return true;
    }

    tracing::debug!(
        chat_id = %msg.chat.id.0,
        text = %text,
        caption = %caption,
        bot_username = %bot_username,
        "mentions_bot: no mention found, dropping"
    );
    false
}

/// Build the Teloxide dispatcher and return it alongside the Axum router
/// fragment that serves the webhook endpoint (or an empty router in polling mode).
pub async fn build(
    config: &Config,
    pool: Db,
) -> Result<(Router, Pin<Box<dyn Future<Output = ()> + Send + 'static>>)> {
    tracing::info!(" bot::build entry, mode={:?}", config.bot_mode);
    let bot = Bot::new(&config.bot_token);
    let me = {
        let mut last_err = None;
        let mut resolved = None;

        for attempt in 0u32..3 {
            match bot.get_me().await {
                Ok(me) => {
                    resolved = Some(me);
                    break;
                }
                Err(err) => {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = ?err,
                        "Failed to fetch bot identity"
                    );
                    last_err = Some(err);

                    if attempt < 2 {
                        tokio::time::sleep(std::time::Duration::from_secs(1u64 << attempt)).await;
                    }
                }
            }
        }

        resolved.ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to fetch bot identity after 3 attempts: {:?}",
                last_err
            )
        })?
    };
    let bot_username = me.username().to_string();
    if bot_username.trim().is_empty() {
        anyhow::bail!("Bot username is empty; group mentions will not work");
    }
    let bot_id = me.id.0 as i64;
    tracing::info!(" bot identity: @{bot_username}, id={bot_id}");
    tracing::info!("Bot identity resolved as @{}", bot_username);

    match config.bot_mode {
        BotMode::Webhook => {
            tracing::info!("bot::build webhook mode");
            let addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.port).parse()?;
            let webhook_url: url::Url = config.webhook_full_url().parse()?;

            // Generate the secret token upfront so both set_webhook calls use
            // the same value. Without this, the second set_webhook (for
            // allowed_updates) would omit secret_token, causing Telegram to
            // stop sending the X-Telegram-Bot-Api-Secret-Token header — every
            // incoming update would then get 401 Unauthorized.
            let mut options = webhooks::Options::new(addr, webhook_url.clone());
            let secret_token = options.get_or_gen_secret_token().to_owned();

            let (listener, stop_flag, webhook_router) =
                webhooks::axum_to_router(bot.clone(), options)
                    .await
                    .inspect_err(|e| tracing::error!("Failed to set up Telegram webhook: {e}"))?;
            tracing::info!("webhook axum_to_router done");

            // Configure allowed_updates after startup to avoid Telegram rate limits.
            // axum_to_router calls set_webhook internally; a second call in the same
            // async context triggers 429 (Retry after 1s). Running it in a spawned
            // task with a delay bypasses this.
            let bot_for_webhook = bot.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Err(e) = bot_for_webhook
                    .set_webhook(webhook_url)
                    .allowed_updates([
                        AllowedUpdate::Message,
                        AllowedUpdate::CallbackQuery,
                        AllowedUpdate::MessageReaction,
                    ])
                    .secret_token(secret_token)
                    .await
                {
                    tracing::error!(
                        "Failed to update webhook allowed_updates (inline keyboards and reactions will not work): {e}"
                    );
                } else {
                    tracing::info!("Webhook allowed_updates configured successfully");
                }
            });

            let pool_clone_cmd = pool.clone();
            let pool_clone_photo = pool.clone();
            let pool_clone_ai = pool.clone();
            let pool_clone_track = pool.clone();
            let ai_ctx = AiContext::from_config(config);
            let ai_ctx_cmd = ai_ctx.clone();
            let ai_ctx_ai_cmd = ai_ctx.clone();
            let ai_ctx_photo = ai_ctx.clone();
            let bot_username_filter = bot_username.clone();
            let bot_username_ai = bot_username.clone();
            let bot_username_photo = bot_username.clone();
            let command_handler = Update::filter_message()
                .branch(
                    dptree::entry()
                        .filter_command::<commands::Command>()
                        .endpoint(move |bot, msg, cmd| {
                            let pool = pool_clone_cmd.clone();
                            let ai_ctx = ai_ctx_cmd.clone();
                            handlers::handle_command(bot, msg, cmd, pool, ai_ctx)
                        }),
                )
                .branch(
                    dptree::entry()
                        .filter(|msg: Message| msg.photo().is_some())
                        .endpoint(move |bot, msg| {
                            let pool = pool_clone_photo.clone();
                            let ai_ctx = ai_ctx_photo.clone();
                            let bot_username = bot_username_photo.clone();
                            reactions::handle_photo(bot, msg, pool, ai_ctx, bot_username, bot_id)
                        }),
                )
                .branch(
                    dptree::entry()
                        .filter(move |msg: Message| {
                            mentions_bot(&msg, &bot_username_filter, bot_id)
                        })
                        .endpoint(move |bot, msg| {
                            let pool = pool_clone_ai.clone();
                            let ai_ctx = ai_ctx_ai_cmd.clone();
                            let bot_username = bot_username_ai.clone();
                            handlers::handle_ai_message(bot, msg, pool, ai_ctx, bot_username)
                        }),
                )
                .branch(dptree::entry().endpoint(move |msg: Message| {
                    let pool = pool_clone_track.clone();
                    async move {
                        track_group_participant(&pool, &msg).await;
                        Ok::<(), teloxide::RequestError>(())
                    }
                }));

            let pool_clone_reaction = pool.clone();
            let ai_ctx_reaction = ai_ctx.clone();
            let reaction_handler =
                Update::filter_message_reaction_updated().endpoint(move |bot, msg| {
                    let pool = pool_clone_reaction.clone();
                    let ai_ctx = ai_ctx_reaction.clone();
                    reactions::handle_reaction(bot, msg, pool, ai_ctx)
                });
            let pool_clone_callback = pool.clone();
            let ai_ctx_callback = ai_ctx.clone();
            let callback_handler = Update::filter_callback_query().endpoint(move |bot, query| {
                let pool = pool_clone_callback.clone();
                let ai_ctx = ai_ctx_callback.clone();
                keyboard::handle_callback(bot, query, pool, ai_ctx)
            });

            let schema = dptree::entry()
                .branch(command_handler)
                .branch(reaction_handler)
                .branch(callback_handler);

            let dispatcher_future = Box::pin(async move {
                tracing::info!(" dispatcher_future starting dispatch_with_listener");
                Dispatcher::builder(bot, schema)
                    .enable_ctrlc_handler()
                    .build()
                    .dispatch_with_listener(
                        listener,
                        LoggingErrorHandler::with_custom_text("Dispatcher error"),
                    )
                    .await;
                tracing::info!(" dispatcher_future dispatch_with_listener returned");

                drop(stop_flag);
                tracing::info!(" dispatcher_future ending");
            });

            tracing::info!("bot::build webhook returning Ok");
            Ok((webhook_router, dispatcher_future))
        }
        BotMode::Polling => {
            bot.delete_webhook()
                .await
                .inspect_err(|e| tracing::error!("Failed to delete webhook before polling: {e}"))
                .ok();
            tracing::info!("Bot running in polling mode");

            let pool_clone_cmd = pool.clone();
            let pool_clone_photo = pool.clone();
            let pool_clone_ai = pool.clone();
            let pool_clone_track = pool.clone();
            let ai_ctx = AiContext::from_config(config);
            let ai_ctx_cmd = ai_ctx.clone();
            let ai_ctx_ai_cmd = ai_ctx.clone();
            let ai_ctx_photo = ai_ctx.clone();
            let bot_username_filter = bot_username.clone();
            let bot_username_ai = bot_username.clone();
            let bot_username_photo = bot_username.clone();
            let command_handler = Update::filter_message()
                .branch(
                    dptree::entry()
                        .filter_command::<commands::Command>()
                        .endpoint(move |bot, msg, cmd| {
                            let pool = pool_clone_cmd.clone();
                            let ai_ctx = ai_ctx_cmd.clone();
                            handlers::handle_command(bot, msg, cmd, pool, ai_ctx)
                        }),
                )
                .branch(
                    dptree::entry()
                        .filter(|msg: Message| msg.photo().is_some())
                        .endpoint(move |bot, msg| {
                            let pool = pool_clone_photo.clone();
                            let ai_ctx = ai_ctx_photo.clone();
                            let bot_username = bot_username_photo.clone();
                            reactions::handle_photo(bot, msg, pool, ai_ctx, bot_username, bot_id)
                        }),
                )
                .branch(
                    dptree::entry()
                        .filter(move |msg: Message| {
                            mentions_bot(&msg, &bot_username_filter, bot_id)
                        })
                        .endpoint(move |bot, msg| {
                            let pool = pool_clone_ai.clone();
                            let ai_ctx = ai_ctx_ai_cmd.clone();
                            let bot_username = bot_username_ai.clone();
                            handlers::handle_ai_message(bot, msg, pool, ai_ctx, bot_username)
                        }),
                )
                .branch(dptree::entry().endpoint(move |msg: Message| {
                    let pool = pool_clone_track.clone();
                    async move {
                        track_group_participant(&pool, &msg).await;
                        Ok::<(), teloxide::RequestError>(())
                    }
                }));

            let pool_clone_reaction = pool.clone();
            let ai_ctx_reaction = ai_ctx.clone();
            let reaction_handler =
                Update::filter_message_reaction_updated().endpoint(move |bot, msg| {
                    let pool = pool_clone_reaction.clone();
                    let ai_ctx = ai_ctx_reaction.clone();
                    reactions::handle_reaction(bot, msg, pool, ai_ctx)
                });
            let pool_clone_callback = pool.clone();
            let ai_ctx_callback = ai_ctx.clone();
            let callback_handler = Update::filter_callback_query().endpoint(move |bot, query| {
                let pool = pool_clone_callback.clone();
                let ai_ctx = ai_ctx_callback.clone();
                keyboard::handle_callback(bot, query, pool, ai_ctx)
            });

            let schema = dptree::entry()
                .branch(command_handler)
                .branch(reaction_handler)
                .branch(callback_handler);

            let dispatcher_future = Box::pin(async move {
                Dispatcher::builder(bot, schema)
                    .enable_ctrlc_handler()
                    .build()
                    .dispatch()
                    .await;
            });

            Ok((Router::new(), dispatcher_future))
        }
    }
}
