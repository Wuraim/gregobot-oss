use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::db::Db;
use crate::db::food_preferences::{FoodPreference, PreferenceKind};
use crate::db::foods::{self, Food};

/// Minimum cosine similarity to accept a catalog hit.
pub(crate) const MIN_SCORE: f32 = 0.52;
const MIN_MARGIN: f32 = 0.04;
pub(crate) const HIGH_CONFIDENCE: f32 = 0.72;
const LIKE_BOOST: f32 = 0.12;
const HABIT_BOOST: f32 = 0.06;

#[derive(Debug, Clone)]
pub struct FoodMatch {
    pub food: Food,
    pub score: f32,
}

/// Embed the food name/brand and persist it for later KNN lookup.
pub async fn embed_food(
    pool: &Db,
    api_key: &str,
    embedding_model: &str,
    food: &Food,
) -> Result<()> {
    let embed_text = food.embed_text();
    let embedding = embed_food_text(api_key, embedding_model, &embed_text)
        .await
        .context("failed to embed food name")?;
    let json = crate::embeddings::serialize_embedding(&embedding);
    foods::upsert_embedding(pool, food.id, &embed_text, &json).await
}

/// Same as chat `register_food` / `update_food`: skip when no API key; log failures.
pub async fn maybe_embed_food(
    pool: &Db,
    api_key: Option<&str>,
    embedding_model: &str,
    food: &Food,
) {
    let Some(api_key) = api_key.filter(|k| !k.is_empty()) else {
        return;
    };
    if let Err(e) = embed_food(pool, api_key, embedding_model, food).await {
        tracing::warn!(food_id = food.id, "Failed to embed food: {e:#}");
    }
}

async fn embed_food_text(api_key: &str, embedding_model: &str, text: &str) -> Result<Vec<f32>> {
    #[cfg(test)]
    if api_key == TEST_EMBED_API_KEY {
        return Ok(stub_embedding(text));
    }
    let (embedding, _usage) = crate::embeddings::embed(api_key, embedding_model, text).await?;
    Ok(embedding)
}

#[cfg(test)]
pub const TEST_EMBED_API_KEY: &str = "test-embed-key";

#[cfg(test)]
fn stub_embedding(text: &str) -> Vec<f32> {
    vec![text.len() as f32, 1.0, 0.0]
}

/// Embed a stored preference (`kind: text`) for later KNN / envie matching.
pub async fn embed_preference(
    pool: &Db,
    api_key: &str,
    embedding_model: &str,
    pref: &FoodPreference,
) -> Result<()> {
    let embed_text = pref.embed_text();
    let embedding = embed_food_text(api_key, embedding_model, &embed_text)
        .await
        .context("failed to embed food preference")?;
    let json = crate::embeddings::serialize_embedding(&embedding);
    crate::db::food_preferences::upsert_embedding(pool, pref.id, &embed_text, &json).await
}

/// Resolve a free-text food name against the user's catalog.
/// Exact/substring SQL match first; embedding KNN if that is empty or ambiguous.
pub async fn resolve_food(
    pool: &Db,
    user_id: i64,
    query: &str,
    api_key: Option<&str>,
    embedding_model: &str,
) -> Result<Vec<FoodMatch>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let sql_hits = foods::search_by_name(pool, user_id, q).await?;
    if sql_hits.len() == 1 {
        return Ok(vec![FoodMatch {
            food: sql_hits.into_iter().next().unwrap(),
            score: 1.0,
        }]);
    }
    if sql_hits.len() > 1 {
        let exact: Vec<Food> = sql_hits
            .iter()
            .filter(|f| f.name.eq_ignore_ascii_case(q))
            .cloned()
            .collect();
        if exact.len() == 1 {
            return Ok(vec![FoodMatch {
                food: exact.into_iter().next().unwrap(),
                score: 1.0,
            }]);
        }
        return Ok(sql_hits
            .into_iter()
            .map(|food| FoodMatch { food, score: 0.9 })
            .collect());
    }

    knn_search(pool, user_id, q, api_key, embedding_model).await
}

