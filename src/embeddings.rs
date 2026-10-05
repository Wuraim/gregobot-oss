use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Fixed dimension for all embedding vectors. The application uses
/// `gemini-embedding-2` which natively returns 3072-dimensional embeddings.
pub const EMBEDDING_DIMENSION: usize = 3072;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmbedUsage {
    pub prompt_tokens: i64,
    pub image_tokens: i64,
}

/// Rough Gemini-style token estimate when the embed API does not return usage.
pub fn estimated_text_tokens(text: &str) -> i64 {
    (((text.chars().count() as i64) + 3) / 4).max(1)
}

fn estimated_texts_tokens(texts: &[String]) -> i64 {
    texts.iter().map(|t| estimated_text_tokens(t)).sum()
}

fn gemini_url(model: &str, api_key: &str) -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:embedContent?key={api_key}"
    )
}

fn batch_gemini_url(model: &str, api_key: &str) -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:batchEmbedContents?key={api_key}"
    )
}

#[derive(Debug, Serialize)]
struct EmbedContentRequest {
    model: String,
    content: Content,
    #[serde(rename = "outputDimensionality")]
    output_dimensionality: usize,
}

#[derive(Debug, Serialize)]
struct BatchEmbedContentsRequest {
    requests: Vec<EmbedContentRequest>,
}

#[derive(Debug, Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<InlineData>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct EmbedContentResponse {
    embedding: Embedding,
    #[serde(default, rename = "usageMetadata", alias = "usage_metadata")]
    usage_metadata: Option<EmbedUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbedContentsResponse {
    embeddings: Vec<Embedding>,
    #[serde(default, rename = "usageMetadata", alias = "usage_metadata")]
    usage_metadata: Option<EmbedUsageMetadata>,
}

#[derive(Debug, Deserialize, Default)]
struct EmbedUsageMetadata {
    #[serde(default, rename = "promptTokenCount", alias = "prompt_token_count")]
    prompt_token_count: i64,
    #[serde(
        default,
        rename = "promptTokensDetails",
        alias = "prompt_tokens_details"
    )]
    prompt_tokens_details: Vec<ModalityTokenCount>,
}

#[derive(Debug, Deserialize, Default)]
struct ModalityTokenCount {
    #[serde(default)]
    modality: String,
    #[serde(default, rename = "tokenCount", alias = "token_count")]
    token_count: i64,
}

#[derive(Debug, Deserialize)]
struct Embedding {
    values: Vec<f32>,
}

fn is_non_text_modality(modality: &str) -> bool {
    matches!(
        modality.trim().to_ascii_uppercase().as_str(),
        "IMAGE" | "DOCUMENT" | "VIDEO" | "AUDIO"
    )
}

fn usage_from_metadata(meta: Option<&EmbedUsageMetadata>, estimated: i64) -> EmbedUsage {
    let Some(meta) = meta else {
        return EmbedUsage {
            prompt_tokens: estimated,
            image_tokens: 0,
        };
    };
    let image_tokens: i64 = meta
        .prompt_tokens_details
        .iter()
        .filter(|d| is_non_text_modality(&d.modality))
        .map(|d| d.token_count.max(0))
        .sum();
    let prompt = if meta.prompt_token_count > 0 {
        meta.prompt_token_count
    } else {
        image_tokens
    };
    if prompt <= 0 && image_tokens <= 0 {
        return EmbedUsage {
            prompt_tokens: estimated,
            image_tokens: 0,
        };
    }
    EmbedUsage {
        prompt_tokens: prompt,
        image_tokens,
    }
}

fn embed_http_error(prefix: &str, status: reqwest::StatusCode, body: String) -> anyhow::Error {
    let _ = crate::ai::gemini_circuit::record_http(status.as_u16(), None, &body);
    anyhow!("{prefix}: {status} - {body}")
}

fn reject_if_circuit_open() -> Result<()> {
    if crate::ai::gemini_circuit::is_open() {
        return Err(crate::ai::gemini_circuit::blocked_error());
    }
    Ok(())
}

