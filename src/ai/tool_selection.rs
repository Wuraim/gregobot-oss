use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::db::Db;

/// Minimum cosine similarity for a tool to be considered relevant.
/// If no tool reaches this threshold, the caller should fall back to all tools.
const SIMILARITY_THRESHOLD: f32 = 0.25;

/// Maximum number of relevant tools to return (excluding always-included tools).
const TOP_K: usize = 8;

/// Tools always made available regardless of the prompt.
const ALWAYS_INCLUDED: &[&str] = &["create_feature_proposal", "list_capabilities"];

/// Food-intent bundle: if any of these is selected, attach the rest.
/// Do not put these on the global ALWAYS_INCLUDED list.
const FOOD_INTENT: &[&str] = &[
    "propose_meals",
    "get_day_nutrition",
    "search_foods",
    "remember_food_preference",
    "log_meal",
    "register_food",
    "update_food",
    "update_meal",
    "remove_meal",
    "show_nutrition_table",
];

fn is_always_included(name: &str) -> bool {
    ALWAYS_INCLUDED.contains(&name)
}

static TOOL_EMBEDDINGS: OnceLock<HashMap<String, Vec<f32>>> = OnceLock::new();

/// Ensure all tool embeddings are loaded into memory.
///
/// Compares the stored `embed_text` with the current tool definition. If a tool
/// is new or its description changed, the embedding is recomputed via the
/// Gemini embedding API and persisted in the database.
pub async fn ensure_embeddings(pool: &Db, api_key: &str, embedding_model: &str) -> Result<()> {
    let tools = crate::ai::tools::all_tools();
    let mut cache = HashMap::with_capacity(tools.len());

    for tool in tools {
        let stored = crate::db::tool_embeddings::find_by_name(pool, &tool.name).await?;

        match stored {
            Some(row) if row.embed_text == tool.embed_text => {
                let embedding = parse_embedding(&row.embedding)
                    .with_context(|| format!("failed to parse embedding for {}", tool.name))?;
                cache.insert(tool.name, embedding);
            }
            _ => match crate::embeddings::embed(api_key, embedding_model, tool.embed_text).await {
                Ok((embedding, _)) => {
                    let embedding_json = serialize_embedding(&embedding);
                    crate::db::tool_embeddings::upsert(
                        pool,
                        &tool.name,
                        tool.embed_text,
                        &embedding_json,
                    )
                    .await
                    .with_context(|| format!("failed to persist embedding for {}", tool.name))?;
                    cache.insert(tool.name, embedding);
                }
                Err(e) => {
                    tracing::warn!(tool = %tool.name, "Failed to embed tool description: {e}");
                }
            },
        }
    }

    let _ = TOOL_EMBEDDINGS.set(cache);
    Ok(())
}

/// Select the most relevant tools for a user prompt embedding.
///
/// Returns `None` if the cache is empty or no tool reaches the similarity
/// threshold, signalling the caller to fall back to all tools.
/// Always-included tools (`create_feature_proposal`, `list_capabilities`) are
/// appended to every successful selection.
pub fn select_tools(prompt_embedding: &[f32]) -> Option<Vec<String>> {
    let cache = TOOL_EMBEDDINGS.get()?;
    if cache.is_empty() {
        return None;
    }

    let mut scored: Vec<(String, f32)> = cache
        .iter()
        .map(|(name, embedding)| {
            let score = cosine_similarity(prompt_embedding, embedding);
            (name.clone(), score)
        })
        .filter(|(name, _)| !is_always_included(name))
        .collect();

    if scored.is_empty() {
        return None;
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let max_score = scored.first().map(|(_, score)| *score).unwrap_or(0.0);
    if max_score < SIMILARITY_THRESHOLD {
        return None;
    }

    let mut selected: Vec<String> = scored
        .into_iter()
        .take(TOP_K)
        .map(|(name, _)| name)
        .collect();

    for name in ALWAYS_INCLUDED {
        let name = (*name).to_string();
        if !selected.contains(&name) {
            selected.push(name);
        }
    }

    expand_food_intent(&mut selected);

    Some(selected)
}

fn expand_food_intent(selected: &mut Vec<String>) {
    if !selected
        .iter()
        .any(|name| FOOD_INTENT.contains(&name.as_str()))
    {
        return;
    }
    for name in FOOD_INTENT {
        if !selected.iter().any(|s| s == *name) {
            selected.push((*name).to_string());
        }
    }
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
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![0.0f32, 0.0];
        let b = vec![1.0f32, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn serialize_then_parse_embedding() {
        let original = vec![0.1f32, 0.2, -0.3];
        let json = serialize_embedding(&original);
        let parsed = parse_embedding(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_embedding_invalid() {
        assert!(parse_embedding("not json").is_err());
        assert!(parse_embedding("{}").is_err());
    }

    #[test]
    fn food_intent_bundles_related_tools_once_each() {
        let mut selected = vec!["propose_meals".into(), "log_weight".into()];
        expand_food_intent(&mut selected);
        for name in FOOD_INTENT {
            assert!(
                selected.iter().any(|s| s == *name),
                "missing {name} in {selected:?}"
            );
        }
        assert!(selected.contains(&"log_weight".to_string()));
        let mut counts = std::collections::HashMap::new();
        for name in &selected {
            *counts.entry(name.as_str()).or_insert(0) += 1;
        }
        for (name, count) in counts {
            assert_eq!(
                count, 1,
                "tool {name} appears {count} times in {selected:?}"
            );
        }
    }

    #[test]
    fn search_photos_is_not_always_included() {
        assert!(!is_always_included("search_photos"));
        assert!(is_always_included("list_capabilities"));
        assert!(is_always_included("create_feature_proposal"));
    }

    #[test]
    fn food_intent_does_not_attach_without_a_food_tool() {
        let mut selected = vec!["log_weight".into()];
        expand_food_intent(&mut selected);
        assert_eq!(selected, vec!["log_weight".to_string()]);
    }
}
