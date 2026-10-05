use anyhow::{Result, anyhow};
use base64::Engine as _;
use serde_json::{Value, json};
use teloxide::payloads::SendPhotoSetters;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, InputFile};
use tracing::warn;

use crate::ai::context::AiContext;
use crate::ai::gemini_error;
use crate::db::Db;
use crate::i18n::Lang;

const IMAGE_SYSTEM_PROMPT: &str = "\
You generate a single ceremonial sports image for a coach bot.
Style: poster, official communiqué, statue, or good-natured sarcastic edit.
Stick strictly to the fact given. Do not invent numbers, PRs, meals, or sessions.
No humiliation, no sexualization, no gore, no real-person likeness beyond the supplied photo.
Never generate or edit a body, physique, selfie, or before/after progress picture.
Disproportionate solemnity is welcome; cruelty is not.";

const GATE_SYSTEM_PROMPT: &str = "\
You decide if a ceremonial sports poster is worth generating (costly).
If the fact is routine, thin, or not visually interesting, reply with exactly SKIP.
Otherwise reply with ONE English visual prompt for a solemn athletic poster.
Do not invent numbers. No humiliation. No sexualization.
Never ask for or generate a body, physique, selfie, or before/after progress picture.";

#[derive(Debug, Clone, Copy)]
pub struct ImageUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
}

#[derive(Debug, Clone)]
pub struct CoachImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub usage: ImageUsage,
}

fn gemini_url(model: &str, api_key: &str) -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
    )
}

#[derive(Debug, Clone, Copy)]
pub struct ImageQuota {
    pub max_per_day: i64,
    pub used_today: i64,
    pub remaining_today: i64,
}

impl ImageQuota {
    pub fn to_json(self) -> serde_json::Value {
        json!({
            "max_per_day": self.max_per_day,
            "used_today": self.used_today,
            "remaining_today": self.remaining_today,
            "resets": "UTC midnight",
        })
    }
}

/// Used / remaining coach images for this user today (UTC). Remaining is never negative.
pub async fn quota_today(
    pool: &Db,
    user_id: i64,
    model: &str,
    max_per_day: i64,
) -> Result<ImageQuota> {
    let used = crate::db::ai_usage::count_today_for_model(pool, user_id, model).await?;
    let remaining = if max_per_day <= 0 {
        0
    } else {
        (max_per_day - used).max(0)
    };
    Ok(ImageQuota {
        max_per_day,
        used_today: used,
        remaining_today: remaining,
    })
}

/// Remaining image slots for this user today (UTC). Never negative.
pub async fn remaining_today(
    pool: &Db,
    user_id: i64,
    model: &str,
    max_per_day: i64,
) -> Result<i64> {
    Ok(quota_today(pool, user_id, model, max_per_day)
        .await?
        .remaining_today)
}

/// Line injected into the /ask system prompt so the coach can answer quota questions.
pub fn format_quota_for_prompt(quota: ImageQuota, images_enabled: bool) -> String {
    if !images_enabled {
        return "Coach images are disabled (no Gemini token). If asked, say so.".into();
    }
    if quota.max_per_day <= 0 {
        return "Coach images are disabled (daily max is 0). If asked, say so.".into();
    }
    format!(
        "Coach image quota this UTC day: {remaining} remaining of {max} (used {used}). \
         If the user asks about the image limit, state these numbers. \
         Do not call send_coach_image merely to check quota.",
        remaining = quota.remaining_today,
        max = quota.max_per_day,
        used = quota.used_today,
    )
}

/// Always insert a usage row so image quota works even when Gemini reports 0 tokens.
pub async fn log_image_usage(
    pool: &Db,
    user_id: i64,
    model: &str,
    usage: &ImageUsage,
    request_id: Option<&str>,
) {
    crate::ai::log::persist_child(
        pool,
        user_id,
        model,
        crate::db::ai_usage::UsageTokens {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            image_tokens: 0,
        },
        crate::db::ai_usage::KIND_IMAGE,
        request_id,
    )
    .await;
}

