use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage};

use crate::ai::context::AiContext;
use crate::db::{self, Db, users::Role};
use crate::i18n::{Lang, Msg};

// ── Keyboard layouts ───────────────────────────────────────────────────────

pub fn main_menu(lang: Lang) -> InlineKeyboardMarkup {
    let msg = Msg::new(lang);
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(msg.btn_record_weight(), "menu:weight"),
            InlineKeyboardButton::callback(msg.btn_log_training(), "menu:training"),
        ],
        vec![
            InlineKeyboardButton::callback(msg.btn_ask_ai(), "ask"),
            InlineKeyboardButton::callback(msg.btn_my_stats(), "menu:stats"),
        ],
        vec![InlineKeyboardButton::callback(
            msg.btn_goals(),
            "menu:goals",
        )],
    ])
}

pub fn goals_menu(lang: Lang) -> InlineKeyboardMarkup {
    let msg = Msg::new(lang);
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(msg.btn_set_weight_goal(), "goal:set_weight"),
            InlineKeyboardButton::callback(msg.btn_set_training_goal(), "goal:set_training"),
        ],
        vec![
            InlineKeyboardButton::callback(msg.btn_my_goals(), "goal:my_goals"),
            InlineKeyboardButton::callback(msg.btn_back(), "menu:main"),
        ],
    ])
}

pub fn weight_menu(lang: Lang, latest_kg: Option<f64>) -> InlineKeyboardMarkup {
    let msg = Msg::new(lang);
    let presets = weight_presets(latest_kg);
    InlineKeyboardMarkup::new(vec![
        vec![
            weight_preset_btn(presets[0]),
            weight_preset_btn(presets[1]),
            weight_preset_btn(presets[2]),
        ],
        vec![
            weight_preset_btn(presets[3]),
            weight_preset_btn(presets[4]),
            weight_preset_btn(presets[5]),
        ],
        vec![
            InlineKeyboardButton::callback(msg.btn_custom(), "weight:custom"),
            InlineKeyboardButton::callback(msg.btn_back(), "menu:main"),
        ],
    ])
}

/// Six 0.5 kg steps centered on the latest weight (fallback 80.0).
fn weight_presets(latest_kg: Option<f64>) -> [f64; 6] {
    let center = latest_kg.unwrap_or(80.0);
    let base = (center * 2.0).round() / 2.0;
    [
        base - 1.0,
        base - 0.5,
        base,
        base + 0.5,
        base + 1.0,
        base + 1.5,
    ]
}

fn weight_preset_btn(kg: f64) -> InlineKeyboardButton {
    let label = format!("{kg:.1}");
    InlineKeyboardButton::callback(label.clone(), format!("weight:{label}"))
}

pub fn training_menu(lang: Lang) -> InlineKeyboardMarkup {
    let msg = Msg::new(lang);
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(msg.btn_ran_5km(), "training:ran_5km"),
            InlineKeyboardButton::callback(msg.btn_gym_session(), "training:gym"),
        ],
        vec![
            InlineKeyboardButton::callback(msg.btn_cycling(), "training:cycling"),
            InlineKeyboardButton::callback(msg.btn_custom(), "training:custom"),
        ],
        vec![InlineKeyboardButton::callback(msg.btn_back(), "menu:main")],
    ])
}

pub fn stats_menu(lang: Lang) -> InlineKeyboardMarkup {
    let msg = Msg::new(lang);
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            msg.btn_last_weight(),
            "stats:last_weight",
        )],
        vec![InlineKeyboardButton::callback(
            msg.btn_weight_chart(),
            "stats:weight_chart",
        )],
        vec![InlineKeyboardButton::callback(
            msg.btn_last_training(),
            "stats:last_training",
        )],
        vec![InlineKeyboardButton::callback(msg.btn_back(), "menu:main")],
    ])
}

// ── Callback handling ──────────────────────────────────────────────────────

