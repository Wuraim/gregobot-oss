use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{
    Content, GenerateContentRequest, GenerateContentResponse, InlineData, Part, SystemInstruction,
    Usage, gemini_url, log_usage,
};
use crate::ai::context::AiContext;
use crate::ai::log;
use crate::db::Db;
use crate::db::strength_exercises::StrengthExercise;
use crate::db::training::{EnduranceStats, NewStrengthSet, NewTrainingEntry};
use crate::i18n::Lang;

const MAX_DESCRIPTION_LEN: usize = 400;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExtractedTraining {
    pub description: Option<String>,
    pub activity: Option<String>,
    pub endurance: Option<ExtractedEndurance>,
    pub strength_sets: Option<Vec<ExtractedSet>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ExtractedEndurance {
    pub duration_sec: Option<i64>,
    pub distance_m: Option<f64>,
    pub elevation_m: Option<f64>,
    pub avg_hr: Option<i64>,
    pub calories: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractedSet {
    /// Catalog slug. Free-text `exercise_name` is accepted only as a fallback and mapped to the enum.
    pub exercise: Option<String>,
    pub exercise_name: Option<String>,
    pub set_index: Option<i64>,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
}

impl ExtractedTraining {
    pub fn into_new_entry(
        self,
        user_id: i64,
        fallback_description: &str,
        recorded_at: Option<chrono::DateTime<chrono::Utc>>,
        keep_description: bool,
    ) -> NewTrainingEntry {
        let description = if keep_description {
            fallback_description.trim().to_string()
        } else {
            self.description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(truncate_description)
                .unwrap_or_else(|| fallback_description.trim().to_string())
        };
        let description = if description.is_empty() {
            fallback_description.trim().to_string()
        } else {
            description
        };

        NewTrainingEntry {
            user_id,
            description,
            recorded_at,
            activity_slug: self
                .activity
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase()),
            endurance: self.endurance.map(|e| EnduranceStats {
                duration_sec: e.duration_sec,
                distance_m: e.distance_m,
                elevation_m: e.elevation_m,
                avg_hr: e.avg_hr,
                calories: e.calories,
            }),
            strength_sets: self
                .strength_sets
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let exercise = StrengthExercise::parse_input(
                        s.exercise.as_deref(),
                        s.exercise_name.as_deref(),
                    )?;
                    Some(NewStrengthSet::new(
                        exercise,
                        s.set_index.unwrap_or((i as i64) + 1),
                        s.reps,
                        s.weight_kg,
                    ))
                })
                .collect(),
        }
    }
}

pub fn parse_extracted_json(raw: &str) -> Result<ExtractedTraining> {
    let trimmed = strip_json_fence(raw.trim());
    if let Ok(parsed) = serde_json::from_str(trimmed) {
        return Ok(parsed);
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| anyhow!("invalid training extract JSON"))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| anyhow!("invalid training extract JSON"))?;
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|e| anyhow!("invalid training extract JSON: {e}"))
}

fn strip_json_fence(s: &str) -> &str {
    let s = s.trim();
    let Some(rest) = s.strip_prefix("```") else {
        return s;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest).trim_start();
    rest.strip_suffix("```").unwrap_or(rest).trim()
}

fn truncate_description(text: &str) -> String {
    if text.chars().count() <= MAX_DESCRIPTION_LEN {
        return text.to_string();
    }
    text.chars().take(MAX_DESCRIPTION_LEN).collect()
}

async fn refine_extracted_sets(
    extracted: &mut ExtractedTraining,
    pool: &Db,
    ctx: &AiContext,
    user_id: i64,
    request_id: Option<&str>,
) {
    let Some(sets) = extracted.strength_sets.as_mut() else {
        return;
    };
    if sets.is_empty() {
        return;
    }
    let owned: Vec<(Option<String>, Option<String>)> = sets
        .iter()
        .map(|s| (s.exercise.clone(), s.exercise_name.clone()))
        .collect();
    let labels: Vec<(Option<&str>, Option<&str>)> = owned
        .iter()
        .map(|(a, b)| (a.as_deref(), b.as_deref()))
        .collect();
    let resolved = crate::ai::exercise_selection::resolve_labels(
        pool,
        user_id,
        ctx.api_key(),
        &ctx.embedding_model,
        &labels,
        request_id,
    )
    .await;
    for (set, ex) in sets.iter_mut().zip(resolved) {
        if let Some(ex) = ex {
            set.exercise = Some(ex.slug().to_string());
            set.exercise_name = None;
        }
    }
}

