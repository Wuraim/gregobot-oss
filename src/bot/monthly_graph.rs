use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, Months, TimeZone, Utc};
use teloxide::prelude::*;
use teloxide::types::InputFile;
use tracing::Instrument;

use crate::bot::training_graph;
use crate::db::Db;

const CHECK_INTERVAL_SECS: u64 = 900; // 15 minutes
const MIN_WEEKS: u32 = 1;
const MAX_WEEKS: u32 = 52;
const MIN_MONTHS: u32 = 1;
const MAX_MONTHS: u32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphWindow {
    CurrentMonth,
    PreviousMonth,
    /// Rolling window of the last `n` weeks ending now (UTC).
    LastWeeks(u32),
    /// Rolling window of the last `n` months ending now (UTC).
    LastMonths(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollingPeriod {
    Weeks(u32),
    Months(u32),
}

/// Parse a rolling window arg like `4w` / `3m`. Returns `None` if not a duration form.
pub fn parse_rolling_arg(arg: &str) -> Result<Option<RollingPeriod>, ()> {
    let arg = arg.trim().to_lowercase();
    if arg.is_empty() {
        return Ok(None);
    }
    let (num, unit) = match arg.chars().last() {
        Some('w') | Some('m') => {
            let unit = arg.chars().last().unwrap();
            let num = &arg[..arg.len() - 1];
            (num, unit)
        }
        _ => return Ok(None),
    };
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return Err(());
    }
    let n: u32 = num.parse().map_err(|_| ())?;
    match unit {
        'w' if (MIN_WEEKS..=MAX_WEEKS).contains(&n) => Ok(Some(RollingPeriod::Weeks(n))),
        'm' if (MIN_MONTHS..=MAX_MONTHS).contains(&n) => Ok(Some(RollingPeriod::Months(n))),
        'w' | 'm' => Err(()),
        _ => Ok(None),
    }
}

pub fn spawn(pool: Db, bot_token: String) {
    tokio::spawn(async move {
        tracing::info!(
            "Monthly training graph scheduler started (interval={CHECK_INTERVAL_SECS}s)"
        );

        loop {
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;

            let run_span = tracing::info_span!("monthly_training_graph_check");
            async {
                if !is_last_day_of_month_utc() {
                    return;
                }

                let chats =
                    match crate::db::chat_settings::list_monthly_training_graph_due(&pool).await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("Monthly graph: failed to list due chats: {e}");
                            return;
                        }
                    };

                let bot = Bot::new(&bot_token);
                for chat_id in chats {
                    let chat_span =
                        tracing::info_span!("monthly_training_graph_chat", chat_id = chat_id);
                    async {
                        if let Err(e) = send_monthly_graph(
                            &bot,
                            &pool,
                            chat_id,
                            GraphWindow::CurrentMonth,
                            true,
                        )
                        .await
                        {
                            tracing::error!(
                                chat_id = chat_id,
                                "Monthly graph: failed to send: {e:#}"
                            );
                        }
                    }
                    .instrument(chat_span)
                    .await;
                }
            }
            .instrument(run_span)
            .await;
        }
    });
}

fn is_last_day_of_month_utc() -> bool {
    let today = Utc::now().date_naive();
    let tomorrow = today + ChronoDuration::days(1);
    tomorrow.month() != today.month()
}

fn month_window_for(
    year: i32,
    month: u32,
) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let now = Utc::now();
    let start = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    let end = if month == 12 {
        Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
            .single()
            .unwrap_or(now)
    } else {
        Utc.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0)
            .single()
            .unwrap_or(now)
    };
    // Lowercase month name — looks cleaner on the chart than "July 2026".
    let title = start.format("%B %Y").to_string().to_lowercase();
    (start, end, title)
}

/// Current calendar month window in UTC: `[first_day, first_day_next)`.
pub fn current_month_window() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let now = Utc::now();
    month_window_for(now.year(), now.month())
}

/// Previous calendar month window in UTC: `[first_of_prev, first_of_this)`.
pub fn previous_month_window() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let now = Utc::now();
    let first_this = Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    let prev = first_this - ChronoDuration::days(1);
    month_window_for(prev.year(), prev.month())
}

/// Rolling last-N-weeks window in UTC: `[now - N weeks, now)`.
pub fn last_weeks_window(weeks: u32) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let end = Utc::now();
    let start = end - ChronoDuration::weeks(weeks as i64);
    let title = if weeks == 1 {
        "last 1 week".to_string()
    } else {
        format!("last {weeks} weeks")
    };
    (start, end, title)
}

