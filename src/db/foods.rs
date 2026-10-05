use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::datetime::parse_sqlite_datetime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoodSource {
    Manual,
    Photo,
    Api,
}

impl FoodSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Photo => "photo",
            Self::Api => "api",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "photo" => Self::Photo,
            "api" => Self::Api,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantityUnit {
    G,
    Ml,
    Serving,
    Piece,
}

impl QuantityUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::G => "g",
            Self::Ml => "ml",
            Self::Serving => "serving",
            Self::Piece => "piece",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "g" | "gram" | "grams" => Ok(Self::G),
            "ml" | "milliliter" | "millilitre" => Ok(Self::Ml),
            "serving" | "servings" | "scoop" | "scoops" => Ok(Self::Serving),
            "piece" | "pieces" | "pc" | "pcs" => Ok(Self::Piece),
            other => Err(anyhow!("unknown unit: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Macros {
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
}

impl Macros {
    pub fn zero() -> Self {
        Self {
            kcal: 0.0,
            protein_g: 0.0,
            carbs_g: 0.0,
            fat_g: 0.0,
        }
    }

    pub fn add(&mut self, other: &Self) {
        self.kcal += other.kcal;
        self.protein_g += other.protein_g;
        self.carbs_g += other.carbs_g;
        self.fat_g += other.fat_g;
    }

    pub fn rounded(self) -> Self {
        Self {
            kcal: round1(self.kcal),
            protein_g: round1(self.protein_g),
            carbs_g: round1(self.carbs_g),
            fat_g: round1(self.fat_g),
        }
    }
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Food {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub brand: Option<String>,
    pub barcode: Option<String>,
    pub notes: Option<String>,
    pub kcal_per_100: f64,
    pub protein_g_per_100: f64,
    pub carbs_g_per_100: f64,
    pub fat_g_per_100: f64,
    pub serving_qty: f64,
    pub serving_unit: QuantityUnit,
    pub serving_label: Option<String>,
    pub source: FoodSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Food {
    pub fn embed_text(&self) -> String {
        match self
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(brand) => format!("{brand} {}", self.name),
            None => self.name.clone(),
        }
    }

    /// Scale catalog nutrients (per 100 g/ml) to a logged quantity.
    pub fn scale(&self, quantity: f64, unit: QuantityUnit) -> Result<Macros> {
        if quantity <= 0.0 {
            return Err(anyhow!("quantity must be positive"));
        }
        let grams = match unit {
            QuantityUnit::G | QuantityUnit::Ml => quantity,
            QuantityUnit::Serving | QuantityUnit::Piece => quantity * self.serving_qty,
        };
        let factor = grams / 100.0;
        Ok(Macros {
            kcal: self.kcal_per_100 * factor,
            protein_g: self.protein_g_per_100 * factor,
            carbs_g: self.carbs_g_per_100 * factor,
            fat_g: self.fat_g_per_100 * factor,
        }
        .rounded())
    }
}

#[derive(Debug, Clone)]
pub struct NewFood {
    pub user_id: i64,
    pub name: String,
    pub brand: Option<String>,
    pub barcode: Option<String>,
    pub notes: Option<String>,
    pub kcal_per_100: f64,
    pub protein_g_per_100: f64,
    pub carbs_g_per_100: f64,
    pub fat_g_per_100: f64,
    pub serving_qty: f64,
    pub serving_unit: QuantityUnit,
    pub serving_label: Option<String>,
    pub source: FoodSource,
}

#[derive(Debug, Clone, Default)]
pub struct FoodUpdate {
    pub name: Option<String>,
    pub brand: Option<Option<String>>,
    pub barcode: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub kcal_per_100: Option<f64>,
    pub protein_g_per_100: Option<f64>,
    pub carbs_g_per_100: Option<f64>,
    pub fat_g_per_100: Option<f64>,
    pub serving_qty: Option<f64>,
    pub serving_unit: Option<QuantityUnit>,
    pub serving_label: Option<Option<String>>,
    pub source: Option<FoodSource>,
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>> {
    parse_sqlite_datetime(s).ok_or_else(|| anyhow!("invalid datetime: {s}"))
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn reject_negative_nutrients(kcal: f64, protein: f64, carbs: f64, fat: f64) -> Result<()> {
    if kcal < 0.0 || protein < 0.0 || carbs < 0.0 || fat < 0.0 {
        Err(anyhow!("nutrients cannot be negative"))
    } else {
        Ok(())
    }
}

fn row_to_food(
    id: i64,
    user_id: i64,
    name: String,
    brand: Option<String>,
    barcode: Option<String>,
    notes: Option<String>,
    kcal_per_100: f64,
    protein_g_per_100: f64,
    carbs_g_per_100: f64,
    fat_g_per_100: f64,
    serving_qty: f64,
    serving_unit: String,
    serving_label: Option<String>,
    source: String,
    created_at: String,
    updated_at: String,
) -> Result<Food> {
    Ok(Food {
        id,
        user_id,
        name,
        brand,
        barcode,
        notes,
        kcal_per_100,
        protein_g_per_100,
        carbs_g_per_100,
        fat_g_per_100,
        serving_qty,
        serving_unit: QuantityUnit::parse(&serving_unit)?,
        serving_label,
        source: FoodSource::parse(&source),
        created_at: parse_dt(&created_at)?,
        updated_at: parse_dt(&updated_at)?,
    })
}

pub async fn insert(pool: &SqlitePool, food: NewFood) -> Result<Food> {
    let name = food.name.trim();
    if name.is_empty() {
        return Err(anyhow!("food name is required"));
    }
    if food.serving_qty <= 0.0 {
        return Err(anyhow!("serving_qty must be positive"));
    }
    reject_negative_nutrients(
        food.kcal_per_100,
        food.protein_g_per_100,
        food.carbs_g_per_100,
        food.fat_g_per_100,
    )?;

    let brand = nonempty(food.brand);
    let barcode = nonempty(food.barcode);
    let notes = nonempty(food.notes);
    let serving_label = nonempty(food.serving_label);
    let serving_unit = food.serving_unit.as_str();
    let source = food.source.as_str();
    let kcal = round1(food.kcal_per_100);
    let protein = round1(food.protein_g_per_100);
    let carbs = round1(food.carbs_g_per_100);
    let fat = round1(food.fat_g_per_100);

    let id = sqlx::query!(
        r#"
        INSERT INTO foods (
            user_id, name, brand, barcode, notes,
            kcal_per_100, protein_g_per_100, carbs_g_per_100, fat_g_per_100,
            serving_qty, serving_unit, serving_label, source
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        food.user_id,
        name,
        brand,
        barcode,
        notes,
        kcal,
        protein,
        carbs,
        fat,
        food.serving_qty,
        serving_unit,
        serving_label,
        source,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    get_for_user(pool, food.user_id, id)
        .await?
        .ok_or_else(|| anyhow!("food {id} missing after insert"))
}

pub async fn get_for_user(pool: &SqlitePool, user_id: i64, food_id: i64) -> Result<Option<Food>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, name, brand, barcode, notes,
               kcal_per_100, protein_g_per_100, carbs_g_per_100, fat_g_per_100,
               serving_qty, serving_unit, serving_label, source, created_at, updated_at
        FROM foods
        WHERE id = ? AND user_id = ?
        "#,
        food_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(row_to_food(
            r.id,
            r.user_id,
            r.name,
            r.brand,
            r.barcode,
            r.notes,
            r.kcal_per_100,
            r.protein_g_per_100,
            r.carbs_g_per_100,
            r.fat_g_per_100,
            r.serving_qty,
            r.serving_unit,
            r.serving_label,
            r.source,
            r.created_at,
            r.updated_at,
        )?)),
    }
}

pub async fn list_for_user(pool: &SqlitePool, user_id: i64) -> Result<Vec<Food>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, name, brand, barcode, notes,
               kcal_per_100, protein_g_per_100, carbs_g_per_100, fat_g_per_100,
               serving_qty, serving_unit, serving_label, source, created_at, updated_at
        FROM foods
        WHERE user_id = ?
        ORDER BY name COLLATE NOCASE ASC
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_food(
                r.id.ok_or_else(|| anyhow!("food missing id"))?,
                r.user_id,
                r.name,
                r.brand,
                r.barcode,
                r.notes,
                r.kcal_per_100,
                r.protein_g_per_100,
                r.carbs_g_per_100,
                r.fat_g_per_100,
                r.serving_qty,
                r.serving_unit,
                r.serving_label,
                r.source,
                r.created_at,
                r.updated_at,
            )
        })
        .collect()
}