/// Extract structured training stats from optional text and/or a workout screenshot.
pub async fn extract_training_from_media(
    pool: &Db,
    ctx: &AiContext,
    user_id: i64,
    lang: Lang,
    text: Option<&str>,
    image: Option<(&[u8], &str)>,
) -> Result<ExtractedTraining> {
    use base64::Engine as _;

    let language = match lang {
        Lang::En => "English",
        Lang::Fr => "French",
    };

    let system_prompt = format!(
        "You extract structured training-session data from user text and/or a workout screenshot \
         (Strava, Garmin, Apple Fitness, gym notes, CrossFit WOD, etc.).\n\
         Return ONLY a JSON object, no markdown, no preamble.\n\
         Schema:\n\
         {{\n\
           \"description\": \"short human summary in {language}, max 20 words\",\n\
           \"activity\": \"run|ride|walk|hike|swim|gym|crossfit|other\",\n\
           \"recorded_at\": null,\n\
           \"endurance\": {{\n\
             \"duration_sec\": null, \"distance_m\": null, \"elevation_m\": null,\n\
             \"avg_hr\": null, \"calories\": null\n\
           }},\n\
           \"strength_sets\": [\n\
             {{\"exercise\": \"bench_press\", \"set_index\": 1, \"reps\": 8, \"weight_kg\": 80}}\n\
           ]\n\
         }}\n\
         Rules:\n\
         - activity: run/ride/walk/hike/swim = endurance sports; gym = weight training; \
           crossfit = mixed WOD/AMRAP/Hyrox; other if unclear.\n\
         - Never invent numbers. Missing values must be JSON null, not 0.\n\
         - For run/ride/walk/hike/swim: fill endurance, set strength_sets to null.\n\
         - For gym: fill strength_sets, set endurance to null.\n\
         - For crossfit: fill both when the data is present.\n\
         - Distances from screens (km) must be converted to meters (8.2 km → 8200).\n\
         - Durations like 42:10 or 1:05:00 become duration_sec.\n\
         - For gym sets, \"exercise\" MUST be a catalog slug: {exercises}.\n\
         - Never invent a free-text exercise name. Unknown movements use slug \"other\".\n\
         - description must name the activity in {language}.\n\
         - If there is no training content, still return JSON with activity \"other\".",
        language = language,
        exercises = StrengthExercise::prompt_enum(),
    );

    let mut user_text = String::from("Extract the training session as JSON.");
    if let Some(t) = text.map(str::trim).filter(|s| !s.is_empty()) {
        user_text.push_str("\nUser text:\n");
        user_text.push_str(t);
    }

    let mut parts = Vec::new();
    if let Some((bytes, mime)) = image {
        parts.push(Part {
            text: None,
            function_call: None,
            function_response: None,
            thought_signature: None,
            inline_data: Some(InlineData {
                mime_type: mime.to_string(),
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        });
    }
    parts.push(Part {
        text: Some(user_text.clone()),
        function_call: None,
        function_response: None,
        thought_signature: None,
        inline_data: None,
    });

    let telegram_user_id = crate::db::users::find_by_id(pool, user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.telegram_user_id)
        .unwrap_or(0);
    let mut trace = log::TraceBuilder::new(
        crate::db::ai_usage::KIND_TRAINING_EXTRACT,
        user_id,
        telegram_user_id,
        &ctx.model,
        &system_prompt,
        vec![log::MessageView {
            role: "user".into(),
            text: if image.is_some() {
                format!("[image] {user_text}")
            } else {
                user_text.clone()
            },
        }],
    );

    let request = GenerateContentRequest {
        system_instruction: Some(SystemInstruction {
            parts: vec![Part {
                text: Some(system_prompt.clone()),
                function_call: None,
                function_response: None,
                thought_signature: None,
                inline_data: None,
            }],
        }),
        contents: vec![Content {
            role: "user".into(),
            parts,
        }],
        tools: None,
        tool_config: None,
    };

    let client = Client::new();
    let outcome: Result<(String, Usage)> = async {
        let response = client
            .post(gemini_url(
                &ctx.model,
                ctx.api_key.as_deref().unwrap_or_default(),
            ))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(crate::ai::gemini_error::fail_http(
                pool,
                &ctx.bot_token,
                &ctx.model,
                "training_extract",
                Some(user_id),
                status,
                body,
                "Gemini API error",
            ));
        }

        let chat_response: GenerateContentResponse = response.json().await?;
        let usage = chat_response
            .usage_metadata
            .map(Usage::from)
            .unwrap_or_default();
        let candidate = chat_response
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No candidates in Gemini response"))?;
        let text = candidate
            .content
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(anyhow!("Model returned empty training extract"));
        }
        Ok((text, usage))
    }
    .await;

    match outcome {
        Ok((text, usage)) => {
            let usage_step = trace.add_usage(
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens,
                usage.cached_tokens,
            );
            trace.record_step(1, text.clone(), Vec::new(), None, usage_step);
            log_usage(
                pool,
                user_id,
                &ctx.model,
                &usage,
                crate::db::ai_usage::KIND_TRAINING_EXTRACT,
                Some(trace.request_id()),
            )
            .await;
            let parsed = parse_extracted_json(&text);
            match parsed {
                Ok(mut extracted) => {
                    refine_extracted_sets(
                        &mut extracted,
                        pool,
                        ctx,
                        user_id,
                        Some(trace.request_id()),
                    )
                    .await;
                    log::global().push(trace.finish(text, None));
                    Ok(extracted)
                }
                Err(e) => {
                    log::global().push(trace.finish(text, Some(format!("{e:#}"))));
                    Err(e)
                }
            }
        }
        Err(e) => {
            log::global().push(trace.finish(String::new(), Some(format!("{e:#}"))));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_json() {
        let raw = r#"{"description":"Tempo","activity":"run","endurance":{"duration_sec":2520,"distance_m":8200,"elevation_m":85,"avg_hr":null,"calories":null},"strength_sets":null}"#;
        let parsed = parse_extracted_json(raw).unwrap();
        assert_eq!(parsed.activity.as_deref(), Some("run"));
        assert_eq!(parsed.endurance.unwrap().distance_m, Some(8200.0));
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "```json\n{\"activity\":\"gym\",\"strength_sets\":[{\"exercise_name\":\"squat\",\"set_index\":1,\"reps\":8,\"weight_kg\":80}]}\n```";
        let parsed = parse_extracted_json(raw).unwrap();
        assert_eq!(parsed.activity.as_deref(), Some("gym"));
        assert_eq!(parsed.strength_sets.unwrap()[0].reps, Some(8));
    }

    #[test]
    fn catalog_slug_wins_over_free_text_name() {
        let raw = r#"{"activity":"gym","strength_sets":[{"exercise":"bench_press","exercise_name":"développé couché","set_index":1,"reps":5,"weight_kg":80}]}"#;
        let parsed = parse_extracted_json(raw).unwrap();
        let entry = parsed.into_new_entry(1, "gym", None, true);
        assert_eq!(
            entry.strength_sets[0].exercise,
            StrengthExercise::BenchPress
        );
    }

    #[test]
    fn unknown_exercise_maps_to_other() {
        let raw = r#"{"activity":"gym","strength_sets":[{"exercise":"other","exercise_name":"farmer walk","set_index":1,"reps":40}]}"#;
        let parsed = parse_extracted_json(raw).unwrap();
        let entry = parsed.into_new_entry(1, "gym", None, true);
        assert_eq!(entry.strength_sets[0].exercise, StrengthExercise::Other);
    }

    #[test]
    fn free_text_name_resolves_to_enum() {
        let raw = r#"{"activity":"gym","strength_sets":[{"exercise_name":"Développé couché","set_index":1,"reps":5}]}"#;
        let parsed = parse_extracted_json(raw).unwrap();
        let entry = parsed.into_new_entry(1, "gym", None, true);
        assert_eq!(
            entry.strength_sets[0].exercise,
            StrengthExercise::BenchPress
        );
    }
}