async fn knn_search(
    pool: &Db,
    user_id: i64,
    query: &str,
    api_key: Option<&str>,
    embedding_model: &str,
) -> Result<Vec<FoodMatch>> {
    let Some(api_key) = api_key.filter(|k| !k.is_empty()) else {
        return Ok(Vec::new());
    };
    let stored = foods::list_embeddings(pool, user_id).await?;
    if stored.is_empty() {
        return Ok(Vec::new());
    }

    let query_emb = match embed_food_text(api_key, embedding_model, query).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("food KNN embed failed: {e:#}");
            return Ok(Vec::new());
        }
    };

    let catalog = foods::list_for_user(pool, user_id).await?;
    let mut scored: Vec<FoodMatch> = Vec::new();
    for row in stored {
        let embedding: Vec<f32> = match serde_json::from_str(&row.embedding) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(food) = catalog.iter().find(|f| f.id == row.food_id).cloned() else {
            continue;
        };
        scored.push(FoodMatch {
            food,
            score: cosine_similarity(&query_emb, &embedding),
        });
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(pick_matches(&scored))
}

fn pick_matches(scored: &[FoodMatch]) -> Vec<FoodMatch> {
    let Some(best) = scored.first() else {
        return Vec::new();
    };
    if best.score < MIN_SCORE {
        return Vec::new();
    }
    let second = scored.get(1).map(|m| m.score).unwrap_or(0.0);
    if best.score < HIGH_CONFIDENCE && (best.score - second) < MIN_MARGIN {
        return scored
            .iter()
            .take(3)
            .filter(|m| m.score >= MIN_SCORE)
            .cloned()
            .collect();
    }
    vec![best.clone()]
}

/// Cosine similarity between two embedding vectors.
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())) as f32
}

#[derive(Debug, Clone)]
pub struct RankedFood {
    pub food: Food,
    pub envie_score: f32,
    pub score: f32,
}

pub fn parse_embedding_json(json: &str) -> Option<Vec<f32>> {
    serde_json::from_str(json).ok()
}

pub fn food_text_matches(food: &Food, needle: &str) -> bool {
    let n = needle.trim().to_lowercase();
    if n.len() < 2 {
        return false;
    }
    let name = food.name.to_lowercase();
    let brand = food.brand.as_deref().unwrap_or("").to_lowercase();
    name.contains(&n) || brand.contains(&n)
}

fn lexical_envie_score(food: &Food, envie: &str) -> f32 {
    let envie = envie.trim();
    if envie.is_empty() {
        return 0.2;
    }
    if food_text_matches(food, envie) {
        return 0.85;
    }
    let tokens: Vec<&str> = envie
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .collect();
    if tokens.iter().any(|t| food_text_matches(food, t)) {
        return 0.7;
    }
    0.2
}

/// Drop foods tagged allergy/dislike by food_id, name/brand text, or high cosine vs the preference embedding.
pub fn food_is_blocked(
    food: &Food,
    prefs: &[FoodPreference],
    food_embeddings: &HashMap<i64, Vec<f32>>,
    pref_embeddings: &HashMap<i64, Vec<f32>>,
) -> bool {
    for pref in prefs {
        if !pref.kind.is_hard_block() {
            continue;
        }
        if pref.food_id == Some(food.id) {
            return true;
        }
        if food_text_matches(food, &pref.text) {
            return true;
        }
        if let (Some(fe), Some(pe)) = (food_embeddings.get(&food.id), pref_embeddings.get(&pref.id))
            && cosine_similarity(fe, pe) >= HIGH_CONFIDENCE
        {
            return true;
        }
    }
    false
}

fn like_habit_boost(food: &Food, prefs: &[FoodPreference]) -> f32 {
    let mut boost = 0.0;
    for pref in prefs {
        let matches = pref.food_id == Some(food.id) || food_text_matches(food, &pref.text);
        if !matches {
            continue;
        }
        match pref.kind {
            PreferenceKind::Like => boost += LIKE_BOOST,
            PreferenceKind::Habit => boost += HABIT_BOOST,
            PreferenceKind::Allergy | PreferenceKind::Dislike => {}
        }
    }
    boost
}