/// Rolling last-N-months window in UTC: `[now - N months, now)`.
pub fn last_months_window(months: u32) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    let end = Utc::now();
    let start = end
        .checked_sub_months(Months::new(months))
        .unwrap_or_else(|| end - ChronoDuration::days(30 * months as i64));
    let title = if months == 1 {
        "last 1 month".to_string()
    } else {
        format!("last {months} months")
    };
    (start, end, title)
}

fn window_bounds(window: GraphWindow) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>, String) {
    match window {
        GraphWindow::CurrentMonth => current_month_window(),
        GraphWindow::PreviousMonth => previous_month_window(),
        GraphWindow::LastWeeks(n) => last_weeks_window(n),
        GraphWindow::LastMonths(n) => last_months_window(n),
    }
}

/// Send the group training graph for a chat window. When `mark_sent` is true, update
/// the idempotency marker (scheduler path).
pub async fn send_monthly_graph(
    bot: &Bot,
    pool: &Db,
    chat_id: i64,
    window: GraphWindow,
    mark_sent: bool,
) -> anyhow::Result<()> {
    let (start, end, period_label) = window_bounds(window);
    let start_s = start.to_rfc3339();
    let end_s = end.to_rfc3339();

    let stats =
        crate::db::training::sessions_by_group_member(pool, chat_id, &start_s, &end_s).await?;

    let tg_chat = ChatId(chat_id);
    if stats.is_empty() {
        bot.send_message(
            tg_chat,
            format!("No training sessions recorded for {period_label}."),
        )
        .await?;
        if mark_sent {
            crate::db::chat_settings::mark_monthly_training_graph_sent(pool, chat_id).await?;
        }
        return Ok(());
    }

    let title = format!("training — {period_label}");
    let png = training_graph::render_sessions_chart(&title, &stats)?;

    let mut caption_lines = vec![format!("training ranking — {period_label}")];
    for (i, s) in stats.iter().enumerate() {
        let label = s.display_name();
        caption_lines.push(format!("{}. {} — {} session(s)", i + 1, label, s.sessions));
    }
    let caption = caption_lines.join("\n");

    bot.send_photo(tg_chat, InputFile::memory(png).file_name("training.png"))
        .caption(caption)
        .await?;

    if mark_sent {
        crate::db::chat_settings::mark_monthly_training_graph_sent(pool, chat_id).await?;
    }

    tracing::info!(chat_id = chat_id, period = %period_label, "Group training graph sent");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rolling_arg_weeks_ok() {
        assert_eq!(
            parse_rolling_arg("4w").unwrap(),
            Some(RollingPeriod::Weeks(4))
        );
        assert_eq!(
            parse_rolling_arg("1W").unwrap(),
            Some(RollingPeriod::Weeks(1))
        );
        assert_eq!(
            parse_rolling_arg("52w").unwrap(),
            Some(RollingPeriod::Weeks(52))
        );
    }

    #[test]
    fn parse_rolling_arg_months_ok() {
        assert_eq!(
            parse_rolling_arg("3m").unwrap(),
            Some(RollingPeriod::Months(3))
        );
        assert_eq!(
            parse_rolling_arg("1M").unwrap(),
            Some(RollingPeriod::Months(1))
        );
        assert_eq!(
            parse_rolling_arg("24m").unwrap(),
            Some(RollingPeriod::Months(24))
        );
        assert_eq!(parse_rolling_arg("last").unwrap(), None);
        assert_eq!(parse_rolling_arg("").unwrap(), None);
    }

    #[test]
    fn parse_rolling_arg_rejects_invalid() {
        assert!(parse_rolling_arg("0w").is_err());
        assert!(parse_rolling_arg("53w").is_err());
        assert!(parse_rolling_arg("0m").is_err());
        assert!(parse_rolling_arg("25m").is_err());
        assert!(parse_rolling_arg("w").is_err());
        assert!(parse_rolling_arg("m").is_err());
        assert!(parse_rolling_arg("-2w").is_err());
        // Not a duration form — handled like other unknown args.
        assert_eq!(parse_rolling_arg("4weeks").unwrap(), None);
        assert_eq!(parse_rolling_arg("2months").unwrap(), None);
    }

    #[test]
    fn last_weeks_window_spans_requested_days() {
        let (start, end, label) = last_weeks_window(4);
        let delta = end - start;
        assert_eq!(delta.num_weeks(), 4);
        assert_eq!(label, "last 4 weeks");
    }

    #[test]
    fn last_months_window_label() {
        let (_start, _end, label) = last_months_window(3);
        assert_eq!(label, "last 3 months");
        let (_start, _end, label1) = last_months_window(1);
        assert_eq!(label1, "last 1 month");
    }
}