pub async fn handle_callback(
    bot: Bot,
    query: CallbackQuery,
    pool: Db,
    ai_ctx: AiContext,
) -> ResponseResult<()> {
    // Answer the callback query to stop the loading spinner
    if let Err(e) = bot.answer_callback_query(query.id.clone()).await {
        tracing::warn!("Failed to answer callback query: {e}");
    }

    let from = &query.from;
    let telegram_user_id = from.id.0 as i64;
    let language_code = from.language_code.clone();
    let msg_lang = Msg::new(Lang::from_iso_code(language_code.as_deref()));

    if let Err(err) = db::users::update_profile(
        &pool,
        telegram_user_id,
        from.username.clone(),
        Some(from.first_name.clone()),
        from.language_code.clone(),
    )
    .await
    {
        tracing::error!("Failed to update profile for user {telegram_user_id}: {err}");
    }

    // Auth check
    let user = match db::users::find_by_telegram_id(&pool, telegram_user_id).await {
        Ok(Some(u)) if u.role != Role::Pending => u,
        Ok(Some(_)) => {
            bot.answer_callback_query(query.id)
                .text(msg_lang.not_authorized_pending())
                .show_alert(true)
                .await?;
            return Ok(());
        }
        Ok(None) => {
            bot.answer_callback_query(query.id)
                .text(msg_lang.not_authorized())
                .show_alert(true)
                .await?;
            return Ok(());
        }
        Err(err) => {
            tracing::error!("Failed to look up user {telegram_user_id}: {err}");
            bot.answer_callback_query(query.id)
                .text(msg_lang.internal_error())
                .show_alert(true)
                .await?;
            return Ok(());
        }
    };

    let user_lang = Msg::new(user.lang());
    let data = query.data.as_deref().unwrap_or("");
    tracing::info!(
        user_id = user.id,
        telegram_user_id,
        callback = data,
        "bot callback {}",
        data
    );

    // Determine target chat and message id for editing
    let (chat_id, message_id) = match &query.message {
        Some(MaybeInaccessibleMessage::Regular(msg)) => (msg.chat.id, Some(msg.id)),
        Some(MaybeInaccessibleMessage::Inaccessible(_)) => {
            bot.send_message(query.from.id, "Message is no longer accessible.")
                .await?;
            return Ok(());
        }
        None => {
            bot.send_message(query.from.id, "Use /start to begin.")
                .await?;
            return Ok(());
        }
    };

    match parse_callback(data) {
        CallbackAction::Menu(menu) => {
            let (text, markup) = match menu {
                Menu::Main => (user_lang.menu_main_text(), main_menu(user.lang())),
                Menu::Weight => {
                    let latest = db::weight::latest_for_user(&pool, user.id)
                        .await
                        .ok()
                        .flatten()
                        .map(|e| e.weight_kg);
                    (
                        user_lang.menu_weight_text(),
                        weight_menu(user.lang(), latest),
                    )
                }
                Menu::Training => (user_lang.menu_training_text(), training_menu(user.lang())),
                Menu::Stats => (user_lang.menu_stats_text(), stats_menu(user.lang())),
                Menu::Goals => (user_lang.menu_goals_text(), goals_menu(user.lang())),
            };
            if let Some(mid) = message_id {
                if let Err(e) = bot
                    .edit_message_text(chat_id, mid, text)
                    .reply_markup(markup.clone())
                    .await
                {
                    tracing::warn!("Failed to edit message for menu: {e}");
                    bot.send_message(chat_id, text).reply_markup(markup).await?;
                }
            } else {
                bot.send_message(chat_id, text).reply_markup(markup).await?;
            }
        }

        CallbackAction::Weight(value) => match value {
            WeightValue::Custom => {
                bot.send_message(chat_id, user_lang.weight_custom_prompt())
                    .await?;
            }
            WeightValue::Kg(kg) => {
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
                        bot.send_message(
                            chat_id,
                            user_lang.weight_recorded(
                                e.weight_kg,
                                &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            ),
                        )
                        .await?;
                        maybe_notify_achievements(
                            &bot,
                            chat_id,
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
                        bot.send_message(chat_id, user_lang.weight_save_error())
                            .await?;
                    }
                }
            }
        },

        CallbackAction::Training(value) => match value {
            TrainingValue::Custom => {
                bot.send_message(chat_id, user_lang.training_custom_prompt())
                    .await?;
            }
            TrainingValue::Preset(desc) => {
                match db::training::try_insert(
                    &pool,
                    db::training::NewTrainingEntry::simple(user.id, desc.clone(), None),
                )
                .await
                {
                    Ok(db::training::TrainingInsert::Created(e)) => {
                        bot.send_message(
                            chat_id,
                            user_lang.training_logged(
                                &e.description,
                                &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            ),
                        )
                        .await?;
                        maybe_notify_achievements(
                            &bot,
                            chat_id,
                            &pool,
                            user.id,
                            user.lang(),
                            &user_lang,
                            &ai_ctx,
                        )
                        .await;
                    }
                    Ok(db::training::TrainingInsert::Duplicate(e)) => {
                        bot.send_message(
                            chat_id,
                            user_lang.training_already_logged(
                                e.id,
                                &e.recorded_at.format("%Y-%m-%d %H:%M UTC").to_string(),
                            ),
                        )
                        .await?;
                    }
                    Err(err) => {
                        tracing::error!("Failed to insert training: {err}");
                        bot.send_message(chat_id, user_lang.training_save_error())
                            .await?;
                    }
                }
            }
        },

        CallbackAction::Stats(stat) => match stat {
            Stat::LastWeight => match db::weight::latest_for_user(&pool, user.id).await {
                Err(err) => {
                    tracing::error!("Failed to fetch weight: {err}");
                    bot.send_message(chat_id, user_lang.weight_fetch_error())
                        .await?;
                }
                Ok(None) => {
                    bot.send_message(chat_id, user_lang.weight_no_entries())
                        .await?;
                }
                Ok(Some(e)) => {
                    bot.send_message(
                        chat_id,
                        user_lang.weight_last(
                            e.weight_kg,
                            &e.recorded_at.format("%Y-%m-%d").to_string(),
                        ),
                    )
                    .await?;
                }
            },
            Stat::WeightChart => {
                crate::bot::handlers::send_weight_chart(
                    &bot,
                    chat_id,
                    None,
                    &pool,
                    &user_lang,
                    &user,
                    Some(90),
                    None,
                )
                .await?;
            }
            Stat::LastTraining => match db::training::list_for_user(&pool, user.id).await {
                Err(err) => {
                    tracing::error!("Failed to fetch training: {err}");
                    bot.send_message(chat_id, user_lang.training_fetch_error())
                        .await?;
                }
                Ok(entries) if entries.is_empty() => {
                    bot.send_message(chat_id, user_lang.training_no_entries())
                        .await?;
                }
                Ok(entries) => {
                    let lines: Vec<String> = entries
                        .into_iter()
                        .take(5)
                        .map(|e| {
                            user_lang.training_session_entry(
                                e.id,
                                &e.recorded_at.format("%Y-%m-%d").to_string(),
                                &db::training::summary_line(&e),
                            )
                        })
                        .collect();
                    bot.send_message(
                        chat_id,
                        format!(
                            "{}{}",
                            user_lang.training_sessions_header(),
                            lines.join("\n")
                        ),
                    )
                    .await?;
                }
            },
        },

        CallbackAction::Ask => {
            bot.send_message(chat_id, user_lang.ask_description())
                .await?;
        }

        CallbackAction::Goal(action) => match action {
            GoalAction::SetWeight => {
                bot.send_message(chat_id, user_lang.goal_weight_set_prompt())
                    .await?;
            }
            GoalAction::SetTraining => {
                bot.send_message(chat_id, user_lang.goal_training_set_prompt())
                    .await?;
            }
            GoalAction::MyGoals => match db::goals::list_active(&pool, user.id).await {
                Err(err) => {
                    tracing::error!("Failed to list goals: {err}");
                    bot.send_message(chat_id, user_lang.db_error()).await?;
                }
                Ok(goals) if goals.is_empty() => {
                    bot.send_message(chat_id, user_lang.goal_no_goals()).await?;
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
                    bot.send_message(chat_id, text).await?;
                }
            },
        },

        CallbackAction::Unknown => {
            tracing::warn!("Unknown callback data: {}", data);
        }
    }

    Ok(())
}

