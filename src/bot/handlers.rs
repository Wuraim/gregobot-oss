use std::str::FromStr;

use chrono::Datelike;
use teloxide::prelude::*;
use teloxide::sugar::request::RequestReplyExt;
use teloxide::types::{ChatAction, InputFile, ParseMode};

use crate::ai::context::{AiContext, AskInput, UserContext};
use crate::bot::command_history::{CommandHistory, cmd_reply, cmd_reply_markup};
use crate::bot::commands::Command;
use crate::db::{
    self, Db,
    users::{Role, User},
};
use crate::i18n::{Lang, Msg};
use crate::productivity::domain::{ReminderRecurrence, ReminderStatus, ReminderTarget};
use crate::productivity::domain::{TaskPriority, TaskSource, TaskStatus};
use crate::productivity::proposal_service;
use crate::productivity::reminder_service;
use crate::productivity::task_service;

async fn reply_text(bot: &Bot, msg: &Message, text: impl Into<String>) -> ResponseResult<Message> {
    bot.send_message(msg.chat.id, text).reply_to(msg.id).await
}

async fn reply_html(bot: &Bot, msg: &Message, html: impl Into<String>) -> ResponseResult<Message> {
    bot.send_message(msg.chat.id, html)
        .parse_mode(ParseMode::Html)
        .reply_to(msg.id)
        .await
}

