use teloxide::prelude::*;
use teloxide::types::{MessageReactionUpdated, ReactionType};

use crate::ai::context::AiContext;
use crate::db::{self, Db};
use crate::i18n::{Lang, Msg};

pub async fn handle_photo(
    bot: Bot,
    msg: Message,
    pool: Db,
    ai_ctx: AiContext,
    bot_username: String,
    bot_id: i64,
) -> ResponseResult<()> {
    let is_private = msg.chat.is_private();

    let from = match msg.from.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };
    let telegram_user_id = from.id.0 as i64;
    let username = from.username.clone();
    let first_name = Some(from.first_name.clone());
    let language_code = from.language_code.clone();
    let caption = msg.caption().map(|s| s.to_string());
    // Keep the largest photo size so it can be downloaded later (AI description).
    let file_id = msg
        .photo()
        .and_then(|sizes| sizes.last())
        .map(|p| p.file.id.0.clone());

    let chat_id = msg.chat.id.0;
    let message_id = msg.id.0;
    let taken_at = msg.date.format("%Y-%m-%d %H:%M:%S").to_string();

    tracing::debug!(
        chat_id = %chat_id,
        message_id = %message_id,
        sender_tg_id = %telegram_user_id,
        username = ?username,
        has_caption = %caption.is_some(),
        has_file_id = %file_id.is_some(),
        is_private = %is_private,
        "handle_photo invoked"
    );

    crate::bot::track_group_participant(&pool, &msg).await;

    // Upsert the sender as pending so they exist in the database
    if let Err(err) =
        db::users::upsert_pending(&pool, telegram_user_id, username, first_name, language_code)
            .await
    {
        if !crate::db::sqlite_full::silence_if_full(&pool, &err).await {
            tracing::error!("Failed to upsert photo sender as pending: {err}");
        }
    }

    // Always save so sport-emoji reaction can count training once (private + group).
    let photo_id = match db::photo::save_photo_message(
        &pool,
        chat_id,
        message_id,
        telegram_user_id,
        caption.clone(),
        file_id.clone(),
        Some(&taken_at),
    )
    .await
    {
        Ok(id) => {
            tracing::debug!(
                chat_id = %chat_id,
                message_id = %message_id,
                photo_id = %id,
                taken_at = %taken_at,
                "handle_photo: photo message saved"
            );
            Some(id)
        }
        Err(err) => {
            tracing::error!("Failed to save photo message: {err}");
            None
        }
    };

    let jpeg = match file_id.as_deref() {
        Some(fid) => match ingest_telegram_photo(&bot, fid).await {
            Ok(bytes) => {
                if let Err(err) =
                    db::photo::save_image_jpeg(&pool, chat_id, message_id, &bytes).await
                {
                    tracing::warn!(
                        chat_id = %chat_id,
                        message_id = %message_id,
                        "Failed to store photo JPEG: {err}"
                    );
                } else if let (Some(photo_id), Some(api_key)) = (photo_id, ai_ctx.api_key()) {
                    let user_id = db::users::find_by_telegram_id(&pool, telegram_user_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|u| u.id);
                    if let Some(user_id) = user_id {
                        db::photo::spawn_embed_photo(
                            pool.clone(),
                            api_key.to_string(),
                            ai_ctx.embedding_model.clone(),
                            user_id,
                            photo_id,
                            bytes.clone(),
                            caption.clone(),
                        );
                    }
                }
                Some(bytes)
            }
            Err(err) => {
                tracing::warn!(
                    chat_id = %chat_id,
                    message_id = %message_id,
                    "Failed to download/normalize photo: {err:#}"
                );
                None
            }
        },
        None => None,
    };

    // Private: always LLM. Group: LLM when mentioned or reply-to-bot.
    // Sport reaction path stays independent via training_counted.
    if is_private || crate::bot::mentions_bot(&msg, &bot_username, bot_id) {
        return crate::bot::handlers::handle_ai_photo(bot, msg, pool, ai_ctx, bot_username, jpeg)
            .await;
    }

    Ok(())
}