/// Embed a single text string using the Gemini embedding API.
pub async fn embed(api_key: &str, model: &str, text: &str) -> Result<(Vec<f32>, EmbedUsage)> {
    reject_if_circuit_open()?;
    let client = Client::new();
    let request = EmbedContentRequest {
        model: format!("models/{model}"),
        content: Content {
            parts: vec![Part {
                text: Some(text.to_string()),
                inline_data: None,
            }],
        },
        output_dimensionality: EMBEDDING_DIMENSION,
    };

    let response = client
        .post(gemini_url(model, api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(embed_http_error("Gemini embedding API error", status, body));
    }

    crate::ai::gemini_circuit::note_success();
    let parsed: EmbedContentResponse = response.json().await?;
    let usage = usage_from_metadata(parsed.usage_metadata.as_ref(), estimated_text_tokens(text));
    Ok((parsed.embedding.values, usage))
}

/// Embed a batch of texts using the Gemini batch embedding API.
/// The public Gemini endpoint supports up to 100 texts per batch.
pub async fn embed_batch(
    api_key: &str,
    model: &str,
    texts: Vec<String>,
) -> Result<(Vec<Vec<f32>>, EmbedUsage)> {
    reject_if_circuit_open()?;
    if texts.is_empty() {
        return Ok((Vec::new(), EmbedUsage::default()));
    }

    const MAX_BATCH_SIZE: usize = 100;
    if texts.len() > MAX_BATCH_SIZE {
        return Err(anyhow!(
            "embed_batch supports at most {MAX_BATCH_SIZE} texts, got {}",
            texts.len()
        ));
    }

    let estimated = estimated_texts_tokens(&texts);
    let client = Client::new();
    let requests = texts
        .into_iter()
        .map(|text| EmbedContentRequest {
            model: format!("models/{model}"),
            content: Content {
                parts: vec![Part {
                    text: Some(text),
                    inline_data: None,
                }],
            },
            output_dimensionality: EMBEDDING_DIMENSION,
        })
        .collect();

    let request = BatchEmbedContentsRequest { requests };

    let response = client
        .post(batch_gemini_url(model, api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(embed_http_error(
            "Gemini batch embedding API error",
            status,
            body,
        ));
    }
    crate::ai::gemini_circuit::note_success();

    let parsed: BatchEmbedContentsResponse = response.json().await?;
    let usage = usage_from_metadata(parsed.usage_metadata.as_ref(), estimated);
    Ok((
        parsed.embeddings.into_iter().map(|e| e.values).collect(),
        usage,
    ))
}

/// Embed a JPEG (and optional caption) with `gemini-embedding-2` multimodal input.
/// The vector lives in the same 3072-d space as text embeddings.
pub async fn embed_image(
    api_key: &str,
    model: &str,
    jpeg: &[u8],
    caption: Option<&str>,
) -> Result<(Vec<f32>, EmbedUsage)> {
    use base64::Engine as _;

    reject_if_circuit_open()?;
    if jpeg.is_empty() {
        return Err(anyhow!("empty image bytes"));
    }

    let mut parts = vec![Part {
        text: None,
        inline_data: Some(InlineData {
            mime_type: crate::media::JPEG_MIME.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(jpeg),
        }),
    }];
    if let Some(caption) = caption.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(Part {
            text: Some(caption.to_string()),
            inline_data: None,
        });
    }

    let client = Client::new();
    let request = EmbedContentRequest {
        model: format!("models/{model}"),
        content: Content { parts },
        output_dimensionality: EMBEDDING_DIMENSION,
    };

    let response = client
        .post(gemini_url(model, api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(embed_http_error(
            "Gemini image embedding API error",
            status,
            body,
        ));
    }
    crate::ai::gemini_circuit::note_success();

    let parsed: EmbedContentResponse = response.json().await?;
    let usage = usage_from_metadata(parsed.usage_metadata.as_ref(), 0);
    Ok((parsed.embedding.values, usage))
}

/// JSON array form required by sqlite-vec `MATCH`.
pub fn serialize_embedding(embedding: &[f32]) -> String {
    let mut s = String::with_capacity(2 + embedding.len() * 8);
    s.push('[');
    for (i, v) in embedding.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&v.to_string());
    }
    s.push(']');
    s
}