/// Case-insensitive exact name match, then substring. Empty query lists nothing here
/// (callers should use `list_for_user`).
pub async fn search_by_name(pool: &SqlitePool, user_id: i64, query: &str) -> Result<Vec<Food>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{q}%");
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, name, brand, barcode, notes,
               kcal_per_100, protein_g_per_100, carbs_g_per_100, fat_g_per_100,
               serving_qty, serving_unit, serving_label, source, created_at, updated_at
        FROM foods
        WHERE user_id = ?
          AND (name LIKE ? COLLATE NOCASE OR IFNULL(brand, '') LIKE ? COLLATE NOCASE)
        ORDER BY
            CASE WHEN name = ? COLLATE NOCASE THEN 0 ELSE 1 END,
            name COLLATE NOCASE ASC
        LIMIT 20
        "#,
        user_id,
        like,
        like,
        q,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            row_to_food(
                r.id.ok_or_else(|| anyhow!("food missing id"))?,
                r.user_id,
                r.name,
                r.brand,
                r.barcode,
                r.notes,
                r.kcal_per_100,
                r.protein_g_per_100,
                r.carbs_g_per_100,
                r.fat_g_per_100,
                r.serving_qty,
                r.serving_unit,
                r.serving_label,
                r.source,
                r.created_at,
                r.updated_at,
            )
        })
        .collect()
}