pub async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    pool: Db,
    ai_ctx: AiContext,
) -> ResponseResult<()> {
    let from = match msg.from.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };

    tracing::info!(
        telegram_user_id = from.id.0,
        command = cmd.slug(),
        "bot command {}",
        cmd.slug()
    );

    crate::bot::track_group_participant(&pool, &msg).await;

    let msg_lang = Msg::new(Lang::from_iso_code(from.language_code.as_deref()));

    if let Command::Start = cmd {
        let telegram_user_id = from.id.0 as i64;
        if let Err(err) = db::users::upsert_pending(
            &pool,
            telegram_user_id,
            from.username.clone(),
            Some(from.first_name.clone()),
            from.language_code.clone(),
        )
        .await
        {
            tracing::error!("Failed to upsert pending user on /start: {err}");
        }
        let username_display = from
            .username
            .as_ref()
            .map(|u| format!(" (@{u})"))
            .unwrap_or_default();
        if let Ok(Some(user)) = db::users::find_by_telegram_id(&pool, telegram_user_id).await {
            let mut hist = CommandHistory::new(&pool, &user, &msg, speaker_label(from), &ai_ctx);
            cmd_reply_markup(
                &bot,
                &msg,
                &mut hist,
                msg_lang.start(telegram_user_id, &username_display),
                crate::bot::keyboard::main_menu(msg_lang.lang()),
            )
            .await?;
        } else {
            bot.send_message(
                msg.chat.id,
                msg_lang.start(telegram_user_id, &username_display),
            )
            .reply_markup(crate::bot::keyboard::main_menu(msg_lang.lang()))
            .await?;
        }
        return Ok(());
    }

    // Timezone / language can be set even while pending (prompted by /start).
    if matches!(cmd, Command::Timezone(_) | Command::Lang(_)) {
        let telegram_user_id = from.id.0 as i64;
        if let Err(err) = db::users::upsert_pending(
            &pool,
            telegram_user_id,
            from.username.clone(),
            Some(from.first_name.clone()),
            from.language_code.clone(),
        )
        .await
        {
            tracing::error!("Failed to sync profile before preference update: {err}");
        }
        let user = match db::users::find_by_telegram_id(&pool, telegram_user_id).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                bot.send_message(msg.chat.id, msg_lang.not_authorized())
                    .await?;
                return Ok(());
            }
            Err(err) => {
                tracing::error!("Failed to look up user for preferences: {err}");
                bot.send_message(msg.chat.id, msg_lang.internal_error())
                    .await?;
                return Ok(());
            }
        };
        let mut hist = CommandHistory::new(&pool, &user, &msg, speaker_label(from), &ai_ctx);
        // Prefer stored language once set; fall back to Telegram locale for this reply.
        let pref_lang = Msg::new(user.lang());
        match cmd {
            Command::Timezone(input) => {
                handle_timezone(&bot, &msg, &pool, &pref_lang, &user, &input, &mut hist).await?;
            }
            Command::Lang(input) => {
                handle_lang(&bot, &msg, &pool, &pref_lang, &user, &input, &mut hist).await?;
            }
            _ => unreachable!(),
        }
        return Ok(());
    }

    let user = match resolve_authorized_user(&bot, &msg, &pool, Some(&ai_ctx)).await {
        Some(u) => u,
        None => return Ok(()),
    };

    let user_lang = Msg::new(user.lang());
    let mut hist = CommandHistory::new(&pool, &user, &msg, speaker_label(from), &ai_ctx);

    match cmd {
        Command::UserAdd(arg) => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            let target = match resolve_user_arg(&pool, &arg).await {
                Ok(Some(u)) => u,
                Ok(None) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_not_found_start()).await?;
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("UserAdd lookup failed: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
                    return Ok(());
                }
            };

            if target.role != Role::Pending {
                cmd_reply(
                    &bot,
                    &msg,
                    &mut hist,
                    user_lang.user_add_already_registered(
                        target.telegram_user_id,
                        &target.role.to_string(),
                    ),
                )
                .await?;
                return Ok(());
            }

            match db::users::set_role(&pool, target.telegram_user_id, Role::Sportive).await {
                Ok(true) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.user_add_success(target.telegram_user_id, target.id),
                    )
                    .await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_add_failed_update()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to add user: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_add_failed()).await?;
                }
            }
        }
        Command::UserRemove(arg) => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            let target = match resolve_user_arg(&pool, &arg).await {
                Ok(Some(u)) => u,
                Ok(None) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_not_found()).await?;
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("UserRemove lookup failed: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
                    return Ok(());
                }
            };

            if target.telegram_user_id == user.telegram_user_id {
                cmd_reply(&bot, &msg, &mut hist, user_lang.cannot_remove_self()).await?;
                return Ok(());
            }

            if target.role == Role::Admin {
                let all_users = db::users::list(&pool).await.unwrap_or_default();
                let admin_count = all_users.iter().filter(|u| u.role == Role::Admin).count();
                if admin_count <= 1 {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.cannot_remove_last_admin()).await?;
                    return Ok(());
                }
            }

            match db::users::delete(&pool, target.telegram_user_id).await {
                Ok(true) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.user_remove_success(target.telegram_user_id, target.id),
                    )
                    .await?;
                }
                Ok(false) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.user_remove_not_found(target.telegram_user_id),
                    )
                    .await?;
                }
                Err(err) => {
                    tracing::error!("Failed to remove user: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_remove_failed()).await?;
                }
            }
        }
        Command::UserSetRole(argstr) => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            let args: Vec<_> = argstr.split_whitespace().collect();
            if args.len() != 2 {
                cmd_reply(&bot, &msg, &mut hist, user_lang.user_setrole_usage()).await?;
                return Ok(());
            }

            let target = match resolve_user_arg(&pool, args[0]).await {
                Ok(Some(u)) => u,
                Ok(None) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_not_found()).await?;
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("UserSetRole lookup failed: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_setrole_lookup_error()).await?;
                    return Ok(());
                }
            };

            let role = args[1].to_lowercase();
            let parsed_role = match Role::try_from(role.clone()) {
                Ok(r) => r,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.unknown_role()).await?;
                    return Ok(());
                }
            };

            // Prevent demoting the last admin
            if target.role == Role::Admin && parsed_role != Role::Admin {
                let all_users = db::users::list(&pool).await.unwrap_or_default();
                let admin_count = all_users.iter().filter(|u| u.role == Role::Admin).count();
                if admin_count <= 1 {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.cannot_demote_last_admin()).await?;
                    return Ok(());
                }
            }

            let ok = db::users::set_role(&pool, target.telegram_user_id, parsed_role.clone()).await;
            match ok {
                Ok(true) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.user_setrole_success(
                            target.telegram_user_id,
                            target.id,
                            &parsed_role.to_string(),
                        ),
                    )
                    .await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_setrole_not_updated()).await?;
                }
                Err(err) => {
                    tracing::error!("UserSetRole DB error: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_setrole_db_error()).await?;
                }
            }
        }
        Command::UserList => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            match db::users::list(&pool).await {
                Ok(users) => {
                    if users.is_empty() {
                        cmd_reply(&bot, &msg, &mut hist, user_lang.no_users_registered()).await?;
                    } else {
                        let mut msg_text = String::from(user_lang.registered_users_header());
                        for user in users {
                            let username_display = user.list_display_suffix();
                            msg_text.push_str(&user_lang.user_list_entry(
                                user.id,
                                user.telegram_user_id,
                                &user.role.to_string(),
                                &username_display,
                            ));
                        }
                        cmd_reply(&bot, &msg, &mut hist, msg_text).await?;
                    }
                }
                Err(err) => {
                    tracing::error!("List users failed: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_remove_failed()).await?;
                }
            }
        }
        Command::UserCost(arg) => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            let Some((user_arg, window)) = crate::db::ai_usage::parse_usercost_arg(&arg) else {
                cmd_reply(&bot, &msg, &mut hist, user_lang.user_cost_usage()).await?;
                return Ok(());
            };
            let target = match resolve_user_arg(&pool, &user_arg).await {
                Ok(Some(u)) => u,
                Ok(None) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_not_found()).await?;
                    return Ok(());
                }
                Err(err) => {
                    tracing::error!("UserCost lookup failed: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
                    return Ok(());
                }
            };

            let model_rows = crate::db::ai_usage::by_model_for_user(&pool, target.id, window).await;
            let kind_rows = crate::db::ai_usage::by_kind_for_user(&pool, target.id, window).await;
            match (model_rows, kind_rows) {
                (Ok(rows), Ok(kinds)) => {
                    let spend = crate::ai::pricing::summarize(&rows)
                        .with_kinds(crate::ai::pricing::summarize_kinds(&kinds));
                    let model_lines: Vec<String> = spend
                        .by_model
                        .iter()
                        .map(|m| {
                            user_lang.user_cost_model_line(
                                &m.model,
                                m.request_count,
                                m.prompt_tokens,
                                m.completion_tokens,
                                m.cached_tokens,
                                m.usd,
                            )
                        })
                        .collect();
                    let kind_lines: Vec<String> = spend
                        .by_kind
                        .iter()
                        .map(|k| user_lang.user_cost_kind_line(&k.kind, k.request_count, k.usd))
                        .collect();
                    let window_label = window.label();
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.user_cost(crate::i18n::UserCostReport {
                            telegram_user_id: target.telegram_user_id,
                            window: &window_label,
                            request_count: spend.request_count,
                            prompt_tokens: spend.prompt_tokens,
                            completion_tokens: spend.completion_tokens,
                            total_tokens: spend.total_tokens,
                            cached_tokens: spend.cached_tokens,
                            cache_usd_saved: spend.cache_usd_saved,
                            estimated_cost: spend.usd,
                            model_lines: &model_lines,
                            kind_lines: &kind_lines,
                        }),
                    )
                    .await?;
                }
                (Err(err), _) | (_, Err(err)) => {
                    tracing::error!("Failed to fetch AI usage: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_cost_error()).await?;
                }
            }
        }
        Command::TopUsage(arg) => {
            if user.role != Role::Admin {
                cmd_reply(&bot, &msg, &mut hist, user_lang.not_admin()).await?;
                return Ok(());
            }
            let mut limit = 10i64;
            let arg = arg.trim();
            if !arg.is_empty() {
                match arg.parse::<i64>() {
                    Ok(n) if (1..=25).contains(&n) => limit = n,
                    _ => {
                        cmd_reply(&bot, &msg, &mut hist, user_lang.top_usage_usage()).await?;
                        return Ok(());
                    }
                }
            }
            match crate::db::ai_usage::all_by_user_and_model(&pool).await {
                Ok(rows) if rows.is_empty() => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.top_usage_empty()).await?;
                }
                Ok(rows) => {
                    let ranked = crate::ai::pricing::rank_users(&rows, limit as usize);
                    let mut lines = vec![user_lang.top_usage_header(ranked.len())];
                    for (i, row) in ranked.iter().enumerate() {
                        let label = row
                            .username
                            .as_deref()
                            .map(|u| format!("@{u}"))
                            .unwrap_or_else(|| row.telegram_user_id.to_string());
                        lines.push(user_lang.top_usage_row(
                            i + 1,
                            &label,
                            row.spend.request_count,
                            row.spend.total_tokens,
                            row.spend.usd,
                        ));
                    }
                    cmd_reply(&bot, &msg, &mut hist, lines.join("\n")).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to fetch top AI usage: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.user_cost_error()).await?;
                }
            }
        }
        Command::Help(arg) => {
            // Repeat the minimal user lookup logic (cannot use the later variable due to early return)
            let from = match msg.from.as_ref() {
                Some(u) => u,
                None => return Ok(()),
            };
            let telegram_user_id = from.id.0 as i64;

            let user_res = db::users::find_by_telegram_id(&pool, telegram_user_id).await;
            match user_res {
                Ok(Some(user)) => {
                    let help_lang = Msg::new(user.lang());
                    let full = arg.trim() == "all";
                    let help_text = help_lang.help_commands_text(&user.role, full);
                    cmd_reply_markup(
                        &bot,
                        &msg,
                        &mut hist,
                        help_text,
                        crate::bot::keyboard::main_menu(user.lang()),
                    )
                    .await?;
                }
                Ok(None) => {
                    cmd_reply(&bot, &msg, &mut hist, msg_lang.unregistered_help()).await?;
                }
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, msg_lang.help_unavailable()).await?;
                }
            }
        }

        Command::Start => {
            // Already handled above, but we need to match it here
        }

        Command::Weight(input) => {
            let input = input.trim().to_string();
            match input.parse::<f64>() {
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.weight_invalid()).await?;
                }
                Ok(kg) => {
                    let entry = db::weight::insert(
                        &pool,
                        db::weight::NewWeightEntry {
                            user_id: user.id,
                            weight_kg: kg,
                            recorded_at: None,
                        },
                    )
                    .await;

                    match entry {
                        Ok(e) => {
                            cmd_reply(
                                &bot,
                                &msg,
                                &mut hist,
                                user_lang.weight_recorded(
                                    e.weight_kg,
                                    &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                                ),
                            )
                            .await?;
                            maybe_notify_achievements(
                                &bot,
                                msg.chat.id,
                                &pool,
                                user.id,
                                user.lang(),
                                &user_lang,
                                &ai_ctx,
                            )
                            .await;
                        }
                        Err(err) => {
                            tracing::error!("Failed to insert weight: {err}");
                            cmd_reply(&bot, &msg, &mut hist, user_lang.weight_save_error()).await?;
                        }
                    }
                }
            }
        }

        Command::LastWeight => match db::weight::latest_for_user(&pool, user.id).await {
            Err(err) => {
                tracing::error!("Failed to fetch weight: {err}");
                cmd_reply(&bot, &msg, &mut hist, user_lang.weight_fetch_error()).await?;
            }
            Ok(None) => {
                cmd_reply(&bot, &msg, &mut hist, user_lang.weight_no_entries()).await?;
            }
            Ok(Some(e)) => {
                cmd_reply(
                    &bot,
                    &msg,
                    &mut hist,
                    user_lang
                        .weight_last(e.weight_kg, &e.recorded_at.format("%Y-%m-%d").to_string()),
                )
                .await?;
            }
        },

        Command::WeightChart(arg) => {
            handle_weight_chart(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }

        Command::Nutrition(arg) => {
            handle_nutrition(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }

        Command::Training(description) => {
            let description = description.trim().to_string();
            if description.is_empty() {
                cmd_reply(&bot, &msg, &mut hist, user_lang.training_empty()).await?;
            } else {
                match db::training::try_insert(
                    &pool,
                    db::training::NewTrainingEntry::simple(user.id, description.clone(), None),
                )
                .await
                {
                    Ok(db::training::TrainingInsert::Created(e)) => {
                        cmd_reply(
                            &bot,
                            &msg,
                            &mut hist,
                            user_lang.training_logged(
                                &e.description,
                                &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            ),
                        )
                        .await?;
                        maybe_notify_achievements(
                            &bot,
                            msg.chat.id,
                            &pool,
                            user.id,
                            user.lang(),
                            &user_lang,
                            &ai_ctx,
                        )
                        .await;
                        spawn_enrich_training(
                            pool.clone(),
                            ai_ctx.clone(),
                            user.id,
                            user.lang(),
                            e.id,
                            description,
                        );
                    }
                    Ok(db::training::TrainingInsert::Duplicate(e)) => {
                        cmd_reply(
                            &bot,
                            &msg,
                            &mut hist,
                            user_lang.training_already_logged(
                                e.id,
                                &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            ),
                        )
                        .await?;
                    }
                    Err(err) => {
                        tracing::error!("Failed to insert training: {err}");
                        cmd_reply(&bot, &msg, &mut hist, user_lang.training_save_error()).await?;
                    }
                }
            }
        }

        Command::ShowTraining(arg) => {
            handle_show_training(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }

        Command::LastTraining(arg) => {
            let limit = parse_last_training_limit(&arg);
            match db::training::list_for_user(&pool, user.id).await {
                Err(err) => {
                    tracing::error!("Failed to fetch training: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.training_fetch_error()).await?;
                }
                Ok(entries) if entries.is_empty() => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.training_no_entries()).await?;
                }
                Ok(entries) => {
                    let lines: Vec<String> = entries
                        .into_iter()
                        .take(limit)
                        .map(|e| {
                            user_lang.training_session_entry(
                                e.id,
                                &e.recorded_at.format("%Y-%m-%d").to_string(),
                                &db::training::summary_line(&e),
                            )
                        })
                        .collect();
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        format!(
                            "{}{}",
                            user_lang.training_sessions_header(),
                            lines.join("\n")
                        ),
                    )
                    .await?;
                }
            }
        }

        Command::UpdateTraining(arg) => {
            handle_update_training(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }

        Command::DelTraining(arg) => {
            handle_del_training(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }

        Command::Ask(prompt) => {
            let prompt = prompt.trim().to_string();
            if prompt.is_empty() {
                reply_text(&bot, &msg, user_lang.ask_description()).await?;
            } else {
                if ai_ctx.api_key.is_none() {
                    reply_text(&bot, &msg, user_lang.ai_no_api_key()).await?;
                    return Ok(());
                }

                // Show typing indicator
                bot.send_chat_action(msg.chat.id, ChatAction::Typing)
                    .await?;

                let group_context = if msg.chat.is_private() {
                    None
                } else {
                    match crate::db::group_members::context_for_chat(&pool, msg.chat.id.0).await {
                        Ok(ctx) => Some(ctx),
                        Err(err) => {
                            tracing::warn!(
                                chat_id = %msg.chat.id.0,
                                "Failed to build group context: {err}"
                            );
                            None
                        }
                    }
                };

                let speaker = speaker_label(from);
                let user_ctx = UserContext {
                    user_id: user.id,
                    telegram_user_id: user.telegram_user_id,
                    lang: user.lang(),
                    web_search_enabled: user.web_search_enabled,
                    is_admin: user.role == Role::Admin,
                };
                let ask_input = AskInput {
                    prompt: &prompt,
                    speaker: &speaker,
                    group_context: group_context.as_ref(),
                    image: None,
                    bot: Some(&bot),
                    chat_id: Some(msg.chat.id.0),
                    reply_to_message_id: Some(msg.id),
                };
                match crate::ai::ask(&pool, &ai_ctx, &user_ctx, &ask_input).await {
                    Ok(response) => {
                        let html = crate::bot::markdown::md_to_html(&response);
                        if html.trim().is_empty() {
                            reply_text(&bot, &msg, user_lang.ai_empty_response()).await?;
                        } else {
                            reply_html(&bot, &msg, html).await?;
                        }
                    }
                    Err(err) => {
                        tracing::error!(user_id = %user.id, "AI ask failed: {err}");
                        reply_text(&bot, &msg, user_lang.ai_error()).await?;
                    }
                }
            }
        }

        Command::SetGoal(argstr) => {
            let args: Vec<_> = argstr.split_whitespace().collect();
            if args.len() != 2 {
                cmd_reply(&bot, &msg, &mut hist, user_lang.goal_set_usage()).await?;
                return Ok(());
            }
            let goal_type = match db::goals::GoalType::from_str(&args[0].to_lowercase()) {
                Some(t) => t,
                None => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.goal_set_invalid_type()).await?;
                    return Ok(());
                }
            };
            let target_value = match args[1].parse::<f64>() {
                Ok(v) => v,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.goal_set_invalid_value()).await?;
                    return Ok(());
                }
            };
            let new_goal = db::goals::NewGoal {
                user_id: user.id,
                goal_type,
                target_value,
            };
            match db::goals::set_goal(&pool, new_goal).await {
                Ok(_) => {
                    let text = match goal_type {
                        db::goals::GoalType::Weight => {
                            user_lang.goal_set_success_weight(target_value)
                        }
                        db::goals::GoalType::TrainingFrequency => {
                            user_lang.goal_set_success_training(target_value)
                        }
                    };
                    cmd_reply(&bot, &msg, &mut hist, text).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to set goal: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
                }
            }
        }

        Command::Goals => match db::goals::list_active(&pool, user.id).await {
            Err(err) => {
                tracing::error!("Failed to list goals: {err}");
                cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
            }
            Ok(goals) if goals.is_empty() => {
                cmd_reply(&bot, &msg, &mut hist, user_lang.goal_no_goals()).await?;
            }
            Ok(goals) => {
                let mut text = String::from(user_lang.goal_progress_header());
                for g in &goals {
                    let description = match db::goals::GoalType::from_str(&g.goal_type) {
                        Some(db::goals::GoalType::Weight) => {
                            let latest = db::weight::latest_for_user(&pool, user.id)
                                .await
                                .ok()
                                .flatten();
                            let current = latest.as_ref().map(|e| e.weight_kg);
                            user_lang.goal_progress_weight(current, g.target_value)
                        }
                        Some(db::goals::GoalType::TrainingFrequency) => {
                            let count = db::goals::count_training_this_week(&pool, user.id)
                                .await
                                .unwrap_or(0.0);
                            user_lang.goal_progress_training(count, g.target_value)
                        }
                        None => format!("? ({})", g.goal_type),
                    };
                    text.push_str(&user_lang.goal_progress_entry(g.id, &description));
                }
                cmd_reply_markup(
                    &bot,
                    &msg,
                    &mut hist,
                    text,
                    crate::bot::keyboard::goals_menu(user.lang()),
                )
                .await?;
            }
        },

        Command::DelGoal(argstr) => {
            let arg = argstr.trim();
            let goal_id = match arg.parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.goal_delete_usage()).await?;
                    return Ok(());
                }
            };
            match db::goals::deactivate(&pool, goal_id, user.id).await {
                Ok(true) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.goal_delete_success()).await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.goal_delete_not_found()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to delete goal: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.db_error()).await?;
                }
            }
        }

        Command::Task(input) => {
            let title = input.trim().to_string();
            if title.is_empty() {
                cmd_reply(&bot, &msg, &mut hist, user_lang.task_usage()).await?;
                return Ok(());
            }
            match task_service::create_task(
                &pool,
                task_service::CreateTaskInput {
                    user_id: user.id,
                    title,
                    description: None,
                    priority: TaskPriority::Medium,
                    due_at: None,
                    tags: vec![],
                    source: TaskSource::Manual,
                },
            )
            .await
            {
                Ok(task) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.task_created(task.id, &task.title),
                    )
                    .await?;
                }
                Err(err) => {
                    tracing::error!("Failed to create task: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_create_error()).await?;
                }
            }
        }

        Command::Tasks => match task_service::list_tasks(&pool, user.id, None).await {
            Ok(tasks) if tasks.is_empty() => {
                cmd_reply(&bot, &msg, &mut hist, user_lang.task_no_tasks()).await?;
            }
            Ok(tasks) => {
                let mut text = user_lang.task_list_header();
                for t in &tasks {
                    text.push_str(&user_lang.task_list_entry(
                        t.id,
                        &t.title,
                        t.status.as_str(),
                        t.priority.as_str(),
                    ));
                }
                cmd_reply(&bot, &msg, &mut hist, text).await?;
            }
            Err(err) => {
                tracing::error!("Failed to list tasks: {err}");
                cmd_reply(&bot, &msg, &mut hist, user_lang.task_list_error()).await?;
            }
        },

        Command::DoneTask(arg) => {
            let task_id = match arg.trim().parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_done_usage()).await?;
                    return Ok(());
                }
            };
            match task_service::update_status(&pool, task_id, user.id, TaskStatus::Done).await {
                Ok(task) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.task_marked_done(&task.title),
                    )
                    .await?;
                }
                Err(err) => {
                    tracing::error!("Failed to mark task done: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_update_error()).await?;
                }
            }
        }

        Command::CancelTask(arg) => {
            let task_id = match arg.trim().parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_cancel_usage()).await?;
                    return Ok(());
                }
            };
            match task_service::update_status(&pool, task_id, user.id, TaskStatus::Cancelled).await
            {
                Ok(task) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_cancelled(&task.title)).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to cancel task: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_update_error()).await?;
                }
            }
        }

        Command::Deltask(arg) => {
            let task_id = match arg.trim().parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_delete_usage()).await?;
                    return Ok(());
                }
            };
            match task_service::delete_task(&pool, task_id, user.id).await {
                Ok(true) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_deleted(task_id)).await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_not_found()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to delete task: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.task_delete_error()).await?;
                }
            }
        }

        Command::Remind(input) => {
            let input = input.trim();
            if input.is_empty() {
                cmd_reply(&bot, &msg, &mut hist, user_lang.remind_usage()).await?;
                return Ok(());
            }

            match parse_reminder_input(input) {
                Some(parsed) => {
                    match reminder_service::create_reminder(
                        &pool,
                        user.id,
                        ReminderTarget::Custom {
                            message: parsed.message.clone(),
                        },
                        parsed.scheduled_at,
                        parsed.recurrence,
                    )
                    .await
                    {
                        Ok(reminder) => {
                            cmd_reply(
                                &bot,
                                &msg,
                                &mut hist,
                                user_lang.remind_created(
                                    &reminder
                                        .scheduled_at
                                        .format("%Y-%m-%d %H:%M UTC")
                                        .to_string(),
                                    &parsed.message,
                                    reminder.recurrence.as_str(),
                                ),
                            )
                            .await?;
                        }
                        Err(err) => {
                            tracing::error!("Failed to create reminder: {err}");
                            cmd_reply(&bot, &msg, &mut hist, user_lang.remind_create_error())
                                .await?;
                        }
                    }
                }
                None => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_usage()).await?;
                }
            }
        }

        Command::Reminders => {
            match reminder_service::list_reminders(&pool, user.id, Some(ReminderStatus::Pending))
                .await
            {
                Ok(reminders) if reminders.is_empty() => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_no_reminders()).await?;
                }
                Ok(reminders) => {
                    let mut text = user_lang.remind_list_header();
                    for r in &reminders {
                        text.push_str(&user_lang.remind_list_entry(
                            r.id,
                            &r.scheduled_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            r.recurrence.as_str(),
                        ));
                    }
                    cmd_reply(&bot, &msg, &mut hist, text).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to list reminders: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_list_error()).await?;
                }
            }
        }

        Command::CancelRemind(input) => {
            let reminder_id: i64 = match input.trim().parse() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_cancel_usage()).await?;
                    return Ok(());
                }
            };
            match reminder_service::cancel_reminder(&pool, reminder_id, user.id).await {
                Ok(true) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.remind_cancelled(reminder_id),
                    )
                    .await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_not_found()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to cancel reminder: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.remind_cancel_error()).await?;
                }
            }
        }

        Command::SetProfile(args) => {
            let desc = args.trim().to_string();
            if desc.is_empty() {
                cmd_reply(&bot, &msg, &mut hist, user_lang.profile_set_usage()).await?;
                return Ok(());
            }
            match db::users::update_description(&pool, user.telegram_user_id, Some(desc)).await {
                Ok(true) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.profile_updated()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to update profile: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.profile_update_error()).await?;
                }
                _ => {}
            }
        }

        Command::Profile => match user.description.as_deref() {
            Some(desc) if !desc.is_empty() => {
                cmd_reply(&bot, &msg, &mut hist, user_lang.profile_view(desc)).await?;
            }
            _ => {
                cmd_reply(&bot, &msg, &mut hist, user_lang.profile_empty()).await?;
            }
        },

        Command::Timezone(_input) => {
            // Handled earlier so pending users can set preferences after /start.
        }
        Command::Lang(_input) => {
            // Handled earlier so pending users can set preferences after /start.
        }

        Command::Propose(input) => {
            let title = input.trim().to_string();
            if title.is_empty() {
                cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_usage()).await?;
                return Ok(());
            }
            match proposal_service::create_proposal(&pool, user.id, title, None).await {
                Ok(proposal) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.proposal_created(proposal.id, &proposal.title),
                    )
                    .await?;
                }
                Err(err) => {
                    tracing::error!("Failed to create proposal: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_create_error()).await?;
                }
            }
        }

        Command::Proposals => {
            let is_admin = user.role == Role::Admin;
            match proposal_service::list_proposals(&pool, user.id, is_admin).await {
                Ok(proposals) if proposals.is_empty() => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_list_empty()).await?;
                }
                Ok(proposals) => {
                    let mut text = if is_admin {
                        user_lang.proposal_list_header_all()
                    } else {
                        user_lang.proposal_list_header_mine()
                    };
                    for p in &proposals {
                        let author = p
                            .author_username
                            .as_deref()
                            .map(|u| format!(" @{}", u))
                            .unwrap_or_default();
                        text.push_str(&user_lang.proposal_list_entry(p.id, &p.title, &author));
                    }
                    cmd_reply(&bot, &msg, &mut hist, text).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to list proposals: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_list_error()).await?;
                }
            }
        }

        Command::DelProposal(arg) => {
            let proposal_id = match arg.trim().parse::<i64>() {
                Ok(id) => id,
                Err(_) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_delete_usage()).await?;
                    return Ok(());
                }
            };
            let is_admin = user.role == Role::Admin;
            match proposal_service::delete_proposal(&pool, proposal_id, user.id, is_admin).await {
                Ok(true) => {
                    cmd_reply(
                        &bot,
                        &msg,
                        &mut hist,
                        user_lang.proposal_deleted(proposal_id),
                    )
                    .await?;
                }
                Ok(false) => {
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_not_found()).await?;
                }
                Err(err) => {
                    tracing::error!("Failed to delete proposal: {err}");
                    cmd_reply(&bot, &msg, &mut hist, user_lang.proposal_delete_error()).await?;
                }
            }
        }

        Command::DailyRecap(arg) => {
            handle_daily_recap(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }
        Command::Nudges(arg) => {
            handle_nudges(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }
        Command::Streak => {
            handle_streak(&bot, &msg, &pool, &user_lang, user.id, &mut hist).await?;
        }
        Command::SetSportEmoji(arg) => {
            handle_set_sport_emoji(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }
        Command::MonthlyGraph(arg) => {
            handle_monthly_graph(&bot, &msg, &pool, &user_lang, &user, &arg, &mut hist).await?;
        }
        Command::EnableWebSearch(arg) => {
            set_web_search_handler(&bot, &msg, &pool, &user_lang, &arg, true, &mut hist).await?;
        }
        Command::DisableWebSearch(arg) => {
            set_web_search_handler(&bot, &msg, &pool, &user_lang, &arg, false, &mut hist).await?;
        }
        Command::Rag => {
            handle_rag_status(&bot, &msg, &pool, &user, &ai_ctx, &mut hist).await?;
        }
        Command::RagRebuild => {
            handle_rag_rebuild(&bot, &msg, &pool, &user, &ai_ctx, &mut hist).await?;
        }
    }

    Ok(())
}

async fn handle_daily_recap(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let arg = arg.trim();
    if arg.is_empty() || arg == "status" {
        let status = user_lang.daily_recap_status(
            user.daily_recap_enabled,
            user.daily_recap_hour,
            user.daily_recap_minute,
        );
        cmd_reply(&bot, &msg, hist, status).await?;
        return Ok(());
    }

    if arg == "on" {
        match crate::db::daily_recap::set_daily_recap_enabled(pool, user.telegram_user_id, true)
            .await
        {
            Ok(true) => {
                let msg_text =
                    user_lang.daily_recap_enabled(user.daily_recap_hour, user.daily_recap_minute);
                cmd_reply(&bot, &msg, hist, msg_text).await?;
            }
            _ => {
                cmd_reply(&bot, &msg, hist, user_lang.daily_recap_error()).await?;
            }
        }
        return Ok(());
    }

    if arg == "off" {
        match crate::db::daily_recap::set_daily_recap_enabled(pool, user.telegram_user_id, false)
            .await
        {
            Ok(true) => {
                cmd_reply(&bot, &msg, hist, user_lang.daily_recap_disabled()).await?;
            }
            _ => {
                cmd_reply(&bot, &msg, hist, user_lang.daily_recap_error()).await?;
            }
        }
        return Ok(());
    }

    if let Some(time_str) = arg.strip_prefix("at ") {
        let time_str = time_str.trim();
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: i64 = parts[0].parse().unwrap_or(-1);
            let minute: i64 = parts[1].parse().unwrap_or(-1);
            if (0..=23).contains(&hour) && (0..=59).contains(&minute) {
                match crate::db::daily_recap::set_daily_recap(
                    pool,
                    user.telegram_user_id,
                    true,
                    hour,
                    minute,
                )
                .await
                {
                    Ok(true) => {
                        cmd_reply(
                            &bot,
                            &msg,
                            hist,
                            user_lang.daily_recap_time_set(hour, minute),
                        )
                        .await?;
                    }
                    _ => {
                        cmd_reply(&bot, &msg, hist, user_lang.daily_recap_error()).await?;
                    }
                }
                return Ok(());
            }
        }
    }

    cmd_reply(&bot, &msg, hist, user_lang.daily_recap_invalid_time()).await?;
    Ok(())
}