pub async fn handle_reaction(
    bot: Bot,
    msg: MessageReactionUpdated,
    pool: Db,
    ai_ctx: AiContext,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id.0;
    let message_id = msg.message_id.0;

    // NOTE: Telegram only sends message_reaction updates to bots that are
    // administrators in the chat. If reactions are not processed in a group,
    // first check whether the bot has admin rights there.
    tracing::debug!(
        chat_id = %chat_id,
        message_id = %message_id,
        reactor_id = ?msg.user().as_ref().map(|u| u.id.0 as i64),
        "handle_reaction invoked"
    );

    // Load the configured sport reaction emoji for this chat (default 🏆)
    let sport_emoji = db::chat_settings::get_sport_emoji(&pool, chat_id).await;

    // Check if the new reaction contains the configured emoji
    let has_sport_emoji = msg
        .new_reaction
        .iter()
        .any(|r| matches!(r, ReactionType::Emoji { emoji } if *emoji == sport_emoji));

    if !has_sport_emoji {
        tracing::debug!(
            chat_id = %chat_id,
            message_id = %message_id,
            sport_emoji = %sport_emoji,
            new_reaction = ?msg.new_reaction,
            "handle_reaction: configured sport emoji not present, ignoring"
        );
        return Ok(());
    }

    // Look up the tracked photo
    let photo = match db::photo::find_photo_message(&pool, chat_id, message_id).await {
        Ok(Some(record)) => {
            tracing::debug!(
                chat_id = %chat_id,
                message_id = %message_id,
                sender_tg_id = %record.sender_telegram_user_id,
                "handle_reaction: tracked photo found"
            );
            record
        }
        Ok(None) => {
            tracing::debug!(
                chat_id = %chat_id,
                message_id = %message_id,
                "handle_reaction: photo not tracked, ignoring"
            );
            return Ok(()); // not a tracked photo
        }
        Err(err) => {
            tracing::error!("Failed to find photo message: {err}");
            return Ok(());
        }
    };

    // Upsert the reactor as pending (they might not be in the DB yet)
    let reactor_tg_id = msg.user().as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let reactor_username = msg.user().as_ref().and_then(|u| u.username.clone());
    let reactor_first_name = msg.user().as_ref().map(|u| u.first_name.clone());
    let reactor_lang_code = msg.user().as_ref().and_then(|u| u.language_code.clone());

    if reactor_tg_id == 0 {
        return Ok(());
    }

    if let Err(err) = db::users::upsert_pending(
        &pool,
        reactor_tg_id,
        reactor_username.clone(),
        reactor_first_name,
        reactor_lang_code.clone(),
    )
    .await
    {
        if !crate::db::sqlite_full::silence_if_full(&pool, &err).await {
            tracing::error!("Failed to upsert reactor as pending: {err}");
        }
    }

    // Count each photo only once, whoever reacts with the sport emoji.
    let is_new = match db::photo::try_mark_training_counted(&pool, chat_id, message_id).await {
        Ok(is_new) => is_new,
        Err(err) => {
            tracing::error!("Failed to mark photo as counted: {err}");
            return Ok(());
        }
    };

    if !is_new {
        tracing::debug!(
            chat_id = %chat_id,
            message_id = %message_id,
            "handle_reaction: photo already counted as training, ignoring"
        );
        return Ok(());
    }

    // Find the sender in users table
    let sender = match db::users::find_by_telegram_id(&pool, photo.sender_telegram_user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            tracing::warn!(
                "Photo sender {} not found in users table, cannot create training entry",
                photo.sender_telegram_user_id
            );
            return Ok(());
        }
        Err(err) => {
            tracing::error!("Failed to find sender: {err}");
            return Ok(());
        }
    };

    // Build the reactor display string
    let reactor_display = reactor_username
        .as_ref()
        .map(|u| format!("@{u}"))
        .unwrap_or_else(|| reactor_tg_id.to_string());

    let sender_lang = sender.lang();
    let description_fallback =
        Msg::new(sender_lang).training_photo_reaction(&reactor_display, &sport_emoji);

    let scope = db::photo::PhotoSearchScope::from_chat_id(Some(chat_id));
    match db::photo::similar_counted_training(&pool, sender.id, &photo, scope).await {
        Ok(Some(dup)) => {
            tracing::info!(
                photo_id = photo.id,
                dup_photo_id = dup.photo_id,
                distance = dup.distance,
                "handle_reaction: visually similar training photo already counted"
            );
        }
        Ok(None) => {
            let new_entry = match extract_photo_training(
                &bot,
                &pool,
                &ai_ctx,
                sender.id,
                sender_lang,
                chat_id,
                message_id,
                &photo,
                &description_fallback,
            )
            .await
            {
                Some(entry) => entry,
                None => db::training::NewTrainingEntry::simple(
                    sender.id,
                    description_fallback,
                    photo.taken_at_utc(),
                ),
            };

            let entry = db::training::try_insert(&pool, new_entry).await;

            match entry {
                Ok(db::training::TrainingInsert::Created(_)) => {}
                Ok(db::training::TrainingInsert::Duplicate(existing)) => {
                    tracing::info!(
                        training_id = existing.id,
                        "handle_reaction: consecutive duplicate training, reusing existing entry"
                    );
                }
                Err(err) => {
                    tracing::error!("Failed to create training entry from reaction: {err}");
                }
            }
        }
        Err(err) => {
            tracing::warn!("Photo similarity check failed: {err}");
            let new_entry = match extract_photo_training(
                &bot,
                &pool,
                &ai_ctx,
                sender.id,
                sender_lang,
                chat_id,
                message_id,
                &photo,
                &description_fallback,
            )
            .await
            {
                Some(entry) => entry,
                None => db::training::NewTrainingEntry::simple(
                    sender.id,
                    description_fallback,
                    photo.taken_at_utc(),
                ),
            };
            if let Err(err) = db::training::try_insert(&pool, new_entry).await {
                tracing::error!("Failed to create training entry from reaction: {err}");
            }
        }
    }

    // Confirm with a 👍 reaction on the original message
    let result = bot
        .set_message_reaction(msg.chat.id, msg.message_id)
        .reaction(vec![ReactionType::Emoji {
            emoji: "👍".to_string(),
        }])
        .await;

    if let Err(err) = result {
        tracing::error!("Failed to set 👍 reaction: {err}");
    }

    Ok(())
}