pub fn extract_inline_image(response: &Value) -> Result<(Vec<u8>, String)> {
    let candidates = response
        .get("candidates")
        .and_then(|c| c.as_array())
        .ok_or_else(|| anyhow!("No candidates in Gemini image response"))?;
    for cand in candidates {
        let Some(parts) = cand
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
        else {
            continue;
        };
        for part in parts {
            let inline = part.get("inlineData").or_else(|| part.get("inline_data"));
            let Some(inline) = inline else {
                continue;
            };
            let Some(data) = inline.get("data").and_then(|d| d.as_str()) else {
                continue;
            };
            let mime = inline
                .get("mimeType")
                .or_else(|| inline.get("mime_type"))
                .and_then(|m| m.as_str())
                .unwrap_or("image/jpeg")
                .to_string();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| anyhow!("Invalid image base64: {e}"))?;
            if !bytes.is_empty() {
                return Ok((bytes, mime));
            }
        }
    }
    Err(anyhow!("No inline image data in Gemini response"))
}

fn json_i64(meta: Option<&Value>, camel: &str, snake: &str) -> i64 {
    meta.and_then(|m| m.get(camel).or_else(|| m.get(snake)))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

pub fn extract_usage(response: &Value) -> ImageUsage {
    let meta = response
        .get("usageMetadata")
        .or_else(|| response.get("usage_metadata"));
    let prompt = json_i64(meta, "promptTokenCount", "prompt_token_count");
    let candidates = json_i64(meta, "candidatesTokenCount", "candidates_token_count");
    let thoughts = json_i64(meta, "thoughtsTokenCount", "thoughts_token_count");
    let total = json_i64(meta, "totalTokenCount", "total_token_count");
    let cached = json_i64(
        meta,
        "cachedContentTokenCount",
        "cached_content_token_count",
    );
    let (prompt_tokens, completion_tokens, total_tokens) =
        crate::ai::pricing::billed_tokens(prompt, candidates, thoughts, total);
    ImageUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cached_tokens: cached.max(0).min(prompt_tokens),
    }
}

/// `SKIP` (any case, optionally with trailing text) → None; otherwise first line.
pub fn parse_celebration_gate_reply(text: &str) -> Option<String> {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let head = first
        .split_whitespace()
        .next()
        .unwrap_or(first)
        .trim_end_matches(['.', ',', ':', ';']);
    if head.eq_ignore_ascii_case("SKIP") {
        return None;
    }
    Some(first.to_string())
}