async fn set_web_search_handler(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    arg: &str,
    enabled: bool,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let target = match resolve_user_arg(pool, arg).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            cmd_reply(&bot, &msg, hist, user_lang.user_not_found()).await?;
            return Ok(());
        }
        Err(err) => {
            tracing::error!("Web search lookup failed: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.db_error()).await?;
            return Ok(());
        }
    };

    match db::users::set_web_search(pool, target.telegram_user_id, enabled).await {
        Ok(true) => {
            cmd_reply(
                &bot,
                &msg,
                hist,
                user_lang.web_search_set(
                    target.telegram_user_id,
                    if enabled { "enabled" } else { "disabled" },
                ),
            )
            .await?;
        }
        Ok(false) => {
            cmd_reply(&bot, &msg, hist, user_lang.user_not_found()).await?;
        }
        Err(err) => {
            tracing::error!("Failed to set web search: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.web_search_error()).await?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct ParsedReminderInput {
    recurrence: ReminderRecurrence,
    scheduled_at: chrono::DateTime<chrono::Utc>,
    message: String,
}

/// Parse a /remind input string.
/// Supports recurring syntax (daily/weekly/monthly and "every ...") or falls back to one-time 1h from now.
fn parse_reminder_input(input: &str) -> Option<ParsedReminderInput> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let first = tokens[0].to_lowercase();

    if first == "daily" {
        return parse_daily(&tokens);
    }
    if first == "weekly" {
        return parse_weekly(&tokens);
    }
    if first == "monthly" {
        return parse_monthly(&tokens);
    }
    if first == "every" && tokens.len() >= 2 {
        return parse_every(&tokens);
    }

    // One-time reminder: schedule 1 hour from now with the whole input as message.
    Some(ParsedReminderInput {
        recurrence: ReminderRecurrence::None,
        scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
        message: input.to_string(),
    })
}

fn parse_daily(tokens: &[&str]) -> Option<ParsedReminderInput> {
    // daily <time> <message>
    if tokens.len() < 3 {
        return None;
    }
    let (hour, minute) = parse_time_of_day(tokens[1])?;
    let message = tokens[2..].join(" ");
    let scheduled_at = datetime_for_daily(hour, minute);
    Some(ParsedReminderInput {
        recurrence: ReminderRecurrence::Daily,
        scheduled_at,
        message,
    })
}

fn parse_weekly(tokens: &[&str]) -> Option<ParsedReminderInput> {
    // weekly <weekday> <time> <message>
    if tokens.len() < 4 {
        return None;
    }
    let weekday = parse_weekday(tokens[1])?;
    let (hour, minute) = parse_time_of_day(tokens[2])?;
    let message = tokens[3..].join(" ");
    let scheduled_at = datetime_for_weekly(weekday, hour, minute);
    Some(ParsedReminderInput {
        recurrence: ReminderRecurrence::Weekly,
        scheduled_at,
        message,
    })
}

fn parse_monthly(tokens: &[&str]) -> Option<ParsedReminderInput> {
    // monthly <day> <time> <message>
    if tokens.len() < 4 {
        return None;
    }
    let day = parse_month_day(tokens[1])?;
    let (hour, minute) = parse_time_of_day(tokens[2])?;
    let message = tokens[3..].join(" ");
    let scheduled_at = datetime_for_monthly(day, hour, minute);
    Some(ParsedReminderInput {
        recurrence: ReminderRecurrence::Monthly,
        scheduled_at,
        message,
    })
}

fn parse_every(tokens: &[&str]) -> Option<ParsedReminderInput> {
    // every day [at] <time> <message>
    // every <weekday> [at] <time> <message>
    // every month [on [the]] <day> [at] <time> <message>
    let second = tokens[1].to_lowercase();

    if second == "day" {
        let mut idx = 2;
        if idx < tokens.len() && tokens[idx].eq_ignore_ascii_case("at") {
            idx += 1;
        }
        if idx + 1 >= tokens.len() {
            return None;
        }
        let (hour, minute) = parse_time_of_day(tokens[idx])?;
        let message = tokens[idx + 1..].join(" ");
        let scheduled_at = datetime_for_daily(hour, minute);
        return Some(ParsedReminderInput {
            recurrence: ReminderRecurrence::Daily,
            scheduled_at,
            message,
        });
    }

    if let Some(weekday) = parse_weekday(&second) {
        let mut idx = 2;
        if idx < tokens.len() && tokens[idx].eq_ignore_ascii_case("at") {
            idx += 1;
        }
        if idx + 1 >= tokens.len() {
            return None;
        }
        let (hour, minute) = parse_time_of_day(tokens[idx])?;
        let message = tokens[idx + 1..].join(" ");
        let scheduled_at = datetime_for_weekly(weekday, hour, minute);
        return Some(ParsedReminderInput {
            recurrence: ReminderRecurrence::Weekly,
            scheduled_at,
            message,
        });
    }

    if second == "month" {
        let mut idx = 2;
        if idx < tokens.len() && tokens[idx].eq_ignore_ascii_case("on") {
            idx += 1;
        }
        if idx < tokens.len() && tokens[idx].eq_ignore_ascii_case("the") {
            idx += 1;
        }
        if idx + 2 >= tokens.len() {
            return None;
        }
        let day = parse_month_day(tokens[idx])?;
        idx += 1;
        if tokens[idx].eq_ignore_ascii_case("at") {
            idx += 1;
        }
        if idx + 1 >= tokens.len() {
            return None;
        }
        let (hour, minute) = parse_time_of_day(tokens[idx])?;
        let message = tokens[idx + 1..].join(" ");
        let scheduled_at = datetime_for_monthly(day, hour, minute);
        return Some(ParsedReminderInput {
            recurrence: ReminderRecurrence::Monthly,
            scheduled_at,
            message,
        });
    }

    None
}

fn parse_time_of_day(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().to_lowercase();
    let is_pm = s.ends_with("pm");
    let is_am = s.ends_with("am");
    let s = s
        .trim_end_matches("pm")
        .trim_end_matches("am")
        .trim_end_matches("h")
        .trim();

    let parts: Vec<&str> = s.split([':', 'h', '.']).collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = if parts.len() == 2 {
        parts[1].parse().ok()?
    } else {
        0
    };

    let mut hour = hour;
    if is_pm && hour != 12 {
        hour += 12;
    }
    if is_am && hour == 12 {
        hour = 0;
    }

    if hour >= 24 || minute >= 60 {
        return None;
    }
    Some((hour, minute))
}

fn parse_weekday(s: &str) -> Option<chrono::Weekday> {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Some(chrono::Weekday::Mon),
        "tuesday" | "tue" | "tues" => Some(chrono::Weekday::Tue),
        "wednesday" | "wed" => Some(chrono::Weekday::Wed),
        "thursday" | "thu" | "thurs" => Some(chrono::Weekday::Thu),
        "friday" | "fri" => Some(chrono::Weekday::Fri),
        "saturday" | "sat" => Some(chrono::Weekday::Sat),
        "sunday" | "sun" => Some(chrono::Weekday::Sun),
        _ => None,
    }
}

