use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::datetime::{local_day_bounds, parse_sqlite_datetime};
use crate::db::foods::{Food, Macros, QuantityUnit, round1};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MealKind {
    Breakfast,
    Lunch,
    Dinner,
    Snack,
    Other,
}

impl MealKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Breakfast => "breakfast",
            Self::Lunch => "lunch",
            Self::Dinner => "dinner",
            Self::Snack => "snack",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "breakfast" => Ok(Self::Breakfast),
            "lunch" => Ok(Self::Lunch),
            "dinner" => Ok(Self::Dinner),
            "snack" => Ok(Self::Snack),
            "other" => Ok(Self::Other),
            other => Err(anyhow!("unknown meal kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemSource {
    Catalog,
    Estimated,
    Photo,
}

impl ItemSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catalog => "catalog",
            Self::Estimated => "estimated",
            Self::Photo => "photo",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "estimated" => Self::Estimated,
            "photo" => Self::Photo,
            _ => Self::Catalog,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MealItem {
    pub id: i64,
    pub meal_id: i64,
    pub food_id: Option<i64>,
    pub description: String,
    pub quantity: f64,
    pub unit: QuantityUnit,
    pub macros: Macros,
    pub source: ItemSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meal {
    pub id: i64,
    pub user_id: i64,
    pub kind: MealKind,
    pub label: Option<String>,
    pub eaten_at: DateTime<Utc>,
    pub notes: Option<String>,
    pub items: Vec<MealItem>,
}

impl Meal {
    pub fn totals(&self) -> Macros {
        let mut t = Macros::zero();
        for item in &self.items {
            t.add(&item.macros);
        }
        t.rounded()
    }
}

#[derive(Debug, Clone)]
pub struct NewMealItem {
    pub food_id: Option<i64>,
    pub description: String,
    pub quantity: f64,
    pub unit: QuantityUnit,
    pub macros: Macros,
    pub source: ItemSource,
}

#[derive(Debug, Clone)]
pub struct NewMeal {
    pub user_id: i64,
    pub kind: MealKind,
    pub label: Option<String>,
    pub eaten_at: DateTime<Utc>,
    pub notes: Option<String>,
    pub items: Vec<NewMealItem>,
}

#[derive(Debug, Clone, Default)]
pub struct MealUpdate {
    pub kind: Option<MealKind>,
    pub label: Option<Option<String>>,
    pub eaten_at: Option<DateTime<Utc>>,
    pub notes: Option<Option<String>>,
    pub items: Option<Vec<NewMealItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayNutrition {
    pub date: NaiveDate,
    pub timezone: String,
    pub meals: Vec<Meal>,
    pub totals: Macros,
}

impl DayNutrition {
    pub fn from_meals(date: NaiveDate, timezone: &str, meals: Vec<Meal>) -> Self {
        let mut totals = Macros::zero();
        for meal in &meals {
            totals.add(&meal.totals());
        }
        Self {
            date,
            timezone: timezone.to_string(),
            meals,
            totals: totals.rounded(),
        }
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>> {
    parse_sqlite_datetime(s).ok_or_else(|| anyhow!("invalid datetime: {s}"))
}

/// Build a snapshotted item from a catalog food.
pub fn item_from_food(
    food: &Food,
    quantity: f64,
    unit: QuantityUnit,
    description: Option<String>,
) -> Result<NewMealItem> {
    let macros = food.scale(quantity, unit)?;
    let description = description
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| food.embed_text());
    Ok(NewMealItem {
        food_id: Some(food.id),
        description,
        quantity,
        unit,
        macros,
        source: ItemSource::Catalog,
    })
}

pub fn estimated_item(
    description: String,
    quantity: f64,
    unit: QuantityUnit,
    macros: Macros,
    source: ItemSource,
) -> Result<NewMealItem> {
    if description.trim().is_empty() {
        return Err(anyhow!("item description is required"));
    }
    if quantity <= 0.0 {
        return Err(anyhow!("quantity must be positive"));
    }
    if macros.kcal < 0.0 || macros.protein_g < 0.0 || macros.carbs_g < 0.0 || macros.fat_g < 0.0 {
        return Err(anyhow!("nutrients cannot be negative"));
    }
    Ok(NewMealItem {
        food_id: None,
        description: description.trim().to_string(),
        quantity,
        unit,
        macros: macros.rounded(),
        source,
    })
}

pub async fn insert(pool: &SqlitePool, meal: NewMeal) -> Result<Meal> {
    if meal.items.is_empty() {
        return Err(anyhow!("meal must have at least one item"));
    }
    let kind = meal.kind.as_str();
    let label = nonempty(meal.label);
    let notes = nonempty(meal.notes);
    let eaten_at = meal.eaten_at.to_rfc3339();

    let mut tx = pool.begin().await?;
    let id = sqlx::query!(
        r#"
        INSERT INTO meals (user_id, kind, label, eaten_at, notes)
        VALUES (?, ?, ?, ?, ?)
        "#,
        meal.user_id,
        kind,
        label,
        eaten_at,
        notes,
    )
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    for item in &meal.items {
        insert_item_tx(&mut tx, id, item).await?;
    }
    tx.commit().await?;
    get_for_user(pool, meal.user_id, id)
        .await?
        .ok_or_else(|| anyhow!("meal {id} missing after insert"))
}

async fn insert_item_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    meal_id: i64,
    item: &NewMealItem,
) -> Result<()> {
    let unit = item.unit.as_str();
    let source = item.source.as_str();
    let kcal = round1(item.macros.kcal);
    let protein = round1(item.macros.protein_g);
    let carbs = round1(item.macros.carbs_g);
    let fat = round1(item.macros.fat_g);
    sqlx::query!(
        r#"
        INSERT INTO meal_items (
            meal_id, food_id, description, quantity, unit,
            kcal, protein_g, carbs_g, fat_g, source
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        meal_id,
        item.food_id,
        item.description,
        item.quantity,
        unit,
        kcal,
        protein,
        carbs,
        fat,
        source,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn get_for_user(pool: &SqlitePool, user_id: i64, meal_id: i64) -> Result<Option<Meal>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, kind, label, eaten_at, notes
        FROM meals
        WHERE id = ? AND user_id = ?
        "#,
        meal_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };
    let items = list_items(pool, r.id).await?;
    Ok(Some(Meal {
        id: r.id,
        user_id: r.user_id,
        kind: MealKind::parse(&r.kind)?,
        label: r.label,
        eaten_at: parse_dt(&r.eaten_at)?,
        notes: r.notes,
        items,
    }))
}

async fn list_items(pool: &SqlitePool, meal_id: i64) -> Result<Vec<MealItem>> {
    let rows = sqlx::query!(
        r#"
        SELECT id, meal_id, food_id, description, quantity, unit,
               kcal, protein_g, carbs_g, fat_g, source
        FROM meal_items
        WHERE meal_id = ?
        ORDER BY id ASC
        "#,
        meal_id,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(MealItem {
                id: r.id.ok_or_else(|| anyhow!("meal_item missing id"))?,
                meal_id: r.meal_id,
                food_id: r.food_id,
                description: r.description,
                quantity: r.quantity,
                unit: QuantityUnit::parse(&r.unit)?,
                macros: Macros {
                    kcal: r.kcal,
                    protein_g: r.protein_g,
                    carbs_g: r.carbs_g,
                    fat_g: r.fat_g,
                },
                source: ItemSource::parse(&r.source),
            })
        })
        .collect()
}

pub async fn day_for_user(
    pool: &SqlitePool,
    user_id: i64,
    timezone: &str,
    date: NaiveDate,
) -> Result<DayNutrition> {
    let tz: Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let (start, end) = local_day_bounds(date, tz)
        .ok_or_else(|| anyhow!("could not resolve local day bounds for {date}"))?;
    let start_s = start.to_rfc3339();
    let end_s = end.to_rfc3339();

    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, kind, label, eaten_at, notes
        FROM meals
        WHERE user_id = ? AND eaten_at >= ? AND eaten_at < ?
        ORDER BY eaten_at ASC, id ASC
        "#,
        user_id,
        start_s,
        end_s,
    )
    .fetch_all(pool)
    .await?;

    let mut meals = Vec::with_capacity(rows.len());
    for r in rows {
        let id = r.id.ok_or_else(|| anyhow!("meal missing id"))?;
        let items = list_items(pool, id).await?;
        meals.push(Meal {
            id,
            user_id: r.user_id,
            kind: MealKind::parse(&r.kind)?,
            label: r.label,
            eaten_at: parse_dt(&r.eaten_at)?,
            notes: r.notes,
            items,
        });
    }
    Ok(DayNutrition::from_meals(date, timezone, meals))
}

pub async fn update(
    pool: &SqlitePool,
    user_id: i64,
    meal_id: i64,
    patch: MealUpdate,
) -> Result<Meal> {
    let current = get_for_user(pool, user_id, meal_id)
        .await?
        .ok_or_else(|| anyhow!("meal not found"))?;

    let kind = patch.kind.unwrap_or(current.kind);
    let kind_s = kind.as_str();
    let label = match patch.label {
        Some(v) => nonempty(v),
        None => current.label.clone(),
    };
    let notes = match patch.notes {
        Some(v) => nonempty(v),
        None => current.notes.clone(),
    };
    let eaten_at = patch.eaten_at.unwrap_or(current.eaten_at).to_rfc3339();

    let mut tx = pool.begin().await?;
    sqlx::query!(
        r#"
        UPDATE meals SET kind = ?, label = ?, eaten_at = ?, notes = ?
        WHERE id = ? AND user_id = ?
        "#,
        kind_s,
        label,
        eaten_at,
        notes,
        meal_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;

    if let Some(items) = patch.items {
        if items.is_empty() {
            return Err(anyhow!("meal must have at least one item"));
        }
        sqlx::query!("DELETE FROM meal_items WHERE meal_id = ?", meal_id)
            .execute(&mut *tx)
            .await?;
        for item in &items {
            insert_item_tx(&mut tx, meal_id, item).await?;
        }
    }
    tx.commit().await?;
    get_for_user(pool, user_id, meal_id)
        .await?
        .ok_or_else(|| anyhow!("meal {meal_id} missing after update"))
}

pub async fn delete(pool: &SqlitePool, user_id: i64, meal_id: i64) -> Result<bool> {
    let res = sqlx::query!(
        "DELETE FROM meals WHERE id = ? AND user_id = ?",
        meal_id,
        user_id,
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::foods::{FoodSource, NewFood, QuantityUnit, insert as insert_food};
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use chrono::TimeZone;

    async fn setup() -> (sqlx::SqlitePool, std::path::PathBuf, i64) {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            88201,
            Role::Sportive,
            Some("t".into()),
            Some("fr".into()),
            None,
        )
        .await
        .unwrap();
        (pool, path, user.id)
    }

    #[tokio::test]
    async fn insert_meal_snapshots_and_day_totals() {
        let (pool, path, user_id) = setup().await;
        let food = insert_food(
            &pool,
            NewFood {
                user_id,
                name: "Whey".into(),
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
            },
        )
        .await
        .unwrap();

        let item = item_from_food(&food, 2.0, QuantityUnit::Serving, None).unwrap();
        assert_eq!(item.macros.protein_g, 48.0);

        let pizza = estimated_item(
            "pizza".into(),
            1.0,
            QuantityUnit::Piece,
            Macros {
                kcal: 700.0,
                protein_g: 28.0,
                carbs_g: 80.0,
                fat_g: 28.0,
            },
            ItemSource::Estimated,
        )
        .unwrap();

        let eaten = chrono_tz::Europe::Paris
            .with_ymd_and_hms(2026, 8, 18, 13, 0, 0)
            .unwrap()
            .with_timezone(&Utc);

        let meal = insert(
            &pool,
            NewMeal {
                user_id,
                kind: MealKind::Lunch,
                label: Some("post-workout".into()),
                eaten_at: eaten,
                notes: None,
                items: vec![item, pizza],
            },
        )
        .await
        .unwrap();
        assert_eq!(meal.items.len(), 2);
        assert_eq!(meal.totals().protein_g, 76.0);

        let day = day_for_user(
            &pool,
            user_id,
            "Europe/Paris",
            NaiveDate::from_ymd_opt(2026, 8, 18).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(day.meals.len(), 1);
        assert_eq!(day.totals.kcal, 922.0);

        let other_day = day_for_user(
            &pool,
            user_id,
            "Europe/Paris",
            NaiveDate::from_ymd_opt(2026, 8, 17).unwrap(),
        )
        .await
        .unwrap();
        assert!(other_day.meals.is_empty());

        assert!(delete(&pool, user_id, meal.id).await.unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_replaces_items() {
        let (pool, path, user_id) = setup().await;
        let eaten = Utc::now();
        let meal = insert(
            &pool,
            NewMeal {
                user_id,
                kind: MealKind::Snack,
                label: None,
                eaten_at: eaten,
                notes: None,
                items: vec![
                    estimated_item(
                        "banana".into(),
                        1.0,
                        QuantityUnit::Piece,
                        Macros {
                            kcal: 90.0,
                            protein_g: 1.0,
                            carbs_g: 23.0,
                            fat_g: 0.3,
                        },
                        ItemSource::Estimated,
                    )
                    .unwrap(),
                ],
            },
        )
        .await
        .unwrap();

        let updated = update(
            &pool,
            user_id,
            meal.id,
            MealUpdate {
                kind: Some(MealKind::Breakfast),
                items: Some(vec![
                    estimated_item(
                        "oats".into(),
                        80.0,
                        QuantityUnit::G,
                        Macros {
                            kcal: 300.0,
                            protein_g: 10.0,
                            carbs_g: 50.0,
                            fat_g: 6.0,
                        },
                        ItemSource::Estimated,
                    )
                    .unwrap(),
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.kind, MealKind::Breakfast);
        assert_eq!(updated.items.len(), 1);
        assert_eq!(updated.items[0].description, "oats");
        let _ = std::fs::remove_file(&path);
    }
}