/// Try to extract structured training stats from the photo. Returns None on any failure
/// so the caller can fall back to a plain description.
async fn extract_photo_training(
    bot: &Bot,
    pool: &Db,
    ai_ctx: &AiContext,
    sender_id: i64,
    sender_lang: Lang,
    chat_id: i64,
    message_id: i32,
    photo: &db::photo::PhotoMessage,
    fallback_description: &str,
) -> Option<db::training::NewTrainingEntry> {
    ai_ctx.api_key()?;

    let image_bytes = if let Some(jpeg) = photo.image_jpeg.clone() {
        jpeg
    } else {
        let file_id = photo.file_id.as_deref()?;
        match ingest_telegram_photo(bot, file_id).await {
            Ok(bytes) => {
                if let Err(err) =
                    db::photo::save_image_jpeg(pool, chat_id, message_id, &bytes).await
                {
                    tracing::warn!("Failed to backfill photo JPEG: {err}");
                }
                bytes
            }
            Err(err) => {
                tracing::warn!("Failed to download photo for AI description: {err:#}");
                return None;
            }
        }
    };

    let user_context = photo
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());

    match crate::ai::training_extract::extract_training_from_media(
        pool,
        ai_ctx,
        sender_id,
        sender_lang,
        user_context,
        Some((&image_bytes, crate::media::JPEG_MIME)),
    )
    .await
    {
        Ok(extracted) => Some(extracted.into_new_entry(
            sender_id,
            fallback_description,
            photo.taken_at_utc(),
            false,
        )),
        Err(err) => {
            tracing::warn!("AI photo training extract failed, using default text: {err:#}");
            None
        }
    }
}

/// Download a Telegram photo and re-encode it to the stored JPEG format.
pub async fn ingest_telegram_photo(bot: &Bot, file_id: &str) -> anyhow::Result<Vec<u8>> {
    let raw = download_telegram_file(bot, file_id).await?;
    tokio::task::spawn_blocking(move || crate::media::normalize_to_jpeg(&raw))
        .await
        .map_err(|e| anyhow::anyhow!("jpeg normalize task failed: {e}"))?
}

/// Download a file from Telegram by file_id.
pub async fn download_telegram_file(bot: &Bot, file_id: &str) -> anyhow::Result<Vec<u8>> {
    let file = bot
        .get_file(teloxide::types::FileId(file_id.to_string()))
        .await?;
    let url = format!(
        "https://api.telegram.org/file/bot{}/{}",
        bot.token(),
        file.path
    );
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        anyhow::bail!(
            "Telegram file download failed with status {}",
            response.status()
        );
    }
    Ok(response.bytes().await?.to_vec())
}