fn parse_month_day(s: &str) -> Option<u32> {
    let s = s.to_lowercase();
    let s = s
        .trim_end_matches("st")
        .trim_end_matches("nd")
        .trim_end_matches("rd")
        .trim_end_matches("th");
    let day: u32 = s.parse().ok()?;
    if (1..=31).contains(&day) {
        Some(day)
    } else {
        None
    }
}

fn datetime_for_daily(hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    let today = now.date_naive();
    let mut dt = today
        .and_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| today.and_hms_opt(0, 0, 0).unwrap())
        .and_utc();
    if dt <= now {
        dt += chrono::Duration::days(1);
    }
    dt
}

fn datetime_for_weekly(
    weekday: chrono::Weekday,
    hour: u32,
    minute: u32,
) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    let today = now.date_naive();
    let days_ahead =
        (weekday.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64 + 7)
            % 7;
    let target_day = today + chrono::Duration::days(days_ahead);
    let mut dt = target_day
        .and_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| target_day.and_hms_opt(0, 0, 0).unwrap())
        .and_utc();
    if dt <= now {
        dt += chrono::Duration::weeks(1);
    }
    dt
}

fn datetime_for_monthly(day: u32, hour: u32, minute: u32) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now();
    let today = now.date_naive();
    let day = day.min(today.num_days_in_month() as u32);

    let mut target_day = today.with_day(day).unwrap_or(today);
    let mut dt = target_day
        .and_hms_opt(hour, minute, 0)
        .unwrap_or_else(|| target_day.and_hms_opt(0, 0, 0).unwrap())
        .and_utc();

    if dt <= now {
        let next_month = if today.month() == 12 {
            today
                .with_year(today.year() + 1)
                .unwrap()
                .with_month(1)
                .unwrap()
        } else {
            today.with_month(today.month() + 1).unwrap()
        };
        let clamped_day = day.min(next_month.num_days_in_month() as u32);
        target_day = next_month.with_day(clamped_day).unwrap_or(next_month);
        dt = target_day
            .and_hms_opt(hour, minute, 0)
            .unwrap_or_else(|| target_day.and_hms_opt(0, 0, 0).unwrap())
            .and_utc();
    }
    dt
}