// ── Callback data parsing ──────────────────────────────────────────────────

enum Menu {
    Main,
    Weight,
    Training,
    Stats,
    Goals,
}

enum WeightValue {
    Custom,
    Kg(f64),
}

enum TrainingValue {
    Custom,
    Preset(String),
}

enum Stat {
    LastWeight,
    WeightChart,
    LastTraining,
}

enum CallbackAction {
    Menu(Menu),
    Weight(WeightValue),
    Training(TrainingValue),
    Stats(Stat),
    Goal(GoalAction),
    Ask,
    Unknown,
}

enum GoalAction {
    SetWeight,
    SetTraining,
    MyGoals,
}

fn parse_callback(data: &str) -> CallbackAction {
    match data {
        "ask" => return CallbackAction::Ask,
        "menu:main" => return CallbackAction::Menu(Menu::Main),
        "menu:weight" => return CallbackAction::Menu(Menu::Weight),
        "menu:training" => return CallbackAction::Menu(Menu::Training),
        "menu:stats" => return CallbackAction::Menu(Menu::Stats),
        "menu:goals" => return CallbackAction::Menu(Menu::Goals),
        "weight:custom" => return CallbackAction::Weight(WeightValue::Custom),
        "training:custom" => return CallbackAction::Training(TrainingValue::Custom),
        "stats:last_weight" => return CallbackAction::Stats(Stat::LastWeight),
        "stats:weight_chart" => return CallbackAction::Stats(Stat::WeightChart),
        "stats:last_training" => return CallbackAction::Stats(Stat::LastTraining),
        "goal:set_weight" => return CallbackAction::Goal(GoalAction::SetWeight),
        "goal:set_training" => return CallbackAction::Goal(GoalAction::SetTraining),
        "goal:my_goals" => return CallbackAction::Goal(GoalAction::MyGoals),
        _ => {}
    }

    if let Some(kg_str) = data.strip_prefix("weight:")
        && let Ok(kg) = kg_str.parse::<f64>()
    {
        return CallbackAction::Weight(WeightValue::Kg(kg));
    }

    if let Some(desc) = data.strip_prefix("training:") {
        return CallbackAction::Training(TrainingValue::Preset(desc.to_string()));
    }

    CallbackAction::Unknown
}

// ── Helpers ─────────────────────────────────────────────────────────────────

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
