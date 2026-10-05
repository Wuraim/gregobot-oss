use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::datetime::parse_sqlite_datetime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceKind {
    Allergy,
    Dislike,
    Like,
    Habit,
}

impl PreferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allergy => "allergy",
            Self::Dislike => "dislike",
            Self::Like => "like",
            Self::Habit => "habit",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "allergy" => Ok(Self::Allergy),
            "dislike" => Ok(Self::Dislike),
            "like" => Ok(Self::Like),
            "habit" => Ok(Self::Habit),
            other => Err(anyhow!("unknown preference kind: {other}")),
        }
    }

    pub fn is_hard_block(self) -> bool {
        matches!(self, Self::Allergy | Self::Dislike)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodPreference {
    pub id: i64,
    pub user_id: i64,
    pub kind: PreferenceKind,
    pub text: String,
    pub food_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FoodPreference {
    pub fn embed_text(&self) -> String {
        format!("{}: {}", self.kind.as_str(), self.text)
    }
}

#[derive(Debug, Clone)]
pub struct NewFoodPreference {
    pub user_id: i64,
    pub kind: PreferenceKind,
    pub text: String,
    pub food_id: Option<i64>,
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>> {
    parse_sqlite_datetime(s).ok_or_else(|| anyhow!("invalid datetime: {s}"))
}

fn row_to_pref(
    id: i64,
    user_id: i64,
    kind: String,
    text: String,
    food_id: Option<i64>,
    created_at: String,
    updated_at: String,
) -> Result<FoodPreference> {
    Ok(FoodPreference {
        id,
        user_id,
        kind: PreferenceKind::parse(&kind)?,
        text,
        food_id,
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

pub async fn insert(pool: &SqlitePool, pref: NewFoodPreference) -> Result<FoodPreference> {
    let text = pref.text.trim();
    if text.is_empty() {
        return Err(anyhow!("preference text is required"));
    }
    let kind = pref.kind.as_str();
    let id = sqlx::query!(
        r#"
        INSERT INTO food_preferences (user_id, kind, text, food_id)
        VALUES (?, ?, ?, ?)
        "#,
        pref.user_id,
        kind,
        text,
        pref.food_id,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    get_for_user(pool, pref.user_id, id)
        .await?
        .ok_or_else(|| anyhow!("preference {id} missing after insert"))
}

pub async fn get_for_user(
    pool: &SqlitePool,
    user_id: i64,
    preference_id: i64,
) -> Result<Option<FoodPreference>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, kind, text, food_id, created_at, updated_at
        FROM food_preferences
        WHERE id = ? AND user_id = ?
        "#,
        preference_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(row_to_pref(
            r.id,
            r.user_id,
            r.kind,
            r.text,
            r.food_id,
            r.created_at,
            r.updated_at,
        )?)),
    }
}

pub async fn list_for_user(pool: &SqlitePool, user_id: i64) -> Result<Vec<FoodPreference>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, kind, text, food_id, created_at, updated_at
        FROM food_preferences
        WHERE user_id = ?
        ORDER BY kind ASC, id ASC
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_pref(
                r.id.ok_or_else(|| anyhow!("preference missing id"))?,
                r.user_id,
                r.kind,
                r.text,
                r.food_id,
                r.created_at,
                r.updated_at,
            )
        })
        .collect()
}

#[allow(dead_code)]
pub async fn delete(pool: &SqlitePool, user_id: i64, preference_id: i64) -> Result<bool> {
    let res = sqlx::query!(
        "DELETE FROM food_preferences WHERE id = ? AND user_id = ?",
        preference_id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PreferenceEmbedding {
    pub preference_id: i64,
    pub embedding: String,
}

pub async fn list_embeddings(pool: &SqlitePool, user_id: i64) -> Result<Vec<PreferenceEmbedding>> {
    sqlx::query_as::<_, PreferenceEmbedding>(
        r#"
        SELECT pe.preference_id, pe.embedding
        FROM food_preference_embeddings pe
        JOIN food_preferences p ON p.id = pe.preference_id
        WHERE p.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("failed to list food preference embeddings")
}

pub async fn upsert_embedding(
    pool: &SqlitePool,
    preference_id: i64,
    embed_text: &str,
    embedding_json: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO food_preference_embeddings (preference_id, embed_text, embedding)
        VALUES (?, ?, ?)
        ON CONFLICT(preference_id) DO UPDATE SET
            embed_text = excluded.embed_text,
            embedding = excluded.embedding,
            created_at = datetime('now')
        "#,
    )
    .bind(preference_id)
    .bind(embed_text)
    .bind(embedding_json)
    .execute(pool)
    .await
    .context("failed to upsert food preference embedding")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::foods::{FoodSource, NewFood, QuantityUnit, insert as insert_food};
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};

    async fn user(pool: &SqlitePool, tg: i64) -> i64 {
        insert_user(pool, tg, Role::Sportive, Some("tester".into()), None, None)
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn insert_list_delete_preference() {
        let (pool, path) = test_db().await;
        let user_id = user(&pool, 89101).await;

        let food = insert_food(
            &pool,
            NewFood {
                user_id,
                name: "Tuna".into(),
                brand: None,
                barcode: None,
                notes: None,
                kcal_per_100: 116.0,
                protein_g_per_100: 26.0,
                carbs_g_per_100: 0.0,
                fat_g_per_100: 1.0,
                serving_qty: 100.0,
                serving_unit: QuantityUnit::G,
                serving_label: None,
                source: FoodSource::Manual,
            },
        )
        .await
        .unwrap();

        let pref = insert(
            &pool,
            NewFoodPreference {
                user_id,
                kind: PreferenceKind::Dislike,
                text: "tuna".into(),
                food_id: Some(food.id),
            },
        )
        .await
        .unwrap();
        assert_eq!(pref.kind, PreferenceKind::Dislike);
        assert_eq!(pref.food_id, Some(food.id));
        assert_eq!(pref.embed_text(), "dislike: tuna");

        let listed = list_for_user(&pool, user_id).await.unwrap();
        assert_eq!(listed.len(), 1);

        assert!(delete(&pool, user_id, pref.id).await.unwrap());
        assert!(
            get_for_user(&pool, user_id, pref.id)
                .await
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_kind_rejects_unknown() {
        assert!(PreferenceKind::parse("maybe").is_err());
        assert_eq!(
            PreferenceKind::parse("ALLERGY").unwrap(),
            PreferenceKind::Allergy
        );
        assert!(PreferenceKind::Allergy.is_hard_block());
        assert!(!PreferenceKind::Like.is_hard_block());
    }
}
