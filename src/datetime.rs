use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

/// Parse a flexible datetime string relative to the user's timezone.
///
/// Supports:
/// - RFC3339 timestamps (with or without trailing `Z`)
/// - Relative French expressions: "hier", "avant-hier", "aujourd'hui",
///   "ce matin", "cet après-midi", "ce soir"
/// - Relative English expressions: "yesterday", "today", "this morning",
///   "this afternoon", "this evening"
///
/// Returns `None` if the input cannot be parsed.
pub fn parse_flexible_datetime(input: &str, tz: &Tz, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let normalized = input.trim().to_lowercase();

    // 1. Try strict RFC3339 first.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(input.trim()) {
        return Some(dt.with_timezone(&Utc));
    }
    let with_tz = format!("{}+00:00", input.trim_end_matches('Z'));
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&with_tz) {
        return Some(dt.with_timezone(&Utc));
    }

    // 2. Resolve relative dates in the user's timezone.
    let tz_now = now.with_timezone(tz);
    let today = tz_now.date_naive();

    let target_date = match normalized.as_str() {
        "hier" | "yesterday" => today - Duration::days(1),
        "avant-hier" | "avant hier" | "day before yesterday" => today - Duration::days(2),
        "aujourd'hui" | "today" => today,
        _ => {
            // Try to parse as a plain date (YYYY-MM-DD).
            if let Ok(date) = chrono::NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d") {
                let naive = date.and_time(NaiveTime::from_hms_opt(12, 0, 0)?);
                return tz
                    .from_local_datetime(&naive)
                    .single()
                    .map(|d| d.with_timezone(&Utc));
            }
            return None;
        }
    };

    let time = match normalized.as_str() {
        "ce matin" | "this morning" => NaiveTime::from_hms_opt(8, 0, 0),
        "cet après-midi" | "cet apres-midi" | "this afternoon" => {
            NaiveTime::from_hms_opt(14, 0, 0)
        }
        "ce soir" | "this evening" => NaiveTime::from_hms_opt(20, 0, 0),
        _ => NaiveTime::from_hms_opt(tz_now.hour(), tz_now.minute(), tz_now.second()),
    }?;

    let naive = target_date.and_time(time);
    tz.from_local_datetime(&naive)
        .single()
        .map(|d| d.with_timezone(&Utc))
}

/// Parse RFC3339 or SQLite `datetime('now')` (`YYYY-MM-DD HH:MM:SS`, UTC).
pub fn parse_sqlite_datetime(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|naive| naive.and_utc())
        })
}

/// UTC half-open range `[start, end)` covering `date` in `tz`.
pub fn local_day_bounds(date: NaiveDate, tz: Tz) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start_naive = date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    let end_date = date.succ_opt()?;
    let end_naive = end_date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
    let start = tz
        .from_local_datetime(&start_naive)
        .single()
        .or_else(|| tz.from_local_datetime(&start_naive).earliest())?;
    let end = tz
        .from_local_datetime(&end_naive)
        .single()
        .or_else(|| tz.from_local_datetime(&end_naive).earliest())?;
    Some((start.with_timezone(&Utc), end.with_timezone(&Utc)))
}

/// Label a past instant relative to `now` in `tz` (e.g. `today 18:30`, `yesterday 09:12`, `2026-08-10 18:30`).
pub fn format_relative_when(at: DateTime<Utc>, tz: &Tz, now: DateTime<Utc>) -> String {
    let local = at.with_timezone(tz);
    let today = now.with_timezone(tz).date_naive();
    let date = local.date_naive();
    let time = local.format("%H:%M");
    if date == today {
        format!("today {time}")
    } else if date == today - Duration::days(1) {
        format!("yesterday {time}")
    } else {
        format!("{} {time}", local.format("%Y-%m-%d"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Paris;

    #[test]
    fn test_rfc3339() {
        let tz = Paris;
        let now = Utc::now();
        let dt = parse_flexible_datetime("2024-01-15T14:00:00Z", &tz, now);
        assert!(dt.is_some());
    }

    #[test]
    fn test_hier() {
        let tz = Paris;
        let now = Utc::now();
        let dt = parse_flexible_datetime("hier", &tz, now).unwrap();
        let expected_date = now.with_timezone(&tz).date_naive() - Duration::days(1);
        assert_eq!(dt.with_timezone(&tz).date_naive(), expected_date);
    }

    #[test]
    fn test_hier_uppercase() {
        let tz = Paris;
        let now = Utc::now();
        let dt = parse_flexible_datetime("Hier", &tz, now).unwrap();
        let expected_date = now.with_timezone(&tz).date_naive() - Duration::days(1);
        assert_eq!(dt.with_timezone(&tz).date_naive(), expected_date);
    }

    #[test]
    fn parse_sqlite_datetime_formats() {
        let sqlite = parse_sqlite_datetime("2026-08-10 18:30:00").unwrap();
        assert_eq!(sqlite.to_rfc3339(), "2026-08-10T18:30:00+00:00");
        let rfc = parse_sqlite_datetime("2026-08-10T18:30:00Z").unwrap();
        assert_eq!(rfc, sqlite);
        assert!(parse_sqlite_datetime("nope").is_none());
    }

    #[test]
    fn format_relative_when_today_yesterday_and_older() {
        let tz = Paris;
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 20, 0, 0).unwrap();
        let today = Utc.with_ymd_and_hms(2026, 8, 18, 10, 15, 0).unwrap();
        let yesterday = Utc.with_ymd_and_hms(2026, 8, 17, 9, 12, 0).unwrap();
        let older = Utc.with_ymd_and_hms(2026, 8, 10, 18, 30, 0).unwrap();
        assert_eq!(format_relative_when(today, &tz, now), "today 12:15");
        assert_eq!(format_relative_when(yesterday, &tz, now), "yesterday 11:12");
        assert_eq!(format_relative_when(older, &tz, now), "2026-08-10 20:30");
    }

    #[test]
    fn local_day_bounds_paris_summer() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 18).unwrap();
        let (start, end) = local_day_bounds(date, Paris).unwrap();
        assert_eq!(start.to_rfc3339(), "2026-08-17T22:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-08-18T22:00:00+00:00");
    }
}
