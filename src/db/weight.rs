use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightEntry {
    pub id: i64,
    pub user_id: i64,
    pub weight_kg: f64,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewWeightEntry {
    pub user_id: i64,
    pub weight_kg: f64,
    pub recorded_at: Option<DateTime<Utc>>,
}

pub async fn insert(pool: &SqlitePool, entry: NewWeightEntry) -> Result<WeightEntry> {
    let recorded_at = entry.recorded_at.unwrap_or_else(Utc::now);
    let recorded_at_str = recorded_at.to_rfc3339();

    let id = sqlx::query!(
        "INSERT INTO weight_entries (user_id, weight_kg, recorded_at) VALUES (?, ?, ?)",
        entry.user_id,
        entry.weight_kg,
        recorded_at_str,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok(WeightEntry {
        id,
        user_id: entry.user_id,
        weight_kg: entry.weight_kg,
        recorded_at,
    })
}

pub async fn list_for_user(pool: &SqlitePool, user_id: i64) -> Result<Vec<WeightEntry>> {
    let rows = sqlx::query!(
        "SELECT id, user_id, weight_kg, recorded_at FROM weight_entries WHERE user_id = ? ORDER BY recorded_at DESC",
        user_id,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(WeightEntry {
                id: r.id,
                user_id: r.user_id,
                weight_kg: r.weight_kg,
                recorded_at: r.recorded_at.parse::<DateTime<Utc>>()?,
            })
        })
        .collect()
}

pub async fn latest_for_user(pool: &SqlitePool, user_id: i64) -> Result<Option<WeightEntry>> {
    let row = sqlx::query!(
        "SELECT id, user_id, weight_kg, recorded_at FROM weight_entries WHERE user_id = ? ORDER BY recorded_at DESC LIMIT 1",
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(WeightEntry {
            id: r.id,
            user_id: r.user_id,
            weight_kg: r.weight_kg,
            recorded_at: r.recorded_at.parse::<DateTime<Utc>>()?,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use chrono::Utc;

    #[tokio::test]
    async fn insert_and_list_weight() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99201,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let entry = insert(
            &pool,
            NewWeightEntry {
                user_id: user.id,
                weight_kg: 78.5,
                recorded_at: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.weight_kg, 78.5);
        assert_eq!(entry.user_id, user.id);

        let list = list_for_user(&pool, user.id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].weight_kg, 78.5);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn latest_weight() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99202,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let older = Utc::now() - chrono::Duration::days(2);
        let newer = Utc::now() - chrono::Duration::hours(1);
        insert(
            &pool,
            NewWeightEntry {
                user_id: user.id,
                weight_kg: 80.0,
                recorded_at: Some(older),
            },
        )
        .await
        .unwrap();
        insert(
            &pool,
            NewWeightEntry {
                user_id: user.id,
                weight_kg: 79.0,
                recorded_at: Some(newer),
            },
        )
        .await
        .unwrap();

        let latest = latest_for_user(&pool, user.id).await.unwrap().unwrap();
        assert_eq!(latest.weight_kg, 79.0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn latest_weight_empty() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99203,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(latest_for_user(&pool, user.id).await.unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