pub async fn update(
    pool: &SqlitePool,
    user_id: i64,
    food_id: i64,
    patch: FoodUpdate,
) -> Result<Food> {
    let current = get_for_user(pool, user_id, food_id)
        .await?
        .ok_or_else(|| anyhow!("food not found"))?;

    let name = patch
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&current.name);
    let brand = match patch.brand {
        Some(v) => nonempty(v),
        None => current.brand.clone(),
    };
    let barcode = match patch.barcode {
        Some(v) => nonempty(v),
        None => current.barcode.clone(),
    };
    let notes = match patch.notes {
        Some(v) => nonempty(v),
        None => current.notes.clone(),
    };
    let kcal = round1(patch.kcal_per_100.unwrap_or(current.kcal_per_100));
    let protein = round1(patch.protein_g_per_100.unwrap_or(current.protein_g_per_100));
    let carbs = round1(patch.carbs_g_per_100.unwrap_or(current.carbs_g_per_100));
    let fat = round1(patch.fat_g_per_100.unwrap_or(current.fat_g_per_100));
    reject_negative_nutrients(kcal, protein, carbs, fat)?;
    let serving_qty = patch.serving_qty.unwrap_or(current.serving_qty);
    if serving_qty <= 0.0 {
        return Err(anyhow!("serving_qty must be positive"));
    }
    let serving_unit = patch.serving_unit.unwrap_or(current.serving_unit);
    let serving_unit_s = serving_unit.as_str();
    let serving_label = match patch.serving_label {
        Some(v) => nonempty(v),
        None => current.serving_label.clone(),
    };
    let source = patch.source.unwrap_or(current.source);
    let source_s = source.as_str();

    sqlx::query!(
        r#"
        UPDATE foods SET
            name = ?, brand = ?, barcode = ?, notes = ?,
            kcal_per_100 = ?, protein_g_per_100 = ?, carbs_g_per_100 = ?, fat_g_per_100 = ?,
            serving_qty = ?, serving_unit = ?, serving_label = ?, source = ?,
            updated_at = datetime('now')
        WHERE id = ? AND user_id = ?
        "#,
        name,
        brand,
        barcode,
        notes,
        kcal,
        protein,
        carbs,
        fat,
        serving_qty,
        serving_unit_s,
        serving_label,
        source_s,
        food_id,
        user_id,
    )
    .execute(pool)
    .await?;

    get_for_user(pool, user_id, food_id)
        .await?
        .ok_or_else(|| anyhow!("food {food_id} missing after update"))
}

