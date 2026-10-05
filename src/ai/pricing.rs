//! Paid-tier Standard Gemini list prices (USD), used to estimate spend.
//!
//! Rates follow Google's Gemini Developer API pricing page (Standard, not
//! Batch/Flex/Priority). Grounding search, audio, and free-tier usage are not
//! modeled. Context-cache hits and embedding IMAGE tokens are billed from
//! `usageMetadata` (`cachedContentTokenCount`, `promptTokensDetails`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::db::ai_usage::{ModelUsageRow, UserModelUsageRow};

/// Per-1M-token Standard rates, plus per-request fallbacks when Gemini
/// reports 0 tokens (typical for image generate/embed).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelRates {
    pub input_per_million: f64,
    pub output_per_million: f64,
    /// Context-cache hits (`cachedContentTokenCount`), Standard tier.
    pub cache_per_million: f64,
    /// Charged per `ai_usage` row when this is an image-gen model and
    /// `completion_tokens == 0` (Gemini often omits image output tokens).
    pub image_usd_if_no_output: f64,
    /// Charged per row when this is an embedding model and `prompt_tokens == 0`
    /// (multimodal image embed with no usageMetadata).
    pub embedding_image_usd: f64,
    /// Embedding IMAGE/DOCUMENT tokens (`promptTokenDetails`), Standard tier.
    pub embedding_image_per_million: f64,
}

impl ModelRates {
    const fn chat(input_per_million: f64, output_per_million: f64, cache_per_million: f64) -> Self {
        Self {
            input_per_million,
            output_per_million,
            cache_per_million,
            image_usd_if_no_output: 0.0,
            embedding_image_usd: 0.0,
            embedding_image_per_million: 0.0,
        }
    }

    const fn image_gen(
        input_per_million: f64,
        output_per_million: f64,
        image_usd_if_no_output: f64,
    ) -> Self {
        Self {
            input_per_million,
            output_per_million,
            cache_per_million: 0.0,
            image_usd_if_no_output,
            embedding_image_usd: 0.0,
            embedding_image_per_million: 0.0,
        }
    }

    const fn embedding(
        input_per_million: f64,
        embedding_image_per_million: f64,
        embedding_image_usd: f64,
    ) -> Self {
        Self {
            input_per_million,
            output_per_million: 0.0,
            cache_per_million: 0.0,
            image_usd_if_no_output: 0.0,
            embedding_image_usd,
            embedding_image_per_million,
        }
    }
}

/// Split usage into billed input / output tokens.
///
/// Gemini reports thinking tokens separately (`thoughts_token_count`). Output
/// pricing includes them. Prefer `total - prompt` when `total` is consistent.
pub fn billed_tokens(prompt: i64, candidates: i64, thoughts: i64, total: i64) -> (i64, i64, i64) {
    let prompt = prompt.max(0);
    let candidates = candidates.max(0);
    let thoughts = thoughts.max(0);
    let total = total.max(0);
    let from_total = if total >= prompt { total - prompt } else { 0 };
    let from_parts = candidates + thoughts;
    let output = from_total.max(from_parts);
    (prompt, output, total.max(prompt + output))
}

/// Published Standard paid-tier card for a known Gemini model id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelRateCard {
    pub model: String,
    pub rates: ModelRates,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PricingCatalog {
    pub source: &'static str,
    pub models: Vec<ModelRateCard>,
}

