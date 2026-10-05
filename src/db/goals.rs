use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalType {
    Weight,
    TrainingFrequency,
}

impl GoalType {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalType::Weight => "weight",
            GoalType::TrainingFrequency => "training_frequency",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "weight" => Some(GoalType::Weight),
            "training_frequency" | "training" => Some(GoalType::TrainingFrequency),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Goal {
    pub id: i64,
    pub user_id: i64,
    pub goal_type: String,
    pub target_value: f64,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub achieved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct NewGoal {
    pub user_id: i64,
    pub goal_type: GoalType,
    pub target_value: f64,
}

/// Parse a datetime string that may be in either RFC 3339 ("2026-06-09T15:23:01Z")
/// or SQLite datetime('now') format ("2026-06-09 15:23:01").
fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    // Fallback: SQLite's datetime('now') format "YYYY-MM-DD HH:MM:SS"
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|ndt| ndt.and_utc())
        .map_err(|e| anyhow::anyhow!("Failed to parse datetime '{}': {}", s, e))
}

fn row_to_goal(
    id: i64,
    user_id: i64,
    goal_type: String,
    target_value: f64,
    is_active: i64,
    created_at: String,
    achieved_at: Option<String>,
) -> Result<Goal> {
    Ok(Goal {
        id,
        user_id,
        goal_type,
        target_value,
        is_active: is_active != 0,
        created_at: parse_datetime(&created_at)?,
        achieved_at: match achieved_at {
            Some(s) => Some(parse_datetime(&s)?),
            None => None,
        },
    })
}

pub async fn set_goal(pool: &SqlitePool, new: NewGoal) -> Result<Goal> {
    let goal_type_str = new.goal_type.as_str();
    let id = sqlx::query!(
        r#"
        INSERT INTO goals (user_id, goal_type, target_value)
        VALUES (?, ?, ?)
        "#,
        new.user_id,
        goal_type_str,
        new.target_value,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    let row = sqlx::query!(
        r#"
        SELECT id, user_id, goal_type, target_value, is_active, created_at, achieved_at
        FROM goals WHERE id = ?
        "#,
        id,
    )
    .fetch_one(pool)
    .await?;

    row_to_goal(
        row.id,
        row.user_id,
        row.goal_type,
        row.target_value,
        row.is_active,
        row.created_at,
        row.achieved_at,
    )
}

pub async fn list_active(pool: &SqlitePool, user_id: i64) -> Result<Vec<Goal>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, goal_type, target_value, is_active, created_at, achieved_at
        FROM goals
        WHERE user_id = ? AND is_active = 1 AND achieved_at IS NULL
        ORDER BY created_at DESC
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_goal(
                r.id,
                r.user_id,
                r.goal_type,
                r.target_value,
                r.is_active,
                r.created_at,
                r.achieved_at,
            )
        })
        .collect()
}