pub async fn delete(pool: &SqlitePool, user_id: i64, food_id: i64) -> Result<bool> {
    let res = sqlx::query!(
        "DELETE FROM foods WHERE id = ? AND user_id = ?",
        food_id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FoodEmbedding {
    pub food_id: i64,
    pub embedding: String,
}

pub async fn list_embeddings(pool: &SqlitePool, user_id: i64) -> Result<Vec<FoodEmbedding>> {
    sqlx::query_as::<_, FoodEmbedding>(
        r#"
        SELECT fe.food_id, fe.embedding
        FROM food_embeddings fe
        JOIN foods f ON f.id = fe.food_id
        WHERE f.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("failed to list food embeddings")
}

pub async fn upsert_embedding(
    pool: &SqlitePool,
    food_id: i64,
    embed_text: &str,
    embedding_json: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO food_embeddings (food_id, embed_text, embedding)
        VALUES (?, ?, ?)
        ON CONFLICT(food_id) DO UPDATE SET
            embed_text = excluded.embed_text,
            embedding = excluded.embedding,
            created_at = datetime('now')
        "#,
    )
    .bind(food_id)
    .bind(embed_text)
    .bind(embedding_json)
    .execute(pool)
    .await
    .context("failed to upsert food embedding")?;
    Ok(())
}

/// Convert per-serving nutrients to per-100g when the label only shows a serving.
pub fn per_100_from_serving(value: f64, serving_qty: f64) -> Result<f64> {
    if serving_qty <= 0.0 {
        return Err(anyhow!("serving_qty must be positive"));
    }
    Ok(round1(value / serving_qty * 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};

    async fn user(pool: &SqlitePool, tg: i64) -> i64 {
        insert_user(pool, tg, Role::Sportive, Some("tester".into()), None, None)
            .await
            .unwrap()
            .id
    }

    fn whey(user_id: i64) -> NewFood {
        NewFood {
            user_id,
            name: "Whey Isolate".into(),
            brand: Some("MyBrand".into()),
            barcode: None,
            notes: None,
            kcal_per_100: 370.0,
            protein_g_per_100: 80.0,
            carbs_g_per_100: 6.0,
            fat_g_per_100: 4.0,
            serving_qty: 30.0,
            serving_unit: QuantityUnit::G,
            serving_label: Some("1 scoop".into()),
            source: FoodSource::Photo,
        }
    }

    #[test]
    fn two_scoops_scale_from_serving() {
        let food = Food {
            id: 1,
            user_id: 1,
            name: "whey".into(),
            brand: None,
            barcode: None,
            notes: None,
            kcal_per_100: 370.0,
            protein_g_per_100: 80.0,
            carbs_g_per_100: 6.0,
            fat_g_per_100: 4.0,
            serving_qty: 30.0,
            serving_unit: QuantityUnit::G,
            serving_label: Some("1 scoop".into()),
            source: FoodSource::Manual,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let m = food.scale(2.0, QuantityUnit::Serving).unwrap();
        assert_eq!(m.protein_g, 48.0);
        assert_eq!(m.kcal, 222.0);
        let g = food.scale(30.0, QuantityUnit::G).unwrap();
        assert_eq!(g.protein_g, 24.0);
    }

    #[test]
    fn per_100_from_label_serving() {
        assert_eq!(per_100_from_serving(24.0, 30.0).unwrap(), 80.0);
    }

    #[tokio::test]
    async fn insert_list_search_update_delete() {
        let (pool, path) = test_db().await;
        let user_id = user(&pool, 88101).await;
        let food = insert(&pool, whey(user_id)).await.unwrap();
        assert_eq!(food.name, "Whey Isolate");
        assert_eq!(food.serving_qty, 30.0);

        let list = list_for_user(&pool, user_id).await.unwrap();
        assert_eq!(list.len(), 1);

        let hits = search_by_name(&pool, user_id, "whey").await.unwrap();
        assert_eq!(hits.len(), 1);
        let brand_hits = search_by_name(&pool, user_id, "MyBrand").await.unwrap();
        assert_eq!(brand_hits.len(), 1);

        let updated = update(
            &pool,
            user_id,
            food.id,
            FoodUpdate {
                protein_g_per_100: Some(82.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.protein_g_per_100, 82.0);

        assert!(delete(&pool, user_id, food.id).await.unwrap());
        assert!(
            get_for_user(&pool, user_id, food.id)
                .await
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn insert_and_update_reject_negative_nutrients() {
        let (pool, path) = test_db().await;
        let user_id = user(&pool, 88102).await;

        let mut bad = whey(user_id);
        bad.kcal_per_100 = -10.0;
        assert!(
            insert(&pool, bad)
                .await
                .unwrap_err()
                .to_string()
                .contains("negative")
        );

        let food = insert(&pool, whey(user_id)).await.unwrap();
        let patches = [
            FoodUpdate {
                kcal_per_100: Some(-1.0),
                ..Default::default()
            },
            FoodUpdate {
                protein_g_per_100: Some(-0.1),
                ..Default::default()
            },
            FoodUpdate {
                carbs_g_per_100: Some(-5.0),
                ..Default::default()
            },
            FoodUpdate {
                fat_g_per_100: Some(-2.0),
                ..Default::default()
            },
        ];
        for patch in patches {
            let err = update(&pool, user_id, food.id, patch)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("negative"), "unexpected error: {err}");
        }

        let still = get_for_user(&pool, user_id, food.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(still.kcal_per_100, 370.0);
        assert_eq!(still.protein_g_per_100, 80.0);
        assert_eq!(still.carbs_g_per_100, 6.0);
        assert_eq!(still.fat_g_per_100, 4.0);

        let zeroed = update(
            &pool,
            user_id,
            food.id,
            FoodUpdate {
                fat_g_per_100: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(zeroed.fat_g_per_100, 0.0);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn upsert_embedding_inserts_and_replaces() {
        let (pool, path) = test_db().await;
        let user_id = user(&pool, 88103).await;
        let food = insert(&pool, whey(user_id)).await.unwrap();

        upsert_embedding(&pool, food.id, "MyBrand Whey Isolate", "[1,0]")
            .await
            .unwrap();
        let stored = list_embeddings(&pool, user_id).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].food_id, food.id);
        assert_eq!(stored[0].embedding, "[1,0]");

        upsert_embedding(&pool, food.id, "MyBrand Whey Concentrate", "[0,1]")
            .await
            .unwrap();
        let stored = list_embeddings(&pool, user_id).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].embedding, "[0,1]");
        let embed_text: String =
            sqlx::query_scalar("SELECT embed_text FROM food_embeddings WHERE food_id = ?")
                .bind(food.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(embed_text, "MyBrand Whey Concentrate");

        let _ = std::fs::remove_file(&path);
    }
}
