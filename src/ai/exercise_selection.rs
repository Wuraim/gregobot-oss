use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::db::Db;
use crate::db::strength_exercises::StrengthExercise;

/// Minimum cosine similarity to accept a KNN hit.
const MIN_SCORE: f32 = 0.52;
/// If 1st and 2nd are this close, treat the query as ambiguous → Other
/// unless the top score is clearly high.
const MIN_MARGIN: f32 = 0.04;
const HIGH_CONFIDENCE: f32 = 0.72;

static EXERCISE_EMBEDDINGS: OnceLock<HashMap<StrengthExercise, Vec<f32>>> = OnceLock::new();

/// Load or recompute catalog embeddings. Skips `Other`.
pub async fn ensure_embeddings(pool: &Db, api_key: &str, embedding_model: &str) -> Result<()> {
    let mut cache = HashMap::new();
    let mut missing: Vec<StrengthExercise> = Vec::new();

    for ex in StrengthExercise::catalog() {
        let embed_text = ex.embed_text();
        let stored = crate::db::exercise_embeddings::find_by_slug(pool, ex.slug()).await?;
        match stored {
            Some(row) if row.embed_text == embed_text => {
                let embedding = parse_embedding(&row.embedding)
                    .with_context(|| format!("failed to parse embedding for {}", ex.slug()))?;
                cache.insert(ex, embedding);
            }
            _ => missing.push(ex),
        }
    }

    if !missing.is_empty() {
        let texts: Vec<String> = missing.iter().map(|ex| ex.embed_text()).collect();
        let (embeddings, _) =
            crate::embeddings::embed_batch(api_key, embedding_model, texts.clone()).await?;
        if embeddings.len() != missing.len() {
            anyhow::bail!(
                "exercise embed_batch size mismatch: {} texts, {} vectors",
                missing.len(),
                embeddings.len()
            );
        }
        for (ex, (embed_text, embedding)) in
            missing.into_iter().zip(texts.into_iter().zip(embeddings))
        {
            let embedding_json = serialize_embedding(&embedding);
            crate::db::exercise_embeddings::upsert(pool, ex.slug(), &embed_text, &embedding_json)
                .await
                .with_context(|| format!("failed to persist embedding for {}", ex.slug()))?;
            cache.insert(ex, embedding);
        }
    }

    let _ = EXERCISE_EMBEDDINGS.set(cache);
    Ok(())
}

/// Resolve several labels, batching the embedding calls for unknown names.
pub async fn resolve_labels(
    pool: &Db,
    user_id: i64,
    api_key: Option<&str>,
    embedding_model: &str,
    labels: &[(Option<&str>, Option<&str>)],
    request_id: Option<&str>,
) -> Vec<Option<StrengthExercise>> {
    let mut results: Vec<Option<StrengthExercise>> = Vec::with_capacity(labels.len());
    let mut pending: Vec<(usize, String)> = Vec::new();

    for (i, (exercise, exercise_name)) in labels.iter().copied().enumerate() {
        match StrengthExercise::parse_input(exercise, exercise_name) {
            Some(ex) if ex != StrengthExercise::Other => results.push(Some(ex)),
            Some(_) | None => {
                if let Some(query) = knn_query(exercise, exercise_name) {
                    results.push(None);
                    pending.push((i, query));
                } else {
                    results.push(StrengthExercise::parse_input(exercise, exercise_name));
                }
            }
        }
    }

    if pending.is_empty() {
        return results;
    }

    let mut unique: Vec<String> = Vec::new();
    for (_, q) in &pending {
        if !unique.iter().any(|u| u == q) {
            unique.push(q.clone());
        }
    }

    let mapped = knn_queries(pool, user_id, api_key, embedding_model, &unique, request_id).await;
    for (idx, query) in pending {
        let ex = unique
            .iter()
            .position(|u| u == &query)
            .and_then(|i| mapped.get(i).copied())
            .unwrap_or(StrengthExercise::Other);
        results[idx] = Some(ex);
    }
    results
}

