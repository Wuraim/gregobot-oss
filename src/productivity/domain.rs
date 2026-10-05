use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

pub use crate::db::tasks::{TaskPriority, TaskSource, TaskStatus};

// ── Task ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub due_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub source: TaskSource,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ── Reminder ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub user_id: i64,
    pub target: ReminderTarget,
    pub scheduled_at: DateTime<Utc>,
    pub status: ReminderStatus,
    pub created_at: DateTime<Utc>,
    pub fired_at: Option<DateTime<Utc>>,
    pub attempts: u32,
    pub recurrence: ReminderRecurrence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReminderTarget {
    Task { task_id: i64 },
    Custom { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderStatus {
    Pending,
    Fired,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderRecurrence {
    None,
    Daily,
    Weekly,
    Monthly,
}

impl ReminderRecurrence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Self::None),
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }

    /// Compute the next occurrence of this recurrence after the given datetime.
    /// Monthly is clamped to the last valid day of the target month.
    pub fn next_occurrence(self, current: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::None => current,
            Self::Daily => current + chrono::Duration::days(1),
            Self::Weekly => current + chrono::Duration::weeks(1),
            Self::Monthly => {
                let naive = current.date_naive();
                let day = naive.day();
                let next_month = if naive.month() == 12 {
                    naive
                        .with_year(naive.year() + 1)
                        .unwrap()
                        .with_month(1)
                        .unwrap()
                } else {
                    naive.with_month(naive.month() + 1).unwrap()
                };
                let clamped_day = day.min(next_month.num_days_in_month() as u32);
                let next_naive = next_month.with_day(clamped_day).unwrap_or(next_month);
                next_naive.and_time(current.time()).and_utc()
            }
        }
    }
}

impl ReminderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fired => "fired",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "fired" => Some(Self::Fired),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

// ── Conversion helpers between raw DB types and domain types ──────────────

/// Parse an RFC3339 string from the DB into DateTime<Utc>.
/// Tolerant of both standard RFC3339 and SQLite datetime('now') format.
pub fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|naive| naive.and_utc())
        })
}

pub fn parse_task(row: &crate::db::tasks::Task) -> Task {
    Task {
        id: row.id,
        user_id: row.user_id,
        title: row.title.clone(),
        description: row.description.clone(),
        status: TaskStatus::from_str(&row.status).unwrap_or(TaskStatus::Todo),
        priority: TaskPriority::from_str(&row.priority).unwrap_or(TaskPriority::Medium),
        due_at: row.due_at.as_deref().and_then(parse_datetime),
        tags: row
            .tags
            .split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        source: TaskSource::from_str(&row.source).unwrap_or(TaskSource::Manual),
        created_at: parse_datetime(&row.created_at).unwrap_or_else(Utc::now),
        completed_at: row.completed_at.as_deref().and_then(parse_datetime),
    }
}

pub fn parse_reminder(row: &crate::db::reminders::Reminder) -> Reminder {
    let target = match row.target_type.as_str() {
        "task" => ReminderTarget::Task {
            task_id: row.target_id.unwrap_or(0),
        },
        "custom" => ReminderTarget::Custom {
            message: row.custom_message.clone().unwrap_or_default(),
        },
        // Legacy "event" rows are cancelled by migration 0029; map to a stub message.
        _ => ReminderTarget::Custom {
            message: row
                .custom_message
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        },
    };
    Reminder {
        id: row.id,
        user_id: row.user_id,
        target,
        scheduled_at: parse_datetime(&row.scheduled_at).unwrap_or_else(Utc::now),
        status: ReminderStatus::from_str(&row.status).unwrap_or(ReminderStatus::Pending),
        created_at: parse_datetime(&row.created_at).unwrap_or_else(Utc::now),
        fired_at: row.fired_at.as_deref().and_then(parse_datetime),
        attempts: row.attempts as u32,
        recurrence: ReminderRecurrence::from_str(&row.recurrence)
            .unwrap_or(ReminderRecurrence::None),
    }
}

pub fn parse_reminders(rows: &[crate::db::reminders::Reminder]) -> Vec<Reminder> {
    rows.iter().map(parse_reminder).collect()
}

pub fn parse_tasks(rows: &[crate::db::tasks::Task]) -> Vec<Task> {
    rows.iter().map(parse_task).collect()
}

// ── Feature Proposal ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalWithAuthor {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub author_username: Option<String>,
}

pub fn parse_proposal(row: &crate::db::proposals::Proposal) -> Proposal {
    Proposal {
        id: row.id,
        user_id: row.user_id,
        title: row.title.clone(),
        description: row.description.clone(),
        created_at: parse_datetime(&row.created_at).unwrap_or_else(Utc::now),
        updated_at: parse_datetime(&row.updated_at).unwrap_or_else(Utc::now),
    }
}

pub fn parse_proposals(rows: &[crate::db::proposals::Proposal]) -> Vec<Proposal> {
    rows.iter().map(parse_proposal).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn reminder_recurrence_round_trip() {
        for variant in [
            ReminderRecurrence::None,
            ReminderRecurrence::Daily,
            ReminderRecurrence::Weekly,
            ReminderRecurrence::Monthly,
        ] {
            assert_eq!(
                ReminderRecurrence::from_str(variant.as_str()),
                Some(variant)
            );
        }
        assert_eq!(ReminderRecurrence::from_str("yearly"), None);
    }

    #[test]
    fn reminder_recurrence_next_occurrence() {
        let base = Utc::now();
        assert_eq!(ReminderRecurrence::None.next_occurrence(base), base);
        assert_eq!(
            ReminderRecurrence::Daily.next_occurrence(base),
            base + chrono::Duration::days(1)
        );
        assert_eq!(
            ReminderRecurrence::Weekly.next_occurrence(base),
            base + chrono::Duration::weeks(1)
        );
    }

    #[test]
    fn reminder_status_round_trip() {
        for variant in [
            ReminderStatus::Pending,
            ReminderStatus::Fired,
            ReminderStatus::Cancelled,
            ReminderStatus::Failed,
        ] {
            assert_eq!(ReminderStatus::from_str(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn parse_datetime_rfc3339() {
        let dt = parse_datetime("2024-01-15T08:30:00Z").unwrap();
        assert_eq!(dt.timestamp(), 1705307400);
    }

    #[test]
    fn parse_datetime_sqlite() {
        let dt = parse_datetime("2024-01-15 08:30:00").unwrap();
        assert_eq!(dt.timestamp(), 1705307400);
    }

    #[test]
    fn parse_datetime_invalid() {
        assert!(parse_datetime("not a date").is_none());
    }

    #[test]
    fn parse_task_maps_defaults() {
        let row = crate::db::tasks::Task {
            id: 1,
            user_id: 2,
            title: "t".into(),
            description: None,
            status: "unknown".into(),
            priority: "unknown".into(),
            due_at: None,
            tags: "".into(),
            source: "unknown".into(),
            created_at: "2024-01-15T08:30:00Z".into(),
            completed_at: None,
        };
        let task = parse_task(&row);
        assert_eq!(task.status, TaskStatus::Todo);
        assert_eq!(task.priority, TaskPriority::Medium);
        assert_eq!(task.source, TaskSource::Manual);
        assert!(task.tags.is_empty());
    }
}