/// Format dated RAG snippets. These are PAST memories, not the current turn.
pub fn format_context(chunks: &[(String, Option<String>)]) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "Relevant context from PAST conversations (dated; not current events unless the user brings them up):\n",
    );
    for (i, (content, when)) in chunks.iter().enumerate() {
        let body = content.trim();
        match when.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(when) => out.push_str(&format!("\n[{}] ({when}) {body}\n", i + 1)),
            None => out.push_str(&format!("\n[{}] {body}\n", i + 1)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_url_format() {
        assert_eq!(
            gemini_url("model-1", "key-abc"),
            "https://generativelanguage.googleapis.com/v1beta/models/model-1:embedContent?key=key-abc"
        );
    }

    #[test]
    fn batch_gemini_url_format() {
        assert_eq!(
            batch_gemini_url("model-1", "key-abc"),
            "https://generativelanguage.googleapis.com/v1beta/models/model-1:batchEmbedContents?key=key-abc"
        );
    }

    #[test]
    fn format_context_empty() {
        assert_eq!(format_context(&[]), "");
    }

    #[test]
    fn format_context_with_chunks() {
        let chunks = vec![
            ("First memory".into(), Some("yesterday 11:12".into())),
            ("Second memory\nwith newline ".into(), None),
        ];
        let ctx = format_context(&chunks);
        assert!(ctx.starts_with(
            "Relevant context from PAST conversations (dated; not current events unless the user brings them up):\n"
        ));
        assert!(ctx.contains("[1] (yesterday 11:12) First memory"));
        assert!(ctx.contains("[2] Second memory\nwith newline"));
    }

    #[test]
    fn embedding_dimension_constant() {
        assert_eq!(EMBEDDING_DIMENSION, 3072);
    }

    #[test]
    fn estimated_text_tokens_char_heuristic() {
        assert_eq!(estimated_text_tokens(""), 1);
        assert_eq!(estimated_text_tokens("abcd"), 1);
        assert_eq!(estimated_text_tokens("abcde"), 2);
    }

    #[test]
    fn serialize_embedding_json_array() {
        let json = serialize_embedding(&[1.0, -0.5, 0.0]);
        assert_eq!(json, "[1,-0.5,0]");
    }

    #[test]
    fn usage_from_metadata_falls_back_to_estimate() {
        let usage = usage_from_metadata(None, 12);
        assert_eq!(
            usage,
            EmbedUsage {
                prompt_tokens: 12,
                image_tokens: 0
            }
        );
    }

    #[test]
    fn usage_from_metadata_splits_image_modality() {
        let meta = EmbedUsageMetadata {
            prompt_token_count: 80,
            prompt_tokens_details: vec![
                ModalityTokenCount {
                    modality: "TEXT".into(),
                    token_count: 12,
                },
                ModalityTokenCount {
                    modality: "IMAGE".into(),
                    token_count: 68,
                },
            ],
        };
        let usage = usage_from_metadata(Some(&meta), 99);
        assert_eq!(
            usage,
            EmbedUsage {
                prompt_tokens: 80,
                image_tokens: 68
            }
        );
    }

    #[test]
    fn usage_from_metadata_snake_case_document_counts_as_image() {
        let raw = serde_json::json!({
            "prompt_token_count": 40,
            "prompt_tokens_details": [{ "modality": "DOCUMENT", "token_count": 40 }]
        });
        let meta: EmbedUsageMetadata = serde_json::from_value(raw).unwrap();
        let usage = usage_from_metadata(Some(&meta), 1);
        assert_eq!(usage.prompt_tokens, 40);
        assert_eq!(usage.image_tokens, 40);
    }
}
