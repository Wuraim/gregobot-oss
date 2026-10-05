use std::time::Duration;

use teloxide::prelude::*;
use teloxide::types::ChatId;
use tracing::Instrument;

use crate::db::Db;
use crate::i18n::{Lang, Msg};
use crate::productivity::domain::ReminderTarget;
use crate::productivity::reminder_service;

const MAX_RETRY_ATTEMPTS: u32 = 3;

pub fn spawn(pool: Db, bot_token: String, interval_secs: u64) {
    let bot = Bot::new(&bot_token);

    tokio::spawn(async move {
        tracing::info!(
            "Reminder scheduler started (interval={interval_secs}s, max_retries={MAX_RETRY_ATTEMPTS})"
        );

        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;

            let run_span = tracing::info_span!("reminder_check");
            async {
                match reminder_service::list_due(&pool, 50).await {
                    Ok(due) => {
                        if due.is_empty() {
                            return;
                        }
                        tracing::info!("Found {} due reminders", due.len());

                        for reminder in due {
                            process_reminder(&bot, &pool, reminder).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to list due reminders: {e}");
                    }
                }
            }
            .instrument(run_span)
            .await;
        }
    });
}

#[tracing::instrument(
    skip(bot, pool, reminder),
    fields(reminder_id = reminder.id, user_id = reminder.user_id)
)]
async fn process_reminder(bot: &Bot, pool: &Db, reminder: crate::productivity::domain::Reminder) {
    let reminder_id = reminder.id;
    let user_id = reminder.user_id;

    let user = match crate::db::users::find_by_id(pool, user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            tracing::warn!(
                "Reminder #{reminder_id}: user_id={user_id} not found, marking as failed"
            );
            let _ = crate::db::reminders::mark_failed(pool, reminder_id).await;
            return;
        }
        Err(e) => {
            tracing::error!("Reminder #{reminder_id}: failed to look up user: {e}");
            return;
        }
    };

    let lang = Lang::from_iso_code(user.language_code.as_deref());
    let msg_lang = Msg::new(lang);

    let message = match &reminder.target {
        ReminderTarget::Custom { message: msg } => msg_lang.remind_target_custom(msg),
        ReminderTarget::Task { task_id } => {
            match crate::db::tasks::find_by_id(pool, *task_id).await {
                Ok(Some(task)) => msg_lang.remind_target_task(&task.title),
                Ok(None) => msg_lang.remind_target_custom("Task (deleted)"),
                Err(e) => {
                    tracing::error!("Reminder #{reminder_id}: failed to fetch task {task_id}: {e}");
                    msg_lang.remind_target_custom("Task reminder")
                }
            }
        }
    };

    let chat_id = ChatId(user.telegram_user_id);
    match bot.send_message(chat_id, &message).await {
        Ok(_) => {
            tracing::info!("Reminder #{reminder_id} fired to user {user_id}");
            if let Err(e) = reminder_service::mark_fired(pool, reminder_id).await {
                tracing::error!("Reminder #{reminder_id}: failed to mark fired: {e}");
            }
        }
        Err(e) => {
            tracing::error!("Reminder #{reminder_id}: failed to send: {e}");
            if let Err(e2) = reminder_service::increment_attempts(pool, reminder_id).await {
                tracing::error!("Reminder #{reminder_id}: failed to increment attempts: {e2}");
            }
            if reminder.attempts + 1 >= MAX_RETRY_ATTEMPTS {
                tracing::warn!("Reminder #{reminder_id}: max retries reached, marking as failed");
                if let Err(e3) = crate::db::reminders::mark_failed(pool, reminder_id).await {
                    tracing::error!("Reminder #{reminder_id}: failed to mark failed: {e3}");
                }
            }
        }
    }
}