pub async fn deactivate(pool: &SqlitePool, goal_id: i64, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "UPDATE goals SET is_active = 0 WHERE id = ? AND user_id = ?",
        goal_id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

fn start_of_iso_week(now: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::Datelike;
    let weekday = now.weekday().num_days_from_monday() as i64;
    let date = now.date_naive();
    let start_date = date - chrono::Duration::days(weekday);
    start_date.and_hms_opt(0, 0, 0).unwrap().and_utc()
}

fn parse_entry_date(s: &str) -> Option<DateTime<Utc>> {
    s.parse::<DateTime<Utc>>().ok()
}

/// Training sessions in the current ISO week (Monday 00:00 UTC → now).
pub async fn count_training_this_week(pool: &SqlitePool, user_id: i64) -> Result<f64> {
    let now = Utc::now();
    let week_start = start_of_iso_week(now);
    let week_start_str = week_start.to_rfc3339();

    let training_rows = sqlx::query!(
        "SELECT recorded_at FROM training_entries WHERE user_id = ? AND recorded_at >= ?",
        user_id,
        week_start_str,
    )
    .fetch_all(pool)
    .await?;

    let count = training_rows
        .iter()
        .filter(|r| {
            parse_entry_date(&r.recorded_at)
                .map(|d| d >= week_start)
                .unwrap_or(false)
        })
        .count();
    Ok(count as f64)
}

/// Current measured value for a goal type, if any data exists.
pub async fn current_value_for(
    pool: &SqlitePool,
    user_id: i64,
    goal_type: GoalType,
) -> Result<Option<f64>> {
    match goal_type {
        GoalType::Weight => Ok(crate::db::weight::latest_for_user(pool, user_id)
            .await?
            .map(|e| e.weight_kg)),
        GoalType::TrainingFrequency => Ok(Some(count_training_this_week(pool, user_id).await?)),
    }
}

/// Check all active goals for a user and auto-achieve any that are met.
/// Returns the list of newly achieved goals.
pub async fn check_achievements(pool: &SqlitePool, user_id: i64) -> Result<Vec<Goal>> {
    let active = list_active(pool, user_id).await?;
    if active.is_empty() {
        return Ok(vec![]);
    }

    let latest_weight = current_value_for(pool, user_id, GoalType::Weight).await?;
    let sessions_this_week = count_training_this_week(pool, user_id).await?;
    let now = Utc::now();

    let mut newly_achieved = vec![];
    for goal in active {
        let met = match GoalType::from_str(&goal.goal_type) {
            Some(GoalType::Weight) => latest_weight
                .map(|w| w <= goal.target_value)
                .unwrap_or(false),
            Some(GoalType::TrainingFrequency) => sessions_this_week >= goal.target_value,
            None => false,
        };

        if met {
            let achieved_at = now.to_rfc3339();
            sqlx::query!(
                "UPDATE goals SET achieved_at = ? WHERE id = ?",
                achieved_at,
                goal.id,
            )
            .execute(pool)
            .await?;

            newly_achieved.push(Goal {
                achieved_at: Some(now),
                ..goal
            });
        }
    }

    Ok(newly_achieved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_type_round_trip() {
        assert_eq!(GoalType::from_str("weight"), Some(GoalType::Weight));
        assert_eq!(
            GoalType::from_str("training_frequency"),
            Some(GoalType::TrainingFrequency)
        );
        assert_eq!(
            GoalType::from_str("training"),
            Some(GoalType::TrainingFrequency)
        );
        assert_eq!(GoalType::from_str("unknown"), None);
    }

    #[test]
    fn goal_type_as_str() {
        assert_eq!(GoalType::Weight.as_str(), "weight");
        assert_eq!(GoalType::TrainingFrequency.as_str(), "training_frequency");
    }

    #[tokio::test]
    async fn set_and_list_active_goals() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99101,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();

        let created = set_goal(
            &pool,
            NewGoal {
                user_id: user.id,
                goal_type: GoalType::Weight,
                target_value: 75.0,
            },
        )
        .await
        .unwrap();
        assert_eq!(created.goal_type, "weight");
        assert_eq!(created.target_value, 75.0);
        assert!(created.is_active);

        let list = list_active(&pool, user.id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].target_value, 75.0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn deactivate_goal() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};

        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99102,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let goal = set_goal(
            &pool,
            NewGoal {
                user_id: user.id,
                goal_type: GoalType::TrainingFrequency,
                target_value: 3.0,
            },
        )
        .await
        .unwrap();

        assert!(deactivate(&pool, goal.id, user.id).await.unwrap());
        let list = list_active(&pool, user.id).await.unwrap();
        assert!(list.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn current_value_for_weight_and_training() {
        use crate::db::tests::test_db;
        use crate::db::training::{NewTrainingEntry, insert as insert_training};
        use crate::db::users::{Role, insert as insert_user};
        use crate::db::weight::{NewWeightEntry, insert as insert_weight};

        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99103,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(
            current_value_for(&pool, user.id, GoalType::Weight)
                .await
                .unwrap(),
            None
        );
        insert_weight(
            &pool,
            NewWeightEntry {
                user_id: user.id,
                weight_kg: 80.0,
                recorded_at: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            current_value_for(&pool, user.id, GoalType::Weight)
                .await
                .unwrap(),
            Some(80.0)
        );

        assert_eq!(
            current_value_for(&pool, user.id, GoalType::TrainingFrequency)
                .await
                .unwrap(),
            Some(0.0)
        );
        insert_training(&pool, NewTrainingEntry::simple(user.id, "Run".into(), None))
            .await
            .unwrap();
        assert_eq!(
            current_value_for(&pool, user.id, GoalType::TrainingFrequency)
                .await
                .unwrap(),
            Some(1.0)
        );
        let _ = std::fs::remove_file(&path);
    }
}