/// Exact model ids and their Standard paid rates (Gemini Developer API card).
pub fn catalog() -> Vec<ModelRateCard> {
    let flash_35 = format!("gemini-{}.{}-flash", 3, 5);
    let flash_36 = format!("gemini-{}.{}-flash", 3, 6);
    let flash_37 = format!("gemini-{}.{}-flash", 3, 7);
    let embed_v2 = format!("gemini-embedding-{}", 2);
    vec![
        card("gemini-2.5-flash", ModelRates::chat(0.30, 2.50, 0.03)),
        card("gemini-2.5-flash-lite", ModelRates::chat(0.10, 0.40, 0.01)),
        card("gemini-2.5-pro", ModelRates::chat(1.25, 10.0, 0.125)),
        card("gemini-2.0-flash", ModelRates::chat(0.10, 0.40, 0.0)),
        card("gemini-2.0-flash-lite", ModelRates::chat(0.075, 0.30, 0.0)),
        card(
            "gemini-2.5-flash-image",
            ModelRates::image_gen(0.30, 30.0, 0.039),
        ),
        card(
            "gemini-2.5-flash-image-preview",
            ModelRates::image_gen(0.30, 30.0, 0.039),
        ),
        card(&flash_35, ModelRates::chat(1.50, 9.00, 0.15)),
        card(
            &format!("{flash_35}-lite"),
            ModelRates::chat(0.30, 2.50, 0.03),
        ),
        // 3.6 / 3.7 Flash: promo Standard rates through 2026-12-31.
        card(&flash_36, ModelRates::chat(0.75, 3.75, 0.075)),
        card(&flash_37, ModelRates::chat(0.75, 3.75, 0.075)),
        card(
            &format!("gemini-{}.{}-flash-lite", 3, 1),
            ModelRates::chat(0.25, 1.50, 0.025),
        ),
        card(&embed_v2, ModelRates::embedding(0.20, 0.45, 0.00012)),
        card(
            "gemini-embedding-001",
            ModelRates::embedding(0.15, 0.0, 0.0),
        ),
    ]
}

pub fn pricing_catalog() -> PricingCatalog {
    PricingCatalog {
        source: "Gemini Developer API Standard paid tier (not Batch/Flex/Priority)",
        models: catalog(),
    }
}

fn card(model: &str, rates: ModelRates) -> ModelRateCard {
    ModelRateCard {
        model: model.to_string(),
        rates,
    }
}

pub fn rates_for(model: &str) -> ModelRates {
    let m = normalize_model(model);
    if let Some(card) = catalog()
        .into_iter()
        .find(|c| normalize_model(&c.model) == m)
    {
        return card.rates;
    }
    family_rates(&m)
}

fn family_rates(m: &str) -> ModelRates {
    if m.contains("embedding-001") {
        return ModelRates::embedding(0.15, 0.0, 0.0);
    }
    if m.contains("embedding") {
        // Multimodal embedding: $0.20/1M text, $0.45/1M image
        return ModelRates::embedding(0.20, 0.45, 0.00012);
    }

    if m.contains("image") {
        if m.contains("lite") {
            return ModelRates::image_gen(0.25, 30.0, 0.0336);
        }
        if m.contains("3.1") {
            return ModelRates::image_gen(0.50, 60.0, 0.067);
        }
        return ModelRates::image_gen(0.30, 30.0, 0.039);
    }

    if m.contains("lite") {
        if m.contains("3.5") {
            return ModelRates::chat(0.30, 2.50, 0.03);
        }
        if m.contains("3.1") || m.starts_with("gemini-3") {
            return ModelRates::chat(0.25, 1.50, 0.025);
        }
        if m.contains("2.0") {
            return ModelRates::chat(0.075, 0.30, 0.0);
        }
        return ModelRates::chat(0.10, 0.40, 0.01);
    }

    if m.contains("pro") {
        return ModelRates::chat(1.25, 10.0, 0.125);
    }

    // 3.6 / 3.7 Flash: promo Standard rates through 2026-12-31.
    if m.contains("3.7") || m.contains("3.6") {
        return ModelRates::chat(0.75, 3.75, 0.075);
    }
    if m.contains("3.5") || m.starts_with("gemini-3") {
        return ModelRates::chat(1.50, 9.00, 0.15);
    }
    if m.contains("2.0") {
        return ModelRates::chat(0.10, 0.40, 0.0);
    }
    ModelRates::chat(0.30, 2.50, 0.03)
}

fn normalize_model(model: &str) -> String {
    model
        .trim()
        .trim_start_matches("models/")
        .to_ascii_lowercase()
}

pub fn estimate_usd(
    model: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
    image_tokens: i64,
    zero_output_requests: i64,
    zero_token_requests: i64,
) -> f64 {
    let rates = rates_for(model);
    let prompt = prompt_tokens.max(0);
    let completion = completion_tokens.max(0);
    let cached = cached_tokens.max(0).min(prompt);
    let image = image_tokens.max(0);
    let uncached = prompt - cached;
    let text_embed = (prompt - image).max(0);

    let mut usd = if rates.embedding_image_per_million > 0.0 || rates.embedding_image_usd > 0.0 {
        (text_embed as f64 / 1_000_000.0) * rates.input_per_million
            + (image as f64 / 1_000_000.0) * rates.embedding_image_per_million
    } else {
        (uncached as f64 / 1_000_000.0) * rates.input_per_million
            + (cached as f64 / 1_000_000.0) * rates.cache_per_million
            + (completion as f64 / 1_000_000.0) * rates.output_per_million
    };
    if rates.image_usd_if_no_output > 0.0 {
        usd += zero_output_requests.max(0) as f64 * rates.image_usd_if_no_output;
    }
    if rates.embedding_image_usd > 0.0 && prompt <= 0 && image <= 0 {
        usd += zero_token_requests.max(0) as f64 * rates.embedding_image_usd;
    }
    usd
}