fn extract_text(response: &Value) -> String {
    let mut out = String::new();
    let Some(candidates) = response.get("candidates").and_then(|c| c.as_array()) else {
        return out;
    };
    for cand in candidates {
        let Some(parts) = cand
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
        else {
            continue;
        };
        for part in parts {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
    }
    out
}

pub async fn generate_coach_image(
    api_key: &str,
    model: &str,
    prompt: &str,
    source_image: Option<(&[u8], &str)>,
    alert: Option<(&Db, &str, i64)>,
) -> Result<CoachImage> {
    if crate::ai::gemini_circuit::is_open() {
        return Err(crate::ai::gemini_circuit::blocked_error());
    }
    let mut parts = vec![json!({ "text": prompt })];
    if let Some((bytes, mime)) = source_image {
        parts.push(json!({
            "inlineData": {
                "mimeType": mime,
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            }
        }));
    }

    let request = json!({
        "systemInstruction": {
            "parts": [{ "text": IMAGE_SYSTEM_PROMPT }]
        },
        "contents": [{
            "role": "user",
            "parts": parts
        }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(gemini_url(model, api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if let Some((pool, bot_token, user_id)) = alert {
            gemini_error::record_and_maybe_alert(
                pool,
                bot_token,
                model,
                "image",
                Some(user_id),
                status,
                &body,
            );
        } else {
            let _ = crate::ai::gemini_circuit::record_http(status.as_u16(), None, &body);
        }
        return Err(anyhow!("Gemini image API error: {status} - {body}"));
    }
    crate::ai::gemini_circuit::note_success();

    let body: Value = response.json().await?;
    let (bytes, mime_type) = extract_inline_image(&body)?;
    Ok(CoachImage {
        bytes,
        mime_type,
        usage: extract_usage(&body),
    })
}

async fn decide_celebration_prompt(
    pool: &Db,
    user_id: i64,
    api_key: &str,
    summarize_model: &str,
    fact: &str,
    alert: Option<(&Db, &str, i64)>,
) -> Result<Option<String>> {
    if crate::ai::gemini_circuit::is_open() {
        return Err(crate::ai::gemini_circuit::blocked_error());
    }
    let request = json!({
        "systemInstruction": {
            "parts": [{ "text": GATE_SYSTEM_PROMPT }]
        },
        "contents": [{
            "role": "user",
            "parts": [{ "text": format!("Fact: {fact}") }]
        }]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(gemini_url(summarize_model, api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if let Some((pool, bot_token, user_id)) = alert {
            gemini_error::record_and_maybe_alert(
                pool,
                bot_token,
                summarize_model,
                "image_gate",
                Some(user_id),
                status,
                &body,
            );
        } else {
            let _ = crate::ai::gemini_circuit::record_http(status.as_u16(), None, &body);
        }
        return Err(anyhow!("Gemini gate API error: {status} - {body}"));
    }
    crate::ai::gemini_circuit::note_success();

    let body: Value = response.json().await?;
    let usage = extract_usage(&body);
    crate::ai::log::persist_child(
        pool,
        user_id,
        summarize_model,
        crate::db::ai_usage::UsageTokens {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            image_tokens: 0,
        },
        crate::db::ai_usage::KIND_IMAGE_GATE,
        None,
    )
    .await;
    Ok(parse_celebration_gate_reply(&extract_text(&body)))
}

/// After the achievement text is sent: cheap gate, then maybe one image. Never blocks the handler.
pub fn spawn_achievement_celebration(
    bot: teloxide::Bot,
    pool: Db,
    ai_ctx: AiContext,
    chat_id: ChatId,
    user_id: i64,
    lang: Lang,
    fact: String,
    caption: String,
) {
    let Some(api_key) = ai_ctx.api_key.clone() else {
        return;
    };
    if ai_ctx.image_max_per_user_day <= 0 {
        return;
    }

    tokio::spawn(async move {
        match remaining_today(
            &pool,
            user_id,
            &ai_ctx.image_model,
            ai_ctx.image_max_per_user_day,
        )
        .await
        {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                warn!(user_id = %user_id, "Image quota check failed: {e}");
                return;
            }
        }

        let prompt = match decide_celebration_prompt(
            &pool,
            user_id,
            &api_key,
            &ai_ctx.summarize_model,
            &fact,
            Some((&pool, &ai_ctx.bot_token, user_id)),
        )
        .await
        {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(e) => {
                warn!(user_id = %user_id, "Achievement image gate failed: {e}");
                return;
            }
        };

        match remaining_today(
            &pool,
            user_id,
            &ai_ctx.image_model,
            ai_ctx.image_max_per_user_day,
        )
        .await
        {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                warn!(user_id = %user_id, "Image quota re-check failed: {e}");
                return;
            }
        }

        let scene = format!(
            "Ceremonial sports poster. Fact (do not invent extras): {fact}. Visual: {prompt}"
        );
        let image = match generate_coach_image(
            &api_key,
            &ai_ctx.image_model,
            &scene,
            None,
            Some((&pool, &ai_ctx.bot_token, user_id)),
        )
        .await
        {
            Ok(img) => img,
            Err(e) => {
                warn!(user_id = %user_id, "Achievement image generation failed: {e}");
                return;
            }
        };

        log_image_usage(&pool, user_id, &ai_ctx.image_model, &image.usage, None).await;

        let filename = if image.mime_type.contains("png") {
            "coach.png"
        } else {
            "coach.jpg"
        };
        let caption = if caption.chars().count() > 200 {
            default_caption(lang)
        } else {
            caption
        };
        if let Err(e) = bot
            .send_photo(chat_id, InputFile::memory(image.bytes).file_name(filename))
            .caption(caption)
            .await
        {
            warn!(user_id = %user_id, "Failed to send achievement image: {e}");
        }
    });
}

fn default_caption(lang: Lang) -> String {
    match lang {
        Lang::Fr => "Objectif atteint. Affiche officielle.".into(),
        Lang::En => "Goal reached. Official poster.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny 1×1 JPEG.
    const TINY_JPEG: &[u8] = &[
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b,
        0x0c, 0x19, 0x12, 0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20,
        0x24, 0x2e, 0x27, 0x20, 0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1f, 0x27, 0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff,
        0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xff, 0xc4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00, 0x08,
        0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x7f, 0x3f, 0xff, 0xd9,
    ];

    #[test]
    fn standard_engine_roundtrips_jpeg_bytes() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(TINY_JPEG);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(decoded, TINY_JPEG);
        assert!(
            b64.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        );
        assert!(!b64.contains('-') && !b64.contains('_'));
    }

    #[test]
    fn parse_fake_gemini_image_response() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(TINY_JPEG);
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        { "text": "here you go" },
                        {
                            "inlineData": {
                                "mimeType": "image/jpeg",
                                "data": b64
                            }
                        }
                    ]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "candidatesTokenCount": 0,
                "totalTokenCount": 12
            }
        });
        let (bytes, mime) = extract_inline_image(&response).unwrap();
        assert_eq!(bytes, TINY_JPEG);
        assert_eq!(mime, "image/jpeg");
        let usage = extract_usage(&response);
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.total_tokens, 12);
        assert_eq!(usage.cached_tokens, 0);
    }

    #[test]
    fn extract_usage_reads_cached_tokens() {
        let response = json!({
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 20,
                "totalTokenCount": 120,
                "cachedContentTokenCount": 80
            }
        });
        let usage = extract_usage(&response);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.cached_tokens, 80);
    }

    #[test]
    fn parse_snake_case_inline_data() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(TINY_JPEG);
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "inline_data": { "mime_type": "image/png", "data": b64 }
                    }]
                }
            }]
        });
        let (bytes, mime) = extract_inline_image(&response).unwrap();
        assert_eq!(bytes, TINY_JPEG);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn gate_skip_variants() {
        assert!(parse_celebration_gate_reply("SKIP").is_none());
        assert!(parse_celebration_gate_reply("skip").is_none());
        assert!(parse_celebration_gate_reply("  SKIP: too routine  ").is_none());
        assert!(parse_celebration_gate_reply("").is_none());
    }

    #[test]
    fn gate_keeps_visual_prompt() {
        let p = parse_celebration_gate_reply(
            "A bronze statue of a runner under stadium lights, official poster.",
        )
        .unwrap();
        assert!(p.to_lowercase().contains("statue"));
    }

    #[test]
    fn quota_prompt_states_remaining_and_max() {
        let line = format_quota_for_prompt(
            ImageQuota {
                max_per_day: 2,
                used_today: 1,
                remaining_today: 1,
            },
            true,
        );
        assert!(line.contains("1 remaining of 2"));
        assert!(line.contains("used 1"));
    }

    #[test]
    fn quota_prompt_disabled_without_token() {
        let line = format_quota_for_prompt(
            ImageQuota {
                max_per_day: 2,
                used_today: 0,
                remaining_today: 2,
            },
            false,
        );
        assert!(line.to_lowercase().contains("disabled"));
    }
}