async fn resolve_authorized_user(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    ai_ctx: Option<&AiContext>,
) -> Option<User> {
    let from = msg.from.as_ref()?;
    let telegram_user_id = from.id.0 as i64;
    let msg_lang = Msg::new(Lang::from_iso_code(from.language_code.as_deref()));

    tracing::debug!(
        chat_id = %msg.chat.id.0,
        telegram_user_id = %telegram_user_id,
        username = ?from.username,
        "Resolving authorized user"
    );

    // Always refresh username / first_name / language from the Telegram payload
    // when we see the user — including pending users who only interact in groups.
    if let Err(err) = db::users::upsert_pending(
        pool,
        telegram_user_id,
        from.username.clone(),
        Some(from.first_name.clone()),
        from.language_code.clone(),
    )
    .await
    {
        tracing::error!("Failed to sync profile for user {telegram_user_id}: {err}");
    }

    let user = match db::users::find_by_telegram_id(pool, telegram_user_id).await {
        Ok(Some(u)) if u.role != Role::Pending => u,
        Ok(Some(pending)) => {
            tracing::debug!(
                chat_id = %msg.chat.id.0,
                telegram_user_id = %telegram_user_id,
                "User is pending, rejecting"
            );
            let denial = msg_lang.not_authorized_pending();
            if let Some(ai_ctx) = ai_ctx {
                let mut hist =
                    CommandHistory::new(pool, &pending, msg, speaker_label(from), ai_ctx);
                cmd_reply(bot, msg, &mut hist, denial).await.ok();
            } else {
                bot.send_message(msg.chat.id, denial).await.ok();
            }
            return None;
        }
        Ok(None) => {
            tracing::debug!(
                chat_id = %msg.chat.id.0,
                telegram_user_id = %telegram_user_id,
                "User not found, rejecting"
            );
            bot.send_message(msg.chat.id, msg_lang.not_authorized())
                .await
                .ok();
            return None;
        }
        Err(err) => {
            tracing::error!("Failed to look up user {telegram_user_id}: {err}");
            bot.send_message(msg.chat.id, msg_lang.internal_error())
                .await
                .ok();
            return None;
        }
    };

    tracing::debug!(
        chat_id = %msg.chat.id.0,
        user_id = %user.id,
        role = ?user.role,
        "User authorized"
    );

    Some(user)
}

fn strip_mention(input: &str, bot_username: &str) -> String {
    let lower = input.to_lowercase();
    let mention = format!("@{}", bot_username.to_lowercase());
    if let Some(pos) = lower.find(&mention) {
        let end = pos + mention.len();
        format!("{}{}", &input[..pos], &input[end..])
            .trim()
            .to_string()
    } else {
        input.trim().to_string()
    }
}

/// Prefer `@username`, then Telegram first name, then numeric id.
fn speaker_label(from: &teloxide::types::User) -> String {
    if let Some(username) = from
        .username
        .as_ref()
        .map(|u| u.trim())
        .filter(|u| !u.is_empty())
    {
        format!("@{username}")
    } else if !from.first_name.trim().is_empty() {
        from.first_name.clone()
    } else {
        format!("User {}", from.id.0)
    }
}

pub async fn handle_ai_message(
    bot: Bot,
    msg: Message,
    pool: Db,
    ai_ctx: AiContext,
    bot_username: String,
) -> ResponseResult<()> {
    let raw_text = msg.text().unwrap_or("");
    let caption = msg.caption().unwrap_or("");

    tracing::debug!(
        chat_id = %msg.chat.id.0,
        is_private = %msg.chat.is_private(),
        text = %raw_text,
        caption = %caption,
        bot_username = %bot_username,
        "handle_ai_message invoked"
    );

    crate::bot::track_group_participant(&pool, &msg).await;

    let user = match resolve_authorized_user(&bot, &msg, &pool, None).await {
        Some(u) => u,
        None => {
            tracing::debug!(
                chat_id = %msg.chat.id.0,
                "handle_ai_message: user not authorized, exiting"
            );
            return Ok(());
        }
    };

    let prompt = if msg.chat.is_private() || bot_username.is_empty() {
        raw_text.to_string()
    } else if !raw_text.is_empty() {
        strip_mention(raw_text, &bot_username)
    } else if !caption.is_empty() {
        strip_mention(caption, &bot_username)
    } else {
        String::new()
    };

    tracing::debug!(
        chat_id = %msg.chat.id.0,
        user_id = %user.id,
        prompt = %prompt,
        "handle_ai_message extracted prompt"
    );

    let user_lang = Msg::new(user.lang());

    if prompt.is_empty() {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            user_id = %user.id,
            "handle_ai_message: prompt is empty, exiting"
        );
        reply_text(&bot, &msg, user_lang.ai_empty_response()).await?;
        return Ok(());
    }

    if ai_ctx.api_key.is_none() {
        tracing::debug!(
            chat_id = %msg.chat.id.0,
            user_id = %user.id,
            "handle_ai_message: no API key configured, exiting"
        );
        reply_text(&bot, &msg, user_lang.ai_no_api_key()).await?;
        return Ok(());
    }

    bot.send_chat_action(msg.chat.id, ChatAction::Typing)
        .await?;

    let group_context = if msg.chat.is_private() {
        None
    } else {
        match crate::db::group_members::context_for_chat(&pool, msg.chat.id.0).await {
            Ok(ctx) => Some(ctx),
            Err(err) => {
                tracing::warn!(
                    chat_id = %msg.chat.id.0,
                    "Failed to build group context: {err}"
                );
                None
            }
        }
    };

    let speaker = msg
        .from
        .as_ref()
        .map(speaker_label)
        .unwrap_or_else(|| format!("User {}", user.telegram_user_id));
    let user_ctx = UserContext {
        user_id: user.id,
        telegram_user_id: user.telegram_user_id,
        lang: user.lang(),
        web_search_enabled: user.web_search_enabled,
        is_admin: user.role == Role::Admin,
    };
    let ask_input = AskInput {
        prompt: &prompt,
        speaker: &speaker,
        group_context: group_context.as_ref(),
        image: None,
        bot: Some(&bot),
        chat_id: Some(msg.chat.id.0),
        reply_to_message_id: Some(msg.id),
    };
    match crate::ai::ask(&pool, &ai_ctx, &user_ctx, &ask_input).await {
        Ok(response) => {
            let html = crate::bot::markdown::md_to_html(&response);
            if html.trim().is_empty() {
                reply_text(&bot, &msg, user_lang.ai_empty_response()).await?;
            } else {
                reply_html(&bot, &msg, html).await?;
            }
        }
        Err(err) => {
            tracing::error!(user_id = %user.id, "AI ask failed: {err}");
            reply_text(&bot, &msg, user_lang.ai_error()).await?;
        }
    }

    Ok(())
}