/// Estimated USD for one generateContent / embed call.
pub fn estimate_call_usd(
    model: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
) -> f64 {
    estimate_tokens_usd(
        model,
        &crate::db::ai_usage::UsageTokens {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.max(0) + completion_tokens.max(0),
            cached_tokens,
            image_tokens: 0,
        },
    )
}

pub fn estimate_tokens_usd(model: &str, tokens: &crate::db::ai_usage::UsageTokens) -> f64 {
    let zero_output = i64::from(tokens.completion_tokens <= 0);
    let zero_token = i64::from(tokens.prompt_tokens <= 0 && tokens.completion_tokens <= 0);
    estimate_usd(
        model,
        tokens.prompt_tokens,
        tokens.completion_tokens,
        tokens.cached_tokens,
        tokens.image_tokens,
        zero_output,
        zero_token,
    )
}

/// Cached share of prompt tokens, 0–100.
pub fn cache_hit_pct(prompt_tokens: i64, cached_tokens: i64) -> f64 {
    if prompt_tokens <= 0 {
        0.0
    } else {
        (cached_tokens.max(0) as f64 / prompt_tokens as f64) * 100.0
    }
}

/// What the call would have cost if every prompt token were billed at the input rate.
pub fn cache_usd_saved(
    model: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
) -> f64 {
    if cached_tokens <= 0 {
        return 0.0;
    }
    let with_cache = estimate_call_usd(model, prompt_tokens, completion_tokens, cached_tokens);
    let without = estimate_call_usd(model, prompt_tokens, completion_tokens, 0);
    (without - with_cache).max(0.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpend {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub request_count: i64,
    pub usd: f64,
    pub cache_usd_saved: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KindSpend {
    pub kind: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub request_count: i64,
    pub usd: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpendSummary {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub request_count: i64,
    pub usd: f64,
    pub cache_usd_saved: f64,
    pub by_model: Vec<ModelSpend>,
    pub by_kind: Vec<KindSpend>,
}

impl SpendSummary {
    fn from_model_spends(mut by_model: Vec<ModelSpend>) -> Self {
        by_model.sort_by(|a, b| {
            b.usd
                .partial_cmp(&a.usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.model.cmp(&b.model))
        });
        let prompt_tokens = by_model.iter().map(|m| m.prompt_tokens).sum();
        let completion_tokens = by_model.iter().map(|m| m.completion_tokens).sum();
        let total_tokens = by_model.iter().map(|m| m.total_tokens).sum();
        let cached_tokens = by_model.iter().map(|m| m.cached_tokens).sum();
        let request_count = by_model.iter().map(|m| m.request_count).sum();
        let usd = by_model.iter().map(|m| m.usd).sum();
        let cache_usd_saved = by_model.iter().map(|m| m.cache_usd_saved).sum();
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            request_count,
            usd,
            cache_usd_saved,
            by_model,
            by_kind: Vec::new(),
        }
    }

    pub fn with_kinds(mut self, mut by_kind: Vec<KindSpend>) -> Self {
        by_kind.sort_by(|a, b| {
            b.usd
                .partial_cmp(&a.usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.kind.cmp(&b.kind))
        });
        self.by_kind = by_kind;
        self
    }
}

fn model_spend(row: &ModelUsageRow) -> ModelSpend {
    let usd = estimate_usd(
        &row.model,
        row.prompt_tokens,
        row.completion_tokens,
        row.cached_tokens,
        row.image_tokens,
        row.zero_output_requests,
        row.zero_token_requests,
    );
    ModelSpend {
        model: row.model.clone(),
        prompt_tokens: row.prompt_tokens,
        completion_tokens: row.completion_tokens,
        total_tokens: row.total_tokens,
        cached_tokens: row.cached_tokens,
        request_count: row.request_count,
        usd,
        cache_usd_saved: cache_usd_saved(
            &row.model,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
        ),
    }
}

pub fn summarize_kinds(rows: &[crate::db::ai_usage::KindUsageRow]) -> Vec<KindSpend> {
    let mut by_kind: HashMap<String, KindSpend> = HashMap::new();
    for row in rows {
        let usd = estimate_usd(
            &row.model,
            row.prompt_tokens,
            row.completion_tokens,
            row.cached_tokens,
            row.image_tokens,
            row.zero_output_requests,
            row.zero_token_requests,
        );
        let entry = by_kind
            .entry(row.kind.clone())
            .or_insert_with(|| KindSpend {
                kind: row.kind.clone(),
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cached_tokens: 0,
                request_count: 0,
                usd: 0.0,
            });
        entry.prompt_tokens += row.prompt_tokens;
        entry.completion_tokens += row.completion_tokens;
        entry.total_tokens += row.total_tokens;
        entry.cached_tokens += row.cached_tokens;
        entry.request_count += row.request_count;
        entry.usd += usd;
    }
    let mut kinds: Vec<KindSpend> = by_kind.into_values().collect();
    kinds.sort_by(|a, b| {
        b.usd
            .partial_cmp(&a.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.kind.cmp(&b.kind))
    });
    kinds
}

pub fn summarize(rows: &[ModelUsageRow]) -> SpendSummary {
    SpendSummary::from_model_spends(rows.iter().map(model_spend).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedUser {
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub spend: SpendSummary,
}

pub fn rank_users(rows: &[UserModelUsageRow], limit: usize) -> Vec<RankedUser> {
    let mut by_user: HashMap<i64, (Option<String>, Vec<ModelSpend>)> = HashMap::new();
    for row in rows {
        let entry = by_user
            .entry(row.telegram_user_id)
            .or_insert_with(|| (row.username.clone(), Vec::new()));
        if entry.0.is_none() {
            entry.0 = row.username.clone();
        }
        entry.1.push(model_spend(&ModelUsageRow {
            model: row.model.clone(),
            prompt_tokens: row.prompt_tokens,
            completion_tokens: row.completion_tokens,
            total_tokens: row.total_tokens,
            request_count: row.request_count,
            cached_tokens: row.cached_tokens,
            image_tokens: row.image_tokens,
            zero_output_requests: row.zero_output_requests,
            zero_token_requests: row.zero_token_requests,
        }));
    }

    let mut ranked: Vec<RankedUser> = by_user
        .into_iter()
        .map(|(telegram_user_id, (username, models))| RankedUser {
            telegram_user_id,
            username,
            spend: SpendSummary::from_model_spends(models),
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.spend
            .usd
            .partial_cmp(&a.spend.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.telegram_user_id.cmp(&b.telegram_user_id))
    });
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flash_35() -> String {
        format!("gemini-{}.{}-flash", 3, 5)
    }

    fn flash_35_lite() -> String {
        format!("{}-lite", flash_35())
    }

    fn embed_v2() -> String {
        format!("gemini-embedding-{}", 2)
    }

    fn row(model: &str, prompt: i64, completion: i64, requests: i64) -> ModelUsageRow {
        ModelUsageRow {
            model: model.to_string(),
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            request_count: requests,
            cached_tokens: 0,
            image_tokens: 0,
            zero_output_requests: if completion <= 0 { requests } else { 0 },
            zero_token_requests: if prompt <= 0 && completion <= 0 {
                requests
            } else {
                0
            },
        }
    }

    fn user_row(
        tg: i64,
        name: &str,
        model: &str,
        prompt: i64,
        completion: i64,
        requests: i64,
    ) -> UserModelUsageRow {
        UserModelUsageRow {
            telegram_user_id: tg,
            username: Some(name.to_string()),
            model: model.to_string(),
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            request_count: requests,
            cached_tokens: 0,
            image_tokens: 0,
            zero_output_requests: if completion <= 0 { requests } else { 0 },
            zero_token_requests: if prompt <= 0 && completion <= 0 {
                requests
            } else {
                0
            },
        }
    }

    #[test]
    fn billed_tokens_includes_thinking_via_total() {
        assert_eq!(billed_tokens(10, 20, 15, 45), (10, 35, 45));
    }

    #[test]
    fn billed_tokens_falls_back_to_candidates_plus_thoughts() {
        assert_eq!(billed_tokens(10, 20, 15, 0), (10, 35, 45));
    }

    #[test]
    fn billed_tokens_keeps_thoughts_when_total_omits_them() {
        assert_eq!(billed_tokens(10, 20, 15, 30), (10, 35, 45));
    }

    #[test]
    fn rates_match_published_standard_card() {
        assert_eq!(
            rates_for("gemini-2.5-flash"),
            ModelRates::chat(0.30, 2.50, 0.03)
        );
        assert_eq!(
            rates_for("gemini-2.5-flash-lite"),
            ModelRates::chat(0.10, 0.40, 0.01)
        );
        assert_eq!(rates_for(&flash_35()), ModelRates::chat(1.50, 9.00, 0.15));
        assert_eq!(
            rates_for(&flash_35_lite()),
            ModelRates::chat(0.30, 2.50, 0.03)
        );
        assert_eq!(
            rates_for("gemini-2.5-pro"),
            ModelRates::chat(1.25, 10.0, 0.125)
        );
        assert_eq!(
            rates_for("models/gemini-3.6-flash"),
            ModelRates::chat(0.75, 3.75, 0.075)
        );
        assert_eq!(
            rates_for("gemini-3.7-flash"),
            ModelRates::chat(0.75, 3.75, 0.075)
        );
        let image = rates_for("gemini-2.5-flash-image");
        assert_eq!(image.input_per_million, 0.30);
        assert_eq!(image.output_per_million, 30.0);
        assert_eq!(image.image_usd_if_no_output, 0.039);
        let embed = rates_for(&embed_v2());
        assert_eq!(embed.input_per_million, 0.20);
        assert_eq!(embed.embedding_image_usd, 0.00012);
        assert!(catalog().iter().any(
            |c| c.model == "gemini-2.5-flash" && c.rates == ModelRates::chat(0.30, 2.50, 0.03)
        ));
    }

    #[test]
    fn exact_catalog_id_is_not_confused_with_family() {
        let lite = format!("gemini-{}.{}-flash-lite", 3, 5);
        let flash = format!("gemini-{}.{}-flash", 3, 5);
        assert_eq!(rates_for(&lite), ModelRates::chat(0.30, 2.50, 0.03));
        assert_eq!(rates_for(&flash), ModelRates::chat(1.50, 9.00, 0.15));
        assert_ne!(rates_for(&lite), rates_for(&flash));
    }

    #[test]
    fn catalog_lists_exact_rate_for_every_known_id() {
        let flash_lite_31 = format!("gemini-{}.{}-flash-lite", 3, 1);
        let flash_36 = format!("gemini-{}.{}-flash", 3, 6);
        let flash_37 = format!("gemini-{}.{}-flash", 3, 7);
        assert_eq!(
            rates_for(&flash_lite_31),
            ModelRates::chat(0.25, 1.50, 0.025)
        );
        assert!(catalog().iter().any(|c| c.model == flash_lite_31));
        assert_eq!(rates_for(&flash_36), ModelRates::chat(0.75, 3.75, 0.075));
        assert_eq!(rates_for(&flash_37), ModelRates::chat(0.75, 3.75, 0.075));
        assert_eq!(catalog().len(), pricing_catalog().models.len());
        assert!(catalog().len() >= 12);
    }

    #[test]
    fn estimate_call_usd_matches_one_request() {
        let usd = estimate_call_usd("gemini-2.5-flash", 1_000_000, 1_000_000, 0);
        assert!((usd - 2.80).abs() < 1e-9);
        let cached = estimate_call_usd("gemini-2.5-flash", 1_000_000, 0, 1_000_000);
        assert!((cached - 0.03).abs() < 1e-9);
    }

    #[test]
    fn cache_hit_pct_and_usd_saved() {
        assert!((cache_hit_pct(1_000_000, 400_000) - 40.0).abs() < 1e-9);
        assert_eq!(cache_hit_pct(0, 10), 0.0);
        let saved = cache_usd_saved("gemini-2.5-flash", 1_000_000, 0, 1_000_000);
        assert!((saved - 0.27).abs() < 1e-9);
    }

    fn est(
        model: &str,
        prompt: i64,
        completion: i64,
        cached: i64,
        image: i64,
        zero_output: i64,
        zero_token: i64,
    ) -> f64 {
        estimate_usd(
            model,
            prompt,
            completion,
            cached,
            image,
            zero_output,
            zero_token,
        )
    }

    #[test]
    fn lite_is_not_billed_as_full_flash() {
        let lite = est("gemini-2.5-flash-lite", 1_000_000, 1_000_000, 0, 0, 0, 0);
        let flash = est("gemini-2.5-flash", 1_000_000, 1_000_000, 0, 0, 0, 0);
        let flash35 = est(&flash_35(), 1_000_000, 1_000_000, 0, 0, 0, 0);
        assert!((lite - 0.50).abs() < 1e-9);
        assert!((flash - 2.80).abs() < 1e-9);
        assert!((flash35 - 10.50).abs() < 1e-9);
    }

    #[test]
    fn cached_prompt_tokens_use_cache_rate() {
        let full = est("gemini-2.5-flash", 1_000_000, 0, 0, 0, 0, 0);
        let cached = est("gemini-2.5-flash", 1_000_000, 0, 1_000_000, 0, 0, 0);
        assert!((full - 0.30).abs() < 1e-9);
        assert!((cached - 0.03).abs() < 1e-9);
        assert!(cached < full);
    }

    #[test]
    fn zero_token_image_rows_still_cost() {
        let usd = est("gemini-2.5-flash-image", 0, 0, 0, 0, 2, 2);
        assert!((usd - 0.078).abs() < 1e-9);
    }

    #[test]
    fn image_output_tokens_are_not_double_counted() {
        // 1290 tokens × $30/1M ≈ $0.0387; no extra $0.039 because completion > 0
        let usd = est("gemini-2.5-flash-image", 0, 1290, 0, 0, 0, 0);
        assert!((usd - 0.0387).abs() < 1e-9);
    }

    #[test]
    fn image_embed_with_no_tokens_uses_per_image_rate() {
        let usd = est(&embed_v2(), 0, 0, 0, 0, 1, 1);
        assert!((usd - 0.00012).abs() < 1e-12);
    }

    #[test]
    fn image_embed_modality_tokens_use_image_rate() {
        // 800k text × $0.20 + 200k image × $0.45
        let usd = est(&embed_v2(), 1_000_000, 0, 0, 200_000, 0, 0);
        assert!((usd - 0.25).abs() < 1e-9);
    }

    #[test]
    fn rank_by_estimated_usd_not_token_count() {
        // Cheap-model heavy user vs expensive-model light user.
        let flash = flash_35();
        let lite_heavy = user_row(1, "lite", "gemini-2.5-flash-lite", 10_000_000, 0, 100);
        let flash_light = user_row(2, "flash", &flash, 1_000_000, 100_000, 5);
        let ranked = rank_users(&[lite_heavy, flash_light], 10);
        assert_eq!(ranked[0].username.as_deref(), Some("flash"));
        assert!(ranked[0].spend.usd > ranked[1].spend.usd);
        // lite: 10M input × $0.10 = $1.00
        // flash: 1M × $1.50 + 0.1M × $9 = $2.40
        assert!((ranked[0].spend.usd - 2.40).abs() < 1e-9);
        assert!((ranked[1].spend.usd - 1.00).abs() < 1e-9);
    }

    #[test]
    fn mixed_image_rows_keep_zero_output_fallback() {
        // One billed image (1290 tokens) + one unbilled-token image.
        let usd = est("gemini-2.5-flash-image", 12, 1290, 0, 0, 1, 1);
        let expected = 12.0 / 1_000_000.0 * 0.30 + 1290.0 / 1_000_000.0 * 30.0 + 0.039;
        assert!((usd - expected).abs() < 1e-9);
    }

    #[test]
    fn summarize_breaks_down_per_model() {
        let flash = flash_35();
        let summary = summarize(&[
            row(&flash, 1_000_000, 0, 3),
            row("gemini-2.5-flash-image", 0, 0, 2),
        ]);
        assert_eq!(summary.request_count, 5);
        assert!((summary.usd - (1.50 + 0.078)).abs() < 1e-9);
        assert_eq!(summary.by_model[0].model, flash);
        assert_eq!(summary.by_model[1].model, "gemini-2.5-flash-image");
    }
}