/// Rank catalog foods against an envie vector (or lexical fallback when embeddings are missing).
pub fn rank_catalog(
    envie: &str,
    envie_emb: Option<&[f32]>,
    catalog: &[Food],
    food_embeddings: &HashMap<i64, Vec<f32>>,
    prefs: &[FoodPreference],
    pref_embeddings: &HashMap<i64, Vec<f32>>,
) -> Vec<RankedFood> {
    let mut ranked: Vec<RankedFood> = catalog
        .iter()
        .filter(|food| !food_is_blocked(food, prefs, food_embeddings, pref_embeddings))
        .map(|food| {
            let envie_score = match (envie_emb, food_embeddings.get(&food.id)) {
                (Some(q), Some(fe)) => cosine_similarity(q, fe),
                _ => lexical_envie_score(food, envie),
            };
            let score = envie_score + like_habit_boost(food, prefs);
            RankedFood {
                food: food.clone(),
                envie_score,
                score,
            }
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::foods::{FoodSource, QuantityUnit};
    use chrono::Utc;

    fn dummy_food(id: i64, name: &str) -> Food {
        Food {
            id,
            user_id: 1,
            name: name.into(),
            brand: None,
            barcode: None,
            notes: None,
            kcal_per_100: 100.0,
            protein_g_per_100: 10.0,
            carbs_g_per_100: 10.0,
            fat_g_per_100: 1.0,
            serving_qty: 100.0,
            serving_unit: QuantityUnit::G,
            serving_label: None,
            source: FoodSource::Manual,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn pick_matches_requires_score() {
        let scored = vec![FoodMatch {
            food: dummy_food(1, "whey"),
            score: 0.40,
        }];
        assert!(pick_matches(&scored).is_empty());
    }

    #[test]
    fn pick_matches_clear_winner() {
        let scored = vec![
            FoodMatch {
                food: dummy_food(1, "whey"),
                score: 0.81,
            },
            FoodMatch {
                food: dummy_food(2, "oats"),
                score: 0.40,
            },
        ];
        let hits = pick_matches(&scored);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].food.id, 1);
    }

    #[test]
    fn pick_matches_ambiguous_returns_candidates() {
        let scored = vec![
            FoodMatch {
                food: dummy_food(1, "whey isolate"),
                score: 0.58,
            },
            FoodMatch {
                food: dummy_food(2, "whey concentrate"),
                score: 0.56,
            },
        ];
        let hits = pick_matches(&scored);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn maybe_embed_food_persists_and_refreshes() {
        use crate::db::foods::{NewFood, insert, list_embeddings, update};
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
        let food = insert(
            &pool,
            NewFood {
                user_id: user.id,
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
                serving_label: None,
                source: FoodSource::Manual,
            },
        )
        .await
        .unwrap();

        maybe_embed_food(&pool, None, "unused", &food).await;
        assert!(list_embeddings(&pool, user.id).await.unwrap().is_empty());

        maybe_embed_food(&pool, Some(TEST_EMBED_API_KEY), "unused", &food).await;
        let stored = list_embeddings(&pool, user.id).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].food_id, food.id);

        let renamed = update(
            &pool,
            user.id,
            food.id,
            crate::db::foods::FoodUpdate {
                name: Some("Oats".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        maybe_embed_food(&pool, Some(TEST_EMBED_API_KEY), "unused", &renamed).await;
        let embed_text: String =
            sqlx::query_scalar("SELECT embed_text FROM food_embeddings WHERE food_id = ?")
                .bind(food.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(embed_text, "MyBrand Oats");

        let _ = std::fs::remove_file(&path);
    }

    fn dummy_pref(
        id: i64,
        kind: PreferenceKind,
        text: &str,
        food_id: Option<i64>,
    ) -> FoodPreference {
        FoodPreference {
            id,
            user_id: 1,
            kind,
            text: text.into(),
            food_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn dislike_filters_out_ranked_food_by_name() {
        let tuna = dummy_food(1, "canned tuna");
        let oats = dummy_food(2, "oats");
        let prefs = vec![dummy_pref(10, PreferenceKind::Dislike, "tuna", None)];
        let ranked = rank_catalog(
            "salty",
            None,
            &[tuna, oats.clone()],
            &HashMap::new(),
            &prefs,
            &HashMap::new(),
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].food.id, oats.id);
    }

    #[test]
    fn dislike_filters_out_ranked_food_by_embedding() {
        let tuna = dummy_food(1, "skipjack");
        let oats = dummy_food(2, "oats");
        let prefs = vec![dummy_pref(10, PreferenceKind::Dislike, "fish I hate", None)];
        let food_emb = HashMap::from([(1, vec![1.0f32, 0.0]), (2, vec![0.0, 1.0])]);
        let pref_emb = HashMap::from([(10, vec![1.0f32, 0.0])]);
        let ranked = rank_catalog(
            "dinner",
            Some(&[0.0, 1.0]),
            &[tuna, oats.clone()],
            &food_emb,
            &prefs,
            &pref_emb,
        );
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].food.id, oats.id);
    }
}