fn knn_query(exercise: Option<&str>, exercise_name: Option<&str>) -> Option<String> {
    let name = exercise_name.map(str::trim).filter(|s| !s.is_empty());
    let slug = exercise.map(str::trim).filter(|s| !s.is_empty());
    match (slug, name) {
        (_, Some(name)) if !name.eq_ignore_ascii_case("other") => Some(name.to_string()),
        (Some(slug), _) if !slug.eq_ignore_ascii_case("other") => Some(slug.to_string()),
        _ => None,
    }
}

async fn knn_queries(
    pool: &Db,
    user_id: i64,
    api_key: Option<&str>,
    embedding_model: &str,
    queries: &[String],
    request_id: Option<&str>,
) -> Vec<StrengthExercise> {
    let Some(cache) = EXERCISE_EMBEDDINGS.get() else {
        return queries.iter().map(|_| StrengthExercise::Other).collect();
    };
    if cache.is_empty() {
        return queries.iter().map(|_| StrengthExercise::Other).collect();
    }
    let Some(api_key) = api_key.filter(|k| !k.is_empty()) else {
        return queries.iter().map(|_| StrengthExercise::Other).collect();
    };

    let embeddings =
        match crate::embeddings::embed_batch(api_key, embedding_model, queries.to_vec()).await {
            Ok((v, usage)) if v.len() == queries.len() => {
                crate::ai::log::persist_embed(
                    pool,
                    user_id,
                    embedding_model,
                    usage,
                    crate::db::ai_usage::KIND_EMBED_EXERCISE,
                    request_id,
                )
                .await;
                v
            }
            Ok(_) => {
                tracing::warn!("exercise KNN batch size mismatch; falling back to Other");
                return queries.iter().map(|_| StrengthExercise::Other).collect();
            }
            Err(e) => {
                tracing::warn!("exercise KNN embed failed: {e:#}");
                return queries.iter().map(|_| StrengthExercise::Other).collect();
            }
        };

    embeddings
        .iter()
        .map(|emb| {
            let scored = score_catalog(cache, emb);
            pick_nearest(&scored).unwrap_or(StrengthExercise::Other)
        })
        .collect()
}

fn score_catalog(
    cache: &HashMap<StrengthExercise, Vec<f32>>,
    query: &[f32],
) -> Vec<(StrengthExercise, f32)> {
    let mut scored: Vec<(StrengthExercise, f32)> = cache
        .iter()
        .map(|(ex, embedding)| (*ex, cosine_similarity(query, embedding)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

fn pick_nearest(scored: &[(StrengthExercise, f32)]) -> Option<StrengthExercise> {
    let (best, best_score) = scored.first()?;
    if *best_score < MIN_SCORE {
        return None;
    }
    let second = scored.get(1).map(|(_, s)| *s).unwrap_or(0.0);
    if *best_score < HIGH_CONFIDENCE && (*best_score - second) < MIN_MARGIN {
        return None;
    }
    Some(*best)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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

fn parse_embedding(json: &str) -> Result<Vec<f32>> {
    serde_json::from_str(json).context("embedding is not a valid JSON array of f32")
}

fn serialize_embedding(embedding: &[f32]) -> String {
    serde_json::to_string(embedding).expect("embedding slice serializes to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knn_query_skips_explicit_other() {
        assert_eq!(knn_query(Some("other"), None), None);
        assert_eq!(
            knn_query(Some("other"), Some("farmer walk")).as_deref(),
            Some("farmer walk")
        );
        assert_eq!(
            knn_query(None, Some("develeoppé couche")).as_deref(),
            Some("develeoppé couche")
        );
    }

    #[test]
    fn pick_nearest_requires_score() {
        let scored = vec![
            (StrengthExercise::BenchPress, 0.40),
            (StrengthExercise::Squat, 0.20),
        ];
        assert_eq!(pick_nearest(&scored), None);
    }

    #[test]
    fn pick_nearest_accepts_clear_winner() {
        let scored = vec![
            (StrengthExercise::BenchPress, 0.81),
            (StrengthExercise::InclineBenchPress, 0.55),
        ];
        assert_eq!(pick_nearest(&scored), Some(StrengthExercise::BenchPress));
    }

    #[test]
    fn pick_nearest_rejects_ambiguous_pair() {
        let scored = vec![
            (StrengthExercise::BenchPress, 0.58),
            (StrengthExercise::InclineBenchPress, 0.56),
        ];
        assert_eq!(pick_nearest(&scored), None);
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }
}