/// Handle a photo message that should go through the chat LLM (private always,
/// or group when mentioned / reply-to-bot). Caller is responsible for sport
/// tracking when applicable.
pub async fn handle_ai_photo(
    bot: Bot,
    msg: Message,
    pool: Db,
    ai_ctx: AiContext,
    bot_username: String,
    jpeg: Option<Vec<u8>>,
) -> ResponseResult<()> {
    crate::bot::track_group_participant(&pool, &msg).await;

    let user = match resolve_authorized_user(&bot, &msg, &pool, None).await {
        Some(u) => u,
        None => return Ok(()),
    };

    let caption = msg.caption().unwrap_or("");
    let prompt = if msg.chat.is_private() || bot_username.is_empty() {
        caption.trim().to_string()
    } else {
        strip_mention(caption, &bot_username)
    };

    let user_lang = Msg::new(user.lang());

    if ai_ctx.api_key.is_none() {
        reply_text(&bot, &msg, user_lang.ai_no_api_key()).await?;
        return Ok(());
    }

    let file_id = match msg
        .photo()
        .and_then(|sizes| sizes.last())
        .map(|p| p.file.id.0.clone())
    {
        Some(id) => id,
        None => return Ok(()),
    };

    bot.send_chat_action(msg.chat.id, ChatAction::Typing)
        .await?;

    let image_bytes = match jpeg {
        Some(bytes) => bytes,
        None => match crate::bot::reactions::ingest_telegram_photo(&bot, &file_id).await {
            Ok(bytes) => {
                if let Err(err) =
                    db::photo::save_image_jpeg(&pool, msg.chat.id.0, msg.id.0, &bytes).await
                {
                    tracing::warn!(user_id = %user.id, "Failed to store photo JPEG: {err}");
                } else if let (Ok(Some(p)), Some(api_key)) = (
                    db::photo::find_photo_message(&pool, msg.chat.id.0, msg.id.0).await,
                    ai_ctx.api_key(),
                ) {
                    db::photo::spawn_embed_photo(
                        pool.clone(),
                        api_key.to_string(),
                        ai_ctx.embedding_model.clone(),
                        user.id,
                        p.id,
                        bytes.clone(),
                        p.caption.clone(),
                    );
                }
                bytes
            }
            Err(err) => {
                tracing::error!(user_id = %user.id, "Failed to download photo for AI: {err:#}");
                reply_text(&bot, &msg, user_lang.ai_error()).await?;
                return Ok(());
            }
        },
    };

    let group_context = if msg.chat.is_private() {
        None
    } else {
        match crate::db::group_members::context_for_chat(&pool, msg.chat.id.0).await {
            Ok(ctx) => Some(ctx),
            Err(err) => {
                tracing::warn!(
                    chat_id = %msg.chat.id.0,
                    "Failed to build group context: {err}"
                );
                None
            }
        }
    };

    let speaker = msg
        .from
        .as_ref()
        .map(speaker_label)
        .unwrap_or_else(|| format!("User {}", user.telegram_user_id));
    let user_ctx = UserContext {
        user_id: user.id,
        telegram_user_id: user.telegram_user_id,
        lang: user.lang(),
        web_search_enabled: user.web_search_enabled,
        is_admin: user.role == Role::Admin,
    };
    let ask_input = AskInput {
        prompt: &prompt,
        speaker: &speaker,
        group_context: group_context.as_ref(),
        image: Some(crate::ai::context::AskImage {
            bytes: &image_bytes,
            mime_type: crate::media::JPEG_MIME,
        }),
        bot: Some(&bot),
        chat_id: Some(msg.chat.id.0),
        reply_to_message_id: Some(msg.id),
    };

    match crate::ai::ask(&pool, &ai_ctx, &user_ctx, &ask_input).await {
        Ok(response) => {
            let html = crate::bot::markdown::md_to_html(&response);
            if html.trim().is_empty() {
                reply_text(&bot, &msg, user_lang.ai_empty_response()).await?;
            } else {
                reply_html(&bot, &msg, html).await?;
            }
        }
        Err(err) => {
            tracing::error!(user_id = %user.id, "AI photo ask failed: {err}");
            reply_text(&bot, &msg, user_lang.ai_error()).await?;
        }
    }

    Ok(())
}

/// Resolve a user argument that can be either a numeric Telegram user ID or a @username.
async fn resolve_user_arg(pool: &Db, arg: &str) -> Result<Option<User>, anyhow::Error> {
    let arg = arg.trim();
    if arg.starts_with('@') {
        db::users::find_by_username(pool, arg).await
    } else {
        match arg.parse::<i64>() {
            Ok(id) => db::users::find_by_telegram_id(pool, id).await,
            Err(_) => Ok(None),
        }
    }
}

/// Check if any of the user's active goals have been achieved by the latest data,
/// mark them achieved in the DB, and send a congratulatory message for each one.
async fn maybe_notify_achievements(
    bot: &Bot,
    chat_id: teloxide::types::ChatId,
    pool: &Db,
    user_id: i64,
    user_lang: Lang,
    lang: &Msg,
    ai_ctx: &AiContext,
) {
    let achieved = match db::goals::check_achievements(pool, user_id).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!("Failed to check goal achievements: {err}");
            return;
        }
    };

    for goal in achieved {
        let (text, fact) = match db::goals::GoalType::from_str(&goal.goal_type) {
            Some(db::goals::GoalType::Weight) => (
                lang.goal_achieved_weight(goal.target_value),
                format!("Weight goal reached: {} kg", goal.target_value),
            ),
            Some(db::goals::GoalType::TrainingFrequency) => (
                lang.goal_achieved_training(goal.target_value),
                format!(
                    "Weekly training-frequency goal reached: {} sessions",
                    goal.target_value
                ),
            ),
            None => continue,
        };
        if let Err(e) = bot.send_message(chat_id, &text).await {
            tracing::warn!("Failed to send achievement notification: {e}");
        }
        crate::ai::image::spawn_achievement_celebration(
            bot.clone(),
            pool.clone(),
            ai_ctx.clone(),
            chat_id,
            user_id,
            user_lang,
            fact,
            text,
        );
    }
}

fn compute_streak(dates: &[chrono::DateTime<chrono::Utc>]) -> i64 {
    if dates.is_empty() {
        return 0;
    }

    use chrono::Utc;
    let today = Utc::now().date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);
    let unique_dates: std::collections::HashSet<chrono::NaiveDate> =
        dates.iter().map(|d| d.date_naive()).collect();

    let latest = *unique_dates.iter().max().unwrap_or(&today);
    if latest < yesterday {
        return 0;
    }

    let mut count = 0i64;
    let mut check_date = latest;
    while unique_dates.contains(&check_date) {
        count += 1;
        check_date = match check_date.pred_opt() {
            Some(d) => d,
            None => break,
        };
        if count > 365 {
            break;
        }
    }
    count
}

/// Parse `/lasttraining [n]`: empty → 5; clamp to 1..=50.
fn parse_last_training_limit(arg: &str) -> usize {
    let arg = arg.trim();
    if arg.is_empty() {
        return 5;
    }
    arg.parse::<usize>().ok().unwrap_or(5).clamp(1, 50)
}

fn spawn_enrich_training(
    pool: Db,
    ai_ctx: AiContext,
    user_id: i64,
    lang: Lang,
    training_id: i64,
    original_description: String,
) {
    if ai_ctx.api_key.is_none() {
        return;
    }
    tokio::spawn(async move {
        match crate::ai::training_extract::extract_training_from_media(
            &pool,
            &ai_ctx,
            user_id,
            lang,
            Some(&original_description),
            None,
        )
        .await
        {
            Ok(extracted) => {
                let parsed = extracted.into_new_entry(user_id, &original_description, None, true);
                if parsed.activity_slug.is_none()
                    && parsed.endurance.is_none()
                    && parsed.strength_sets.is_empty()
                {
                    return;
                }
                if let Err(e) = db::training::update(
                    &pool,
                    training_id,
                    user_id,
                    false,
                    db::training::UpdateTraining {
                        description: None,
                        recorded_at: None,
                        activity_slug: parsed.activity_slug,
                        endurance: parsed.endurance,
                        strength_sets: if parsed.strength_sets.is_empty() {
                            None
                        } else {
                            Some(parsed.strength_sets)
                        },
                    },
                )
                .await
                {
                    tracing::warn!(
                        user_id = %user_id,
                        training_id = %training_id,
                        "Failed to enrich training entry: {e}"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %user_id,
                    training_id = %training_id,
                    "Training text extract failed: {e:#}"
                );
            }
        }
    });
}

async fn handle_update_training(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let arg = arg.trim();
    let Some((id_str, rest)) = arg.split_once(char::is_whitespace) else {
        cmd_reply(&bot, &msg, hist, user_lang.training_update_usage()).await?;
        return Ok(());
    };
    let Ok(id) = id_str.parse::<i64>() else {
        cmd_reply(&bot, &msg, hist, user_lang.training_update_usage()).await?;
        return Ok(());
    };
    let description = rest.trim();
    if description.is_empty() {
        cmd_reply(&bot, &msg, hist, user_lang.training_update_usage()).await?;
        return Ok(());
    }

    let is_admin = user.role == Role::Admin;
    match db::training::update(
        pool,
        id,
        user.id,
        is_admin,
        db::training::UpdateTraining {
            description: Some(description.to_string()),
            recorded_at: None,
            ..Default::default()
        },
    )
    .await
    {
        Ok(Some(entry)) => {
            cmd_reply(
                &bot,
                &msg,
                hist,
                user_lang.training_updated(
                    entry.id,
                    &entry.description,
                    &entry.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                ),
            )
            .await?;
        }
        Ok(None) => {
            cmd_reply(
                &bot,
                &msg,
                hist,
                user_lang.training_not_found_or_forbidden(),
            )
            .await?;
        }
        Err(err) => {
            tracing::error!("Failed to update training {id}: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.training_update_error()).await?;
        }
    }
    Ok(())
}

async fn handle_show_training(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let arg = arg.trim();
    let Ok(id) = arg.parse::<i64>() else {
        cmd_reply(&bot, &msg, hist, user_lang.training_show_usage()).await?;
        return Ok(());
    };

    match db::training::find_by_id(pool, id).await {
        Ok(Some(entry)) if entry.user_id == user.id || user.role == Role::Admin => {
            cmd_reply(&bot, &msg, hist, db::training::format_card(&entry)).await?;
        }
        Ok(Some(_)) | Ok(None) => {
            cmd_reply(
                &bot,
                &msg,
                hist,
                user_lang.training_not_found_or_forbidden(),
            )
            .await?;
        }
        Err(err) => {
            tracing::error!("Failed to fetch training {id}: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.training_fetch_error()).await?;
        }
    }
    Ok(())
}

async fn handle_del_training(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let arg = arg.trim();
    let Ok(id) = arg.parse::<i64>() else {
        cmd_reply(&bot, &msg, hist, user_lang.training_delete_usage()).await?;
        return Ok(());
    };

    let is_admin = user.role == Role::Admin;
    match db::training::delete(pool, id, user.id, is_admin).await {
        Ok(true) => {
            cmd_reply(&bot, &msg, hist, user_lang.training_deleted(id)).await?;
        }
        Ok(false) => {
            cmd_reply(
                &bot,
                &msg,
                hist,
                user_lang.training_not_found_or_forbidden(),
            )
            .await?;
        }
        Err(err) => {
            tracing::error!("Failed to delete training {id}: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.training_delete_error()).await?;
        }
    }
    Ok(())
}

async fn handle_timezone(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    input: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let input = input.trim();
    if input.is_empty() {
        cmd_reply(&bot, &msg, hist, user_lang.timezone_current(&user.timezone)).await?;
        return Ok(());
    }
    match chrono_tz::Tz::from_str(input) {
        Ok(_) => match db::users::set_timezone(pool, user.telegram_user_id, input).await {
            Ok(true) => {
                cmd_reply(&bot, &msg, hist, user_lang.timezone_set(input)).await?;
            }
            Ok(false) => {
                cmd_reply(&bot, &msg, hist, user_lang.timezone_save_error()).await?;
            }
            Err(err) => {
                tracing::error!("Failed to set timezone: {err}");
                cmd_reply(&bot, &msg, hist, user_lang.timezone_save_error()).await?;
            }
        },
        Err(_) => {
            cmd_reply(&bot, &msg, hist, user_lang.timezone_invalid(input)).await?;
        }
    }
    Ok(())
}

async fn handle_lang(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    input: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        let code = user.language_code.as_deref().unwrap_or("en");
        cmd_reply(&bot, &msg, hist, user_lang.lang_current(code)).await?;
        return Ok(());
    }
    let code = match input.as_str() {
        "en" | "fr" => input,
        _ => {
            cmd_reply(&bot, &msg, hist, user_lang.lang_usage()).await?;
            return Ok(());
        }
    };
    match db::users::set_language_code(pool, user.telegram_user_id, &code).await {
        Ok(true) => {
            let reply_lang = Msg::new(Lang::from_iso_code(Some(&code)));
            cmd_reply(&bot, &msg, hist, reply_lang.lang_set(&code)).await?;
        }
        Ok(false) => {
            cmd_reply(&bot, &msg, hist, user_lang.lang_save_error()).await?;
        }
        Err(err) => {
            tracing::error!("Failed to set language: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.lang_save_error()).await?;
        }
    }
    Ok(())
}

async fn handle_weight_chart(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let days = match crate::bot::weight_graph::parse_days_arg(arg) {
        Ok(d) => d,
        Err(()) => {
            cmd_reply(&bot, &msg, hist, user_lang.weight_chart_usage()).await?;
            return Ok(());
        }
    };

    send_weight_chart(
        bot,
        msg.chat.id,
        Some(msg),
        pool,
        user_lang,
        user,
        days,
        Some(hist),
    )
    .await
}

/// Send a personal weight history chart to `chat_id` (default window: 90 days).
pub(crate) async fn send_weight_chart(
    bot: &Bot,
    chat_id: ChatId,
    msg: Option<&Message>,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    days: Option<i64>,
    hist: Option<&mut CommandHistory<'_>>,
) -> ResponseResult<()> {
    let entries = match db::weight::list_for_user(pool, user.id).await {
        Ok(e) => e,
        Err(err) => {
            tracing::error!("Failed to fetch weight history: {err}");
            let text = user_lang.weight_fetch_error();
            if let (Some(msg), Some(hist)) = (msg, hist) {
                cmd_reply(bot, msg, hist, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
            return Ok(());
        }
    };

    let goal_kg = match db::goals::list_active(pool, user.id).await {
        Ok(goals) => goals
            .into_iter()
            .find(|g| {
                db::goals::GoalType::from_str(&g.goal_type) == Some(db::goals::GoalType::Weight)
            })
            .map(|g| g.target_value),
        Err(err) => {
            tracing::warn!("Failed to load weight goal for chart: {err}");
            None
        }
    };

    let chart = match crate::bot::weight_graph::build_weight_chart(
        &entries,
        &user.timezone,
        days,
        goal_kg,
        user_lang,
    ) {
        Ok(c) => c,
        Err(crate::bot::weight_graph::BuildWeightChartError::NoEntries) => {
            let text = user_lang.weight_no_entries();
            if let (Some(msg), Some(hist)) = (msg, hist) {
                cmd_reply(bot, msg, hist, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
            return Ok(());
        }
        Err(crate::bot::weight_graph::BuildWeightChartError::NoDataInWindow) => {
            let text = user_lang.weight_chart_no_data_in_window();
            if let (Some(msg), Some(hist)) = (msg, hist) {
                cmd_reply(bot, msg, hist, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
            return Ok(());
        }
        Err(crate::bot::weight_graph::BuildWeightChartError::Render(err)) => {
            tracing::error!("Failed to render weight chart: {err:#}");
            let text = user_lang.weight_chart_error();
            if let (Some(msg), Some(hist)) = (msg, hist) {
                cmd_reply(bot, msg, hist, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
            return Ok(());
        }
    };

    bot.send_photo(
        chat_id,
        InputFile::memory(chart.png).file_name("weight.png"),
    )
    .caption(chart.caption.clone())
    .await?;

    if let Some(hist) = hist {
        hist.note_reply(&chart.caption).await;
    }

    Ok(())
}

async fn handle_nutrition(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    if !msg.chat.is_private() {
        cmd_reply(bot, msg, hist, user_lang.nutrition_private_only()).await?;
        return Ok(());
    }

    let date = match crate::bot::nutrition_table::parse_date_arg(
        arg,
        &user.timezone,
        chrono::Utc::now(),
    ) {
        Ok(d) => d,
        Err(()) => {
            cmd_reply(bot, msg, hist, user_lang.nutrition_usage()).await?;
            return Ok(());
        }
    };

    send_nutrition_table(
        bot,
        msg.chat.id,
        Some(msg),
        pool,
        user_lang,
        user,
        date,
        Some(hist),
    )
    .await
}

async fn send_nutrition_table(
    bot: &Bot,
    chat_id: ChatId,
    msg: Option<&Message>,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    date: chrono::NaiveDate,
    hist: Option<&mut CommandHistory<'_>>,
) -> ResponseResult<()> {
    let day = match db::meals::day_for_user(pool, user.id, &user.timezone, date).await {
        Ok(d) => d,
        Err(err) => {
            tracing::error!("Failed to fetch nutrition day: {err}");
            let text = user_lang.nutrition_fetch_error();
            if let (Some(msg), Some(hist)) = (msg, hist) {
                cmd_reply(bot, msg, hist, text).await?;
            } else {
                bot.send_message(chat_id, text).await?;
            }
            return Ok(());
        }
    };

    let table =
        match crate::bot::nutrition_table::build_nutrition_table(&day, &user.timezone, user_lang) {
            Ok(t) => t,
            Err(crate::bot::nutrition_table::BuildNutritionTableError::Empty) => {
                let text = user_lang.nutrition_empty(&date.format("%Y-%m-%d").to_string());
                if let (Some(msg), Some(hist)) = (msg, hist) {
                    cmd_reply(bot, msg, hist, text).await?;
                } else {
                    bot.send_message(chat_id, text).await?;
                }
                return Ok(());
            }
            Err(crate::bot::nutrition_table::BuildNutritionTableError::Render(err)) => {
                tracing::error!("Failed to render nutrition table: {err:#}");
                let text = user_lang.nutrition_error();
                if let (Some(msg), Some(hist)) = (msg, hist) {
                    cmd_reply(bot, msg, hist, text).await?;
                } else {
                    bot.send_message(chat_id, text).await?;
                }
                return Ok(());
            }
        };

    bot.send_photo(
        chat_id,
        InputFile::memory(table.png).file_name("nutrition.png"),
    )
    .caption(table.caption.clone())
    .await?;

    if let Some(hist) = hist {
        hist.note_reply(&table.caption).await;
    }

    Ok(())
}

async fn handle_streak(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user_id: i64,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let weight_entries = crate::db::weight::list_for_user(pool, user_id)
        .await
        .unwrap_or_default();
    let weight_dates: Vec<_> = weight_entries.iter().map(|e| e.recorded_at).collect();
    let weight_streak = compute_streak(&weight_dates);

    let training_entries = crate::db::training::list_for_user(pool, user_id)
        .await
        .unwrap_or_default();
    let training_dates: Vec<_> = training_entries.iter().map(|e| e.recorded_at).collect();
    let training_streak = compute_streak(&training_dates);

    let text = if weight_streak == 0 && training_streak == 0 {
        user_lang.streak_empty()
    } else {
        user_lang.streak_message(weight_streak, training_streak)
    };

    cmd_reply(&bot, &msg, hist, text).await?;
    Ok(())
}

async fn handle_nudges(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let arg = arg.trim();
    if arg.is_empty() || arg == "status" {
        cmd_reply(
            &bot,
            &msg,
            hist,
            user_lang.nudge_status(user.nudges_enabled, user.nudge_inactivity_days),
        )
        .await?;
        return Ok(());
    }

    if arg == "on" {
        match crate::db::users::set_nudge_settings(pool, user.telegram_user_id, Some(true), None)
            .await
        {
            Ok(true) => {
                cmd_reply(
                    &bot,
                    &msg,
                    hist,
                    user_lang.nudge_enabled(user.nudge_inactivity_days),
                )
                .await?;
            }
            _ => {
                cmd_reply(&bot, &msg, hist, user_lang.nudge_error()).await?;
            }
        }
        return Ok(());
    }

    if arg == "off" {
        match crate::db::users::set_nudge_settings(pool, user.telegram_user_id, Some(false), None)
            .await
        {
            Ok(true) => {
                cmd_reply(&bot, &msg, hist, user_lang.nudge_disabled()).await?;
            }
            _ => {
                cmd_reply(&bot, &msg, hist, user_lang.nudge_error()).await?;
            }
        }
        return Ok(());
    }

    if let Some(days_str) = arg.strip_prefix("at ") {
        let days_str = days_str.trim();
        if let Ok(days) = days_str.parse::<i64>()
            && (1..=365).contains(&days)
        {
            match crate::db::users::set_nudge_settings(
                pool,
                user.telegram_user_id,
                Some(true),
                Some(days),
            )
            .await
            {
                Ok(true) => {
                    cmd_reply(&bot, &msg, hist, user_lang.nudge_threshold_set(days)).await?;
                }
                _ => {
                    cmd_reply(&bot, &msg, hist, user_lang.nudge_error()).await?;
                }
            }
            return Ok(());
        }
    }

    cmd_reply(&bot, &msg, hist, user_lang.nudge_usage()).await?;
    Ok(())
}

async fn handle_monthly_graph(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    if msg.chat.is_private() {
        cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_group_only()).await?;
        return Ok(());
    }

    let chat_id = msg.chat.id.0;
    let arg = arg.trim().to_lowercase();

    // Any authorized user can request last month's chart.
    if arg == "last" {
        match crate::bot::monthly_graph::send_monthly_graph(
            bot,
            pool,
            chat_id,
            crate::bot::monthly_graph::GraphWindow::PreviousMonth,
            false,
        )
        .await
        {
            Ok(()) => {
                hist.note_reply("Sent monthly training graph").await;
            }
            Err(err) => {
                tracing::error!("Failed to send monthly graph last: {err:#}");
                cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_error()).await?;
            }
        }
        return Ok(());
    }

    // Rolling window charts, e.g. /monthlygraph 4w or /monthlygraph 3m
    match crate::bot::monthly_graph::parse_rolling_arg(&arg) {
        Ok(Some(period)) => {
            let (window, note) = match period {
                crate::bot::monthly_graph::RollingPeriod::Weeks(weeks) => (
                    crate::bot::monthly_graph::GraphWindow::LastWeeks(weeks),
                    format!("Sent training graph (last {weeks} weeks)"),
                ),
                crate::bot::monthly_graph::RollingPeriod::Months(months) => (
                    crate::bot::monthly_graph::GraphWindow::LastMonths(months),
                    format!("Sent training graph (last {months} months)"),
                ),
            };
            match crate::bot::monthly_graph::send_monthly_graph(bot, pool, chat_id, window, false)
                .await
            {
                Ok(()) => {
                    hist.note_reply(&note).await;
                }
                Err(err) => {
                    tracing::error!("Failed to send rolling training graph: {err:#}");
                    cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_error()).await?;
                }
            }
            return Ok(());
        }
        Err(()) => {
            cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_usage()).await?;
            return Ok(());
        }
        Ok(None) => {}
    }

    if user.role != Role::Admin {
        cmd_reply(&bot, &msg, hist, user_lang.not_admin()).await?;
        return Ok(());
    }

    if arg.is_empty() || arg == "status" {
        let enabled = db::chat_settings::is_monthly_training_graph_enabled(pool, chat_id).await;
        cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_status(enabled)).await?;
        return Ok(());
    }

    if arg == "on" {
        match db::chat_settings::set_monthly_training_graph_enabled(pool, chat_id, true).await {
            Ok(()) => {
                cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_enabled()).await?;
            }
            Err(err) => {
                tracing::error!("Failed to enable monthly graph: {err}");
                cmd_reply(&bot, &msg, hist, user_lang.db_error()).await?;
            }
        }
        return Ok(());
    }

    if arg == "off" {
        match db::chat_settings::set_monthly_training_graph_enabled(pool, chat_id, false).await {
            Ok(()) => {
                cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_disabled()).await?;
            }
            Err(err) => {
                tracing::error!("Failed to disable monthly graph: {err}");
                cmd_reply(&bot, &msg, hist, user_lang.db_error()).await?;
            }
        }
        return Ok(());
    }

    if arg == "now" {
        match crate::bot::monthly_graph::send_monthly_graph(
            bot,
            pool,
            chat_id,
            crate::bot::monthly_graph::GraphWindow::CurrentMonth,
            false,
        )
        .await
        {
            Ok(()) => {
                hist.note_reply("Sent monthly training graph").await;
            }
            Err(err) => {
                tracing::error!("Failed to send monthly graph now: {err:#}");
                cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_error()).await?;
            }
        }
        return Ok(());
    }

    cmd_reply(&bot, &msg, hist, user_lang.monthly_graph_usage()).await?;
    Ok(())
}

async fn handle_set_sport_emoji(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user_lang: &Msg,
    user: &User,
    arg: &str,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    if user.role != Role::Admin {
        cmd_reply(&bot, &msg, hist, user_lang.not_admin()).await?;
        return Ok(());
    }

    let chat_id = msg.chat.id.0;
    let arg = arg.trim();

    if arg.is_empty() {
        let current = db::chat_settings::get_sport_emoji(pool, chat_id).await;
        cmd_reply(&bot, &msg, hist, user_lang.sport_emoji_current(&current)).await?;
        return Ok(());
    }

    // Validate that the argument is a single emoji character.
    if arg.chars().count() != 1 {
        cmd_reply(&bot, &msg, hist, user_lang.sport_emoji_invalid()).await?;
        return Ok(());
    }

    match db::chat_settings::set_sport_emoji(pool, chat_id, arg).await {
        Ok(()) => {
            cmd_reply(&bot, &msg, hist, user_lang.sport_emoji_set(arg)).await?;
        }
        Err(err) => {
            tracing::error!("Failed to set sport reaction emoji: {err}");
            cmd_reply(&bot, &msg, hist, user_lang.sport_emoji_error()).await?;
        }
    }

    Ok(())
}

async fn handle_rag_status(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user: &User,
    ai_ctx: &AiContext,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    let chunk_count = crate::db::chunks::count_for_user(pool, user.id)
        .await
        .unwrap_or(0);
    let status = if ai_ctx.rag_enabled {
        "enabled"
    } else {
        "disabled"
    };
    let text = format!(
        "RAG status: {status}\n\
         Embedding model: {}\n\
         Top-K chunks: {}\n\
         Stored chunks: {chunk_count}",
        ai_ctx.embedding_model, ai_ctx.rag_top_k,
    );
    cmd_reply(&bot, &msg, hist, text).await?;
    Ok(())
}

async fn handle_rag_rebuild(
    bot: &Bot,
    msg: &Message,
    pool: &Db,
    user: &User,
    ai_ctx: &AiContext,
    hist: &mut CommandHistory<'_>,
) -> ResponseResult<()> {
    if user.role != Role::Admin {
        cmd_reply(&bot, &msg, hist, "This command is admin-only.").await?;
        return Ok(());
    }

    let Some(api_key) = ai_ctx.api_key.as_deref() else {
        cmd_reply(&bot, &msg, hist, "AI API key is not configured.").await?;
        return Ok(());
    };

    cmd_reply(
        &bot,
        &msg,
        hist,
        "Rebuilding RAG embeddings from conversation history...",
    )
    .await?;

    const BATCH_SIZE: i64 = 50;
    let mut total_embedded = 0usize;

    loop {
        let batch =
            match crate::db::chunks::list_unembedded_conversations(pool, user.id, BATCH_SIZE).await
            {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("Failed to fetch unembedded conversations: {e}");
                    cmd_reply(
                        &bot,
                        &msg,
                        hist,
                        "Failed to fetch conversations for rebuild.",
                    )
                    .await?;
                    return Ok(());
                }
            };

        if batch.is_empty() {
            break;
        }

        let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();
        let embeddings =
            match crate::embeddings::embed_batch(api_key, &ai_ctx.embedding_model, texts.clone())
                .await
            {
                Ok((e, usage)) => {
                    crate::ai::log::persist_embed(
                        pool,
                        user.id,
                        &ai_ctx.embedding_model,
                        usage,
                        crate::db::ai_usage::KIND_EMBED_RAG,
                        None,
                    )
                    .await;
                    e
                }
                Err(e) => {
                    tracing::error!("Failed to embed batch: {e}");
                    cmd_reply(&bot, &msg, hist, format!("Embedding failed: {e}")).await?;
                    return Ok(());
                }
            };

        if embeddings.len() != batch.len() {
            cmd_reply(&bot, &msg, hist, "Embedding batch size mismatch.").await?;
            return Ok(());
        }

        for (conv, embedding) in batch.into_iter().zip(embeddings) {
            if let Err(e) = crate::db::chunks::insert_conversation_chunk(
                pool,
                user.id,
                conv.chat_id,
                &conv.role,
                &conv.content,
                conv.id.unwrap(),
                &embedding,
            )
            .await
            {
                tracing::warn!(
                    "Failed to insert chunk for conversation {}: {e}",
                    conv.id.unwrap()
                );
            } else {
                total_embedded += 1;
            }
        }
    }

    cmd_reply(
        &bot,
        &msg,
        hist,
        format!("RAG rebuild complete. Embedded {total_embedded} messages."),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn parse_time_of_day_24h() {
        assert_eq!(parse_time_of_day("14:30"), Some((14, 30)));
        assert_eq!(parse_time_of_day("09:05"), Some((9, 5)));
        assert_eq!(parse_time_of_day("23h59"), Some((23, 59)));
        assert_eq!(parse_time_of_day("7"), Some((7, 0)));
    }

    #[test]
    fn parse_time_of_day_am_pm() {
        assert_eq!(parse_time_of_day("2pm"), Some((14, 0)));
        assert_eq!(parse_time_of_day("12:30pm"), Some((12, 30)));
        assert_eq!(parse_time_of_day("12am"), Some((0, 0)));
        assert_eq!(parse_time_of_day("10:15am"), Some((10, 15)));
    }

    #[test]
    fn parse_time_of_day_invalid() {
        assert_eq!(parse_time_of_day("25:00"), None);
        assert_eq!(parse_time_of_day("12:60"), None);
        assert_eq!(parse_time_of_day("abc"), None);
        assert_eq!(parse_time_of_day(""), None);
    }

    #[test]
    fn parse_weekday_variants() {
        assert_eq!(parse_weekday("monday"), Some(chrono::Weekday::Mon));
        assert_eq!(parse_weekday("Tue"), Some(chrono::Weekday::Tue));
        assert_eq!(parse_weekday("Tues"), Some(chrono::Weekday::Tue));
        assert_eq!(parse_weekday("fri"), Some(chrono::Weekday::Fri));
        assert_eq!(parse_weekday("notaday"), None);
    }

    #[test]
    fn parse_month_day_variants() {
        assert_eq!(parse_month_day("1"), Some(1));
        assert_eq!(parse_month_day("15th"), Some(15));
        assert_eq!(parse_month_day("22nd"), Some(22));
        assert_eq!(parse_month_day("23rd"), Some(23));
        assert_eq!(parse_month_day("32"), None);
        assert_eq!(parse_month_day("0"), None);
    }

    #[test]
    fn strip_mention_basic() {
        assert_eq!(strip_mention("hello @bot", "bot"), "hello");
        assert_eq!(strip_mention("@Bot hello", "bot"), "hello");
        assert_eq!(strip_mention("no mention", "bot"), "no mention");
        assert_eq!(strip_mention("@BOT hello", "bot"), "hello");
    }

    #[test]
    fn compute_streak_empty() {
        assert_eq!(compute_streak(&[]), 0);
    }

    #[test]
    fn compute_streak_today_and_yesterday() {
        let today = Utc::now();
        let yesterday = today - chrono::Duration::days(1);
        assert_eq!(compute_streak(&[today, yesterday]), 2);
    }

    #[test]
    fn compute_streak_broken() {
        let today = Utc::now();
        let two_days_ago = today - chrono::Duration::days(2);
        assert_eq!(compute_streak(&[today, two_days_ago]), 1);
    }

    #[test]
    fn compute_streak_old_only() {
        let three_days_ago = Utc::now() - chrono::Duration::days(3);
        assert_eq!(compute_streak(&[three_days_ago]), 0);
    }
}
