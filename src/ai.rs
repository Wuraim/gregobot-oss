pub mod context;
pub mod exercise_selection;
pub mod food_propose;
pub mod food_selection;
pub mod gemini_circuit;
pub mod gemini_error;
pub mod image;
pub mod log;
pub mod pricing;
pub mod tool_loop;
pub mod tool_selection;
pub mod tools;
pub mod training_extract;

use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::ai::context::{AiContext, AskInput, GenerationInput, UserContext};
use crate::db::Db;
use crate::i18n::{Lang, Msg};

fn gemini_url(model: &str, api_key: &str) -> String {
    format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
    )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    role: String,
    parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<InlineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FunctionCall {
    name: String,
    args: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    function_declarations: Option<Vec<tools::ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    google_search: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    include_server_side_tool_invocations: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<ToolConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
    #[serde(default)]
    usage_metadata: Option<UsageMetadata>,
    #[serde(default)]
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    #[serde(default)]
    block_reason: Option<String>,
    #[serde(default)]
    block_reason_message: Option<String>,
    #[serde(default)]
    safety_ratings: Vec<SafetyRating>,
}

#[derive(Debug, Clone, Deserialize)]
struct SafetyRating {
    category: Option<String>,
    probability: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Content,
    finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    #[serde(default)]
    prompt_token_count: i64,
    #[serde(default)]
    candidates_token_count: i64,
    #[serde(default)]
    thoughts_token_count: i64,
    #[serde(default)]
    total_token_count: i64,
    #[serde(default)]
    cached_content_token_count: i64,
}

#[derive(Debug, Clone, Default)]
struct Usage {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    cached_tokens: i64,
}

impl UsageMetadata {
    fn billed(&self) -> Usage {
        let (prompt_tokens, completion_tokens, total_tokens) = pricing::billed_tokens(
            self.prompt_token_count,
            self.candidates_token_count,
            self.thoughts_token_count,
            self.total_token_count,
        );
        Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens: self.cached_content_token_count.max(0).min(prompt_tokens),
        }
    }
}

impl From<UsageMetadata> for Usage {
    fn from(meta: UsageMetadata) -> Self {
        meta.billed()
    }
}

/// Prefix a user message with the Telegram speaker so the model can tell
/// participants apart (especially in shared group history).
pub fn label_user_message(speaker: &str, prompt: &str) -> String {
    let speaker = speaker.trim();
    if speaker.is_empty() {
        prompt.to_string()
    } else {
        format!("[{speaker}]: {prompt}")
    }
}

/// Group vs private reply rules injected into `/ask` system prompts.
fn chat_scope_hint(
    group_context: Option<&crate::db::group_members::GroupContext>,
    speaker: &str,
) -> String {
    match group_context {
        Some(gc) => format!(
            "You are in a group chat with multiple participants. {}\n\
             Current speaker: {}.\n\
             Each user message is prefixed with [speaker]. Address the current speaker by name.\n\
             You may compare group training session counts and rankings when the thread is competitive (get_group_training_stats). Never reveal another participant's weight or meals.\n\
             GROUP LENGTH: this is a public thread, not a 1:1 session. Keep the visible reply to at most 3 short sentences (~400 characters). Skip RAPPORT / LES FAITS / VERDICT / COMMUNIQUÉ theater unless it fits in two lines. One joke max. No recap of the whole thread. One next action if needed, then stop.\n",
            gc.prompt_text(),
            speaker,
        ),
        None => {
            if speaker.trim().is_empty() {
                "Private chat: you may be a bit longer when a plan or a breakdown helps; still stay concise.\n"
                    .to_string()
            } else {
                format!(
                    "Private chat: you may be a bit longer when a plan or a breakdown helps; still stay concise.\n\
                     Current speaker: {}.\n\
                     Each user message is prefixed with [speaker]. Address them by name when natural.\n",
                    speaker,
                )
            }
        }
    }
}

pub async fn ask(
    pool: &Db,
    ctx: &AiContext,
    user_ctx: &UserContext,
    input: &AskInput<'_>,
) -> Result<String> {
    let prompt_text = if input.prompt.trim().is_empty() && input.image.is_some() {
        "Describe and respond to this image.".to_string()
    } else {
        input.prompt.to_string()
    };
    let labeled_prompt = label_user_message(input.speaker, &prompt_text);

    let span = tracing::info_span!(
        "ai_request",
        user_id = %user_ctx.user_id,
        speaker = %input.speaker,
        original_prompt = %prompt_text,
    );
    let _enter = span.enter();

    let description = get_user_description(pool, user_ctx.user_id).await;

    let timezone = crate::db::users::find_by_id(pool, user_ctx.user_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.timezone)
        .unwrap_or_else(|| "UTC".to_string());
    let tz: chrono_tz::Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let now = chrono::Utc::now();

    let group_hint = chat_scope_hint(input.group_context, input.speaker);

    let mut contents: Vec<Content> = Vec::new();

    let history_result = match input.group_context {
        Some(gc) => crate::db::conversations::recent_for_chat(pool, gc.chat_id, 10).await,
        None => crate::db::conversations::recent_for_user(pool, user_ctx.user_id, 10).await,
    };

    if let Ok(history) = history_result {
        for msg in history {
            let text = crate::db::conversations::content_for_model(&msg, &tz, now);
            contents.push(Content {
                role: msg.role,
                parts: vec![Part {
                    text: Some(text),
                    function_call: None,
                    function_response: None,
                    thought_signature: None,
                    inline_data: None,
                }],
            });
        }
    }

    let request_id = uuid::Uuid::new_v4().to_string();
    let prompt_embedding = match crate::embeddings::embed(
        ctx.api_key.as_deref().unwrap_or_default(),
        &ctx.embedding_model,
        &prompt_text,
    )
    .await
    {
        Ok(v) => {
            log::persist_embed(
                pool,
                user_ctx.user_id,
                &ctx.embedding_model,
                v.1,
                crate::db::ai_usage::KIND_EMBED_QUERY,
                Some(&request_id),
            )
            .await;
            Some(v.0)
        }
        Err(e) => {
            warn!(user_id = %user_ctx.user_id, "Prompt embedding failed: {e}");
            None
        }
    };

    let rag_context = if ctx.rag_enabled {
        if let Some(ref emb) = prompt_embedding {
            retrieve_rag_context(pool, user_ctx.user_id, emb, ctx.rag_top_k, &tz, now).await
        } else {
            None
        }
    } else {
        None
    };

    let selected_tool_names = prompt_embedding
        .as_deref()
        .and_then(tool_selection::select_tools);

    tracing::debug!(
        "For the given prompt {}. The matched tools of the last prompt are : {};",
        prompt_text,
        selected_tool_names
            .as_ref()
            .map(|names| names.join(", "))
            .unwrap_or_default(),
    );

    let image_quota_line = match crate::ai::image::quota_today(
        pool,
        user_ctx.user_id,
        &ctx.image_model,
        ctx.image_max_per_user_day,
    )
    .await
    {
        Ok(q) => crate::ai::image::format_quota_for_prompt(q, ctx.api_key().is_some()),
        Err(e) => {
            warn!(user_id = %user_ctx.user_id, "Failed to load image quota: {e}");
            String::new()
        }
    };

    let photo_archive_line =
        if should_attach_photo_archive(&prompt_text, selected_tool_names.as_deref()) {
            if let Some(ref emb) = prompt_embedding {
                match crate::db::photo::search(
                    pool,
                    user_ctx.user_id,
                    emb,
                    crate::db::photo::PhotoSearchScope::from_chat_id(input.chat_id),
                    3,
                    None,
                    false,
                )
                .await
                {
                    Ok(hits) => crate::db::photo::format_pointer_context(&hits).unwrap_or_default(),
                    Err(e) => {
                        warn!(user_id = %user_ctx.user_id, "Photo archive search failed: {e}");
                        String::new()
                    }
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

    let system_prompt = format!(
        "{persona}\n\
         You have access to the user's daily life data — fitness, tasks, reminders, and feature proposals. Help them win the day. Lead with the move that matters most.\n\
         {group_hint}\
          IMPORTANT: Respond ONLY in {language}.\n\
          Reply as the coach: short, factual, next action. Use RAPPORT / LES FAITS / VERDICT only when they fit; otherwise a normal reply. If a fact is missing, say so. Never invent numbers, meals, or sessions.\n\
          When the user mentions they completed a workout or training session (e.g. 'I did back workout yesterday'), call log_training in that same turn even with no sets, distance, or duration, using the appropriate recorded_at. For weight training pass activity gym even when the loads are still unknown. Ask for loads or reps only after that session id exists, and attach them with update_training. Use the user's timezone ({timezone}) to interpret relative dates like 'yesterday' or 'this morning'.\n\
          If log_training returns duplicate=true, the session was already saved — do not call log_training again; use update_training on that id if details are missing.\n\
          If a workout screenshot is attached (Strava, Garmin, gym log, CrossFit WOD), extract visible stats into log_training: activity run/ride/walk/hike/swim with endurance (distance_m, duration_sec, elevation_m), gym with strength_sets whose exercise field is a catalog slug (bench_press, squat, dumbbell_biceps_curl, push_up, … — never a free-text name), CrossFit/Hyrox with both. Never invent numbers.\n\
          When the user explicitly states their body weight (e.g. 'I weigh 82.5 kg'), use log_weight once. Never invent a weight. Gym loads (bar, machine, dumbbells) are log_training, not log_weight. If log_weight returns an error or already-succeeded, stop calling it.\n\
          When the user logs food, a meal, macros, or a nutrition-label photo, use register_food / search_foods / log_meal / update_meal / remove_meal. Prefer catalog foods over estimates. If estimating, set source estimated and say so.\n\
          When they have a craving (envie), ask what to eat, or want a meal idea, use propose_meals. Propose 2–3 options from the catalog and today's diary; do not auto-log. If they say ok, then log_meal. Save allergies, dislikes, likes, and eating habits with remember_food_preference — do not rely on conversation memory alone for never-eat-X.\n\
          When they ask what they ate, their calories/protein for the day, or want a nutrition table, use get_day_nutrition and show_nutrition_table. Food diary is private: in groups, do not log, propose, or display nutrition.\n\
          Never ask the user to take a photo of their body or physique to track evolution. Never suggest a selfie, mirror shot, or before/after. Progress is weight, training, and nutrition — not pictures of the person.\n\
          For tasks, use create_task, update_task, or remove_task when the user wants to manage a todo.\n\
          update_task supports partial updates: only send the fields the user wants to change, such as priority, due_at, tags, description, or title.\n\
          If the user wants to move or change an existing reminder, first inspect reminders with get_reminders, then use update_reminder on the matching reminder.\n\
          You can create or remove feature proposals on the user's behalf — if they describe an idea or wish for the bot, use create_feature_proposal; if they want to drop one, use list_my_proposals then remove_feature_proposal.\n\
         {web_search}\
         {rag_context}\
         {photo_archive}\
         {image_quota}\
         TIME: it is {local_now} in the user's timezone ({timezone}); UTC {now}. Conversation history and RAG snippets are dated. Treat them as the past. Do not bring up an old memory as if it just happened. Prefer tool data (training, weight, meals) for what is current. Interpret relative dates like yesterday in {timezone}.\n\
         {description}",
        persona = ctx.persona,
        group_hint = group_hint,
        language = lang_name(user_ctx.lang),
        timezone = timezone,
        web_search = web_search_prompt(user_ctx.web_search_enabled),
        rag_context = rag_context.unwrap_or_default(),
        photo_archive = photo_archive_line,
        image_quota = image_quota_line,
        now = now.format("%Y-%m-%dT%H:%M:%SZ"),
        local_now = now.with_timezone(&tz).format("%Y-%m-%d %H:%M"),
        description = description
            .map(|d| format!("\nUser profile: {d}"))
            .unwrap_or_default(),
    );

    tracing::debug!("The full given prompt is : {}", &system_prompt);

    let mut user_parts = Vec::new();
    if let Some(image) = input.image {
        use base64::Engine as _;
        user_parts.push(Part {
            text: None,
            function_call: None,
            function_response: None,
            thought_signature: None,
            inline_data: Some(InlineData {
                mime_type: image.mime_type.to_string(),
                data: base64::engine::general_purpose::STANDARD.encode(image.bytes),
            }),
        });
    }
    user_parts.push(Part {
        text: Some(labeled_prompt.clone()),
        function_call: None,
        function_response: None,
        thought_signature: None,
        inline_data: None,
    });

    contents.push(Content {
        role: "user".into(),
        parts: user_parts,
    });

    let (tools, tool_config) =
        build_tools(user_ctx.web_search_enabled, selected_tool_names.as_deref());

    let (content, usage) = run_tool_loop(
        ctx,
        pool,
        user_ctx,
        RunToolLoopInput {
            system_prompt: &system_prompt,
            contents,
            tools,
            tool_config,
            original_prompt: &labeled_prompt,
            kind: crate::db::ai_usage::KIND_ASK,
            bot: input.bot,
            chat_id: input.chat_id,
            reply_to_message_id: input.reply_to_message_id,
            image: input.image,
            prompt_embedding: prompt_embedding.as_deref(),
            request_id: request_id.clone(),
        },
    )
    .await?;

    if content.is_empty() {
        warn!(
            user_id = %user_ctx.user_id,
            prompt = %input.prompt,
            "ask() received empty content from run_tool_loop"
        );
        return Err(anyhow!("Model returned empty content repeatedly"));
    }

    tracing::info!(
        user_id = %user_ctx.user_id,
        model = %ctx.model,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        total_tokens = usage.total_tokens,
        "AI request completed"
    );

    let chat_id = input.group_context.map(|gc| gc.chat_id);

    let user_msg_id =
        crate::db::conversations::insert(pool, user_ctx.user_id, "user", &labeled_prompt, chat_id)
            .await;
    if let Err(e) = &user_msg_id {
        warn!("Failed to save user message to conversation history: {e}");
    }

    let assistant_msg_id =
        crate::db::conversations::insert(pool, user_ctx.user_id, "assistant", &content, chat_id)
            .await;
    if let Err(e) = &assistant_msg_id {
        warn!("Failed to save AI response to conversation history: {e}");
    }

    if ctx.rag_enabled {
        if let Ok(id) = user_msg_id {
            spawn_embed_conversation(EmbedConversationJob {
                pool: pool.clone(),
                api_key: ctx.api_key.clone().unwrap_or_default(),
                embedding_model: ctx.embedding_model.clone(),
                user_id: user_ctx.user_id,
                chat_id,
                role: "user",
                content: labeled_prompt.clone(),
                source_id: id,
                request_id: Some(request_id.clone()),
            });
        }
        if let Ok(id) = assistant_msg_id {
            spawn_embed_conversation(EmbedConversationJob {
                pool: pool.clone(),
                api_key: ctx.api_key.clone().unwrap_or_default(),
                embedding_model: ctx.embedding_model.clone(),
                user_id: user_ctx.user_id,
                chat_id,
                role: "assistant",
                content: content.clone(),
                source_id: id,
                request_id: Some(request_id.clone()),
            });
        }
    }

    Ok(content)
}

async fn retrieve_rag_context(
    pool: &Db,
    user_id: i64,
    query_embedding: &[f32],
    rag_top_k: usize,
    tz: &chrono_tz::Tz,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    match crate::db::chunks::search(pool, user_id, query_embedding, rag_top_k as i64).await {
        Ok(hits) if !hits.is_empty() => {
            let chunks: Vec<(String, Option<String>)> = hits
                .into_iter()
                .map(|h| {
                    let when = crate::datetime::parse_sqlite_datetime(&h.created_at)
                        .map(|at| crate::datetime::format_relative_when(at, tz, now));
                    (h.content, when)
                })
                .collect();
            Some(crate::embeddings::format_context(&chunks))
        }
        Ok(_) => None,
        Err(e) => {
            warn!(user_id = %user_id, "RAG chunk search failed: {e}");
            None
        }
    }
}

struct EmbedConversationJob {
    pool: Db,
    api_key: String,
    embedding_model: String,
    user_id: i64,
    chat_id: Option<i64>,
    role: &'static str,
    content: String,
    source_id: i64,
    request_id: Option<String>,
}

fn spawn_embed_conversation(job: EmbedConversationJob) {
    tokio::spawn(async move {
        match crate::embeddings::embed(&job.api_key, &job.embedding_model, &job.content).await {
            Ok((embedding, usage)) => {
                crate::ai::log::persist_embed(
                    &job.pool,
                    job.user_id,
                    &job.embedding_model,
                    usage,
                    crate::db::ai_usage::KIND_EMBED_RAG,
                    job.request_id.as_deref(),
                )
                .await;
                if let Err(e) = crate::db::chunks::insert_conversation_chunk(
                    &job.pool,
                    job.user_id,
                    job.chat_id,
                    job.role,
                    &job.content,
                    job.source_id,
                    &embedding,
                )
                .await
                {
                    tracing::warn!(
                        user_id = %job.user_id,
                        source_id = %job.source_id,
                        "Failed to save conversation chunk embedding: {e}"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %job.user_id,
                    source_id = %job.source_id,
                    "Failed to embed conversation chunk: {e}"
                );
            }
        }
    });
}

/// Embed a conversation turn for RAG (used by ask + slash-command history).
pub fn spawn_embed_conversation_turn(
    pool: Db,
    api_key: String,
    embedding_model: String,
    user_id: i64,
    chat_id: Option<i64>,
    role: &'static str,
    content: String,
    source_id: i64,
) {
    spawn_embed_conversation(EmbedConversationJob {
        pool,
        api_key,
        embedding_model,
        user_id,
        chat_id,
        role,
        content,
        source_id,
        request_id: None,
    });
}

fn build_tools(
    web_search_enabled: bool,
    selected_tool_names: Option<&[String]>,
) -> (Vec<GeminiTool>, Option<ToolConfig>) {
    let tool_defs = match selected_tool_names {
        Some(names) => tools::tools_by_names(names),
        None => tools::all_tools(),
    };
    let mut tools = vec![GeminiTool {
        function_declarations: Some(tool_defs),
        google_search: None,
    }];
    let tool_config = if web_search_enabled {
        tools.push(GeminiTool {
            function_declarations: None,
            google_search: Some(serde_json::json!({})),
        });
        Some(ToolConfig {
            include_server_side_tool_invocations: Some(true),
        })
    } else {
        None
    };
    (tools, tool_config)
}

struct RunToolLoopInput<'a> {
    system_prompt: &'a str,
    contents: Vec<Content>,
    tools: Vec<GeminiTool>,
    tool_config: Option<ToolConfig>,
    original_prompt: &'a str,
    kind: &'static str,
    bot: Option<&'a teloxide::Bot>,
    chat_id: Option<i64>,
    reply_to_message_id: Option<teloxide::types::MessageId>,
    image: Option<context::AskImage<'a>>,
    prompt_embedding: Option<&'a [f32]>,
    request_id: String,
}

fn content_to_view(c: &Content) -> log::MessageView {
    let text = c
        .parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    log::MessageView {
        role: c.role.clone(),
        text,
    }
}

/// Wrapper that records a full trace of the call into [`log::global()`],
/// then delegates to the actual tool loop.
async fn run_tool_loop(
    ctx: &AiContext,
    pool: &Db,
    user_ctx: &UserContext,
    input: RunToolLoopInput<'_>,
) -> Result<(String, Usage)> {
    let kind = input.kind;
    let mut trace = log::TraceBuilder::new(
        kind,
        user_ctx.user_id,
        user_ctx.telegram_user_id,
        &ctx.model,
        input.system_prompt,
        input.contents.iter().map(content_to_view).collect(),
    )
    .with_request_id(input.request_id.clone());
    let span = tracing::info_span!(
        "llm_call",
        request_id = %trace.request_id(),
        kind = input.kind,
        user_id = user_ctx.user_id,
    );
    let _enter = span.enter();

    let result = run_tool_loop_inner(ctx, pool, user_ctx, input, &mut trace).await;

    if let Ok((_, usage)) = &result {
        log_usage(
            pool,
            user_ctx.user_id,
            &ctx.model,
            usage,
            kind,
            Some(trace.request_id()),
        )
        .await;
    }

    let (final_text, error) = match &result {
        Ok((text, _)) => (text.clone(), None),
        Err(e) => (String::new(), Some(format!("{e:#}"))),
    };
    log::global().push(trace.finish(final_text, error));
    result
}

async fn run_tool_loop_inner(
    ctx: &AiContext,
    pool: &Db,
    user_ctx: &UserContext,
    mut input: RunToolLoopInput<'_>,
    trace: &mut log::TraceBuilder,
) -> Result<(String, Usage)> {
    let client = Client::new();
    const MAX_ITERATIONS: usize = 10;
    let mut iterations = 0;
    let mut total_usage = Usage::default();
    let mut empty_count = 0u32;
    let mut tools_were_used = false;
    let mut malformed_nudged = false;
    let mut consecutive_unknown_tools = 0u32;
    let available_tool_names = attached_function_tool_names(&input.tools);
    let mut turn_guards = tool_loop::TurnGuards::default();
    let mut force_text = false;

    if gemini_circuit::is_open() {
        return Err(gemini_circuit::blocked_error());
    }

    loop {
        if iterations >= MAX_ITERATIONS {
            warn!(
                user_id = %user_ctx.user_id,
                iterations = iterations,
                "Too many tool call iterations; returning a short fallback instead of failing the ask"
            );
            let reply = match turn_guards.writes_summary() {
                Some(writes) => Msg::new(user_ctx.lang).ai_tool_loop_limit_after_writes(&writes),
                None => Msg::new(user_ctx.lang).ai_tool_loop_limit().to_string(),
            };
            return Ok((reply, total_usage));
        }
        iterations += 1;
        let mut step_usage = log::StepUsage::default();

        debug!(
            user_id = %user_ctx.user_id,
            iteration = iterations,
            empty_count = empty_count,
            tools_were_used = tools_were_used,
            contents_len = input.contents.len(),
            prompt = %input.original_prompt,
            "run_tool_loop iteration start"
        );

        let use_tools = !force_text && iterations < MAX_ITERATIONS;
        let request = GenerateContentRequest {
            system_instruction: Some(SystemInstruction {
                parts: vec![Part {
                    text: Some(input.system_prompt.to_string()),
                    function_call: None,
                    function_response: None,
                    thought_signature: None,
                    inline_data: None,
                }],
            }),
            contents: input.contents.clone(),
            tools: use_tools.then(|| input.tools.clone()),
            tool_config: if use_tools {
                input.tool_config.clone()
            } else {
                None
            },
        };

        tracing::debug!("Request number {} : {:?}", iterations, request);

        let response = client
            .post(gemini_url(
                &ctx.model,
                ctx.api_key.as_deref().unwrap_or_default(),
            ))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        tracing::debug!("Response : {:?}", response);

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(gemini_error::fail_http(
                pool,
                &ctx.bot_token,
                &ctx.model,
                input.kind,
                Some(user_ctx.user_id),
                status,
                body,
                "Gemini API error",
            ));
        }
        gemini_circuit::note_success();

        let raw_body = response.text().await?;
        debug!(?raw_body, "Gemini raw response body");
        let chat_response: GenerateContentResponse = serde_json::from_str(&raw_body)?;
        if let Some(ref fb) = chat_response.prompt_feedback {
            let reasons: Vec<&str> = fb
                .safety_ratings
                .iter()
                .filter_map(|r| {
                    r.category.as_deref().and_then(|cat| {
                        let prob = r.probability.as_deref().unwrap_or("UNKNOWN");
                        if prob != "NEGLIGIBLE" && prob != "LOW" {
                            Some(cat)
                        } else {
                            None
                        }
                    })
                })
                .collect();
            debug!(
                user_id = %user_ctx.user_id,
                block_reason = ?fb.block_reason,
                block_reason_message = ?fb.block_reason_message,
                high_risk_categories = ?reasons,
                "Gemini prompt feedback received"
            );
            if fb.block_reason.is_some() || !reasons.is_empty() {
                warn!(
                    user_id = %user_ctx.user_id,
                    block_reason = ?fb.block_reason,
                    block_reason_message = ?fb.block_reason_message,
                    high_risk_categories = ?reasons,
                    "Gemini response blocked by safety filter"
                );
                return Err(anyhow!(
                    "Safety filter blocked response: block_reason={}, categories={:?}",
                    fb.block_reason.as_deref().unwrap_or("none"),
                    reasons
                ));
            }
        }
        if let Some(meta) = chat_response.usage_metadata {
            let billed = meta.billed();
            total_usage.prompt_tokens += billed.prompt_tokens;
            total_usage.completion_tokens += billed.completion_tokens;
            total_usage.total_tokens += billed.total_tokens;
            total_usage.cached_tokens += billed.cached_tokens;
            step_usage = trace.add_usage(
                billed.prompt_tokens,
                billed.completion_tokens,
                billed.total_tokens,
                billed.cached_tokens,
            );
        }

        let Candidate {
            content: model_content,
            finish_reason,
        } = chat_response
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No candidates in Gemini response"))?;
        let mut text_parts = Vec::new();
        let mut function_calls = Vec::new();

        for part in &model_content.parts {
            if let Some(text) = &part.text {
                text_parts.push(text.clone());
            }
            if let Some(fc) = &part.function_call {
                function_calls.push(fc.clone());
            }
        }

        let model_text = text_parts.join("\n");

        if !function_calls.is_empty() {
            tools_were_used = true;
            input.contents.push(model_content);

            let mut call_views = Vec::with_capacity(function_calls.len());
            for fc in &function_calls {
                debug!(
                    user_id = %user_ctx.user_id,
                    iteration = iterations,
                    tool = %fc.name,
                    args = %fc.args,
                    "Executing tool call"
                );
                let timezone = crate::db::users::find_by_id(pool, user_ctx.user_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.timezone)
                    .unwrap_or_else(|| "UTC".to_string());
                let parent_request_id = trace.request_id().to_string();
                let result = match turn_guards.decide(&fc.name, &fc.args) {
                    tool_loop::ToolGuard::Block(body) => {
                        turn_guards.record(&fc.name, &fc.args, false, &body);
                        if turn_guards.should_force_text() {
                            force_text = true;
                        }
                        body
                    }
                    tool_loop::ToolGuard::Allow => {
                        let tool_ctx = context::ToolCtx {
                            pool,
                            user_id: user_ctx.user_id,
                            timezone: &timezone,
                            lang: user_ctx.lang,
                            is_admin: user_ctx.is_admin,
                            bot: input.bot,
                            bot_token: &ctx.bot_token,
                            chat_id: input.chat_id,
                            reply_to_message_id: input.reply_to_message_id,
                            api_key: ctx.api_key(),
                            embedding_model: &ctx.embedding_model,
                            image_model: &ctx.image_model,
                            image_max_per_user_day: ctx.image_max_per_user_day,
                            user_photo: input.image,
                            prompt_embedding: input.prompt_embedding,
                            request_id: Some(parent_request_id.as_str()),
                            rag_enabled: ctx.rag_enabled,
                        };
                        let body = tools::execute_tool(
                            &tool_ctx,
                            &fc.name,
                            &fc.args,
                            &available_tool_names,
                        )
                        .await;
                        turn_guards.record(&fc.name, &fc.args, true, &body);
                        body
                    }
                };
                debug!(
                    user_id = %user_ctx.user_id,
                    iteration = iterations,
                    tool = %fc.name,
                    result = %result,
                    "Tool call result"
                );
                if tools::is_unknown_tool_response(&result) {
                    consecutive_unknown_tools += 1;
                    if consecutive_unknown_tools >= MAX_CONSECUTIVE_UNKNOWN_TOOLS {
                        warn!(
                            user_id = %user_ctx.user_id,
                            tool = %fc.name,
                            consecutive_unknown_tools,
                            "Aborting tool loop after consecutive unknown tool calls"
                        );
                        return Err(anyhow!(
                            "Too many unknown tool calls ({consecutive_unknown_tools} consecutive); last tool: {}",
                            fc.name
                        ));
                    }
                } else {
                    consecutive_unknown_tools = 0;
                }
                call_views.push(log::ToolCallView {
                    name: fc.name.clone(),
                    args: log::truncate_field(&fc.args.to_string()),
                    result: log::truncate_field(&result),
                });
                let response_value = serde_json::from_str(&result)
                    .unwrap_or_else(|_| serde_json::json!({"result": result}));
                input.contents.push(Content {
                    role: "user".into(),
                    parts: vec![Part {
                        text: None,
                        function_call: None,
                        function_response: Some(FunctionResponse {
                            name: fc.name.clone(),
                            response: response_value,
                        }),
                        thought_signature: None,
                        inline_data: None,
                    }],
                });
            }

            trace.record_step(
                iterations,
                model_text,
                call_views,
                finish_reason.clone(),
                step_usage,
            );
            continue;
        }

        let text = model_text;
        trace.record_step(
            iterations,
            text.clone(),
            Vec::new(),
            finish_reason.clone(),
            step_usage,
        );
        if !text.is_empty() {
            return Ok((text, total_usage));
        }

        empty_count += 1;

        debug!(
            user_id = %user_ctx.user_id,
            iteration = iterations,
            finish_reason = ?finish_reason,
            empty_count = empty_count,
            tools_were_used = tools_were_used,
            "Model response had no text content"
        );

        if empty_count > 3 {
            warn!(
                user_id = %user_ctx.user_id,
                iterations = iterations,
                finish_reason = ?finish_reason,
                tools_were_used = tools_were_used,
                "Model returned empty content repeatedly; returning empty response"
            );
            return Ok((String::new(), total_usage));
        }

        if iterations == 1 && finish_reason.as_deref() == Some("STOP") {
            if input.contents.len() > 4 {
                debug!(
                    user_id = %user_ctx.user_id,
                    iteration = iterations,
                    contents_len = input.contents.len(),
                    "Compressing conversation due to empty STOP response"
                );
                let last = input
                    .contents
                    .pop()
                    .ok_or_else(|| anyhow!("No user message to compress against"))?;
                let history_summary = compress_conversation(
                    pool,
                    user_ctx.user_id,
                    ctx,
                    &input.contents,
                    Some(trace.request_id()),
                )
                .await?;
                input.contents = vec![
                    Content {
                        role: "user".into(),
                        parts: vec![Part {
                            text: Some(format!(
                                "[Previous conversation summary]\n{history_summary}"
                            )),
                            function_call: None,
                            function_response: None,
                            thought_signature: None,
                            inline_data: None,
                        }],
                    },
                    last,
                ];
            } else {
                debug!(
                    user_id = %user_ctx.user_id,
                    iteration = iterations,
                    "Nudging model to respond after empty STOP on first iteration"
                );
                input.contents.push(Content {
                    role: "user".into(),
                    parts: vec![Part {
                        text: Some("Please respond to the user's message.".to_string()),
                        function_call: None,
                        function_response: None,
                        thought_signature: None,
                        inline_data: None,
                    }],
                });
            }
            continue;
        }

        if tools_were_used {
            debug!(
                user_id = %user_ctx.user_id,
                iteration = iterations,
                "Nudging model to summarize tool results"
            );
            input.contents.push(Content {
                role: "user".into(),
                parts: vec![Part {
                    text: Some(
                        "Tu viens d'utiliser un outil. Résume en une phrase très courte et en français ce que tu as fait pour l'utilisateur. Si tu as enregistré plusieurs choses, dis-le simplement."
                            .to_string(),
                    ),
                    function_call: None,
                    function_response: None,
                    thought_signature: None,
                    inline_data: None,
                }],
            });
            continue;
        }

        if finish_reason.as_deref() == Some("MALFORMED_FUNCTION_CALL") && !malformed_nudged {
            malformed_nudged = true;
            warn!(
                user_id = %user_ctx.user_id,
                iteration = iterations,
                "Model produced malformed function call; nudging to respond without tools"
            );
            input.contents.push(Content {
                role: "user".into(),
                parts: vec![Part {
                    text: Some(
                        "Please just respond to the user's message directly in natural language, without using any tools."
                            .to_string(),
                    ),
                    function_call: None,
                    function_response: None,
                    thought_signature: None,
                    inline_data: None,
                }],
            });
            continue;
        }

        warn!(
            user_id = %user_ctx.user_id,
            iteration = iterations,
            finish_reason = ?finish_reason,
            "Model returned empty content without using tools — letting empty_count tick"
        );
    }
}

async fn compress_conversation(
    pool: &Db,
    user_id: i64,
    ctx: &AiContext,
    contents: &[Content],
    request_id: Option<&str>,
) -> Result<String> {
    let mut conversation_text = String::new();
    for content in contents {
        let role = &content.role;
        let texts: Vec<&str> = content
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect();
        if texts.is_empty() {
            continue;
        }
        conversation_text.push_str(&format!("{role}: {}\n", texts.join(" ")));
    }

    let client = Client::new();
    let request = GenerateContentRequest {
        system_instruction: Some(SystemInstruction {
            parts: vec![Part {
                text: Some(
                    "You are a conversation compressor. Output only the summary, no preamble."
                        .to_string(),
                ),
                function_call: None,
                function_response: None,
                thought_signature: None,
                inline_data: None,
            }],
        }),
        contents: vec![Content {
            role: "user".into(),
            parts: vec![Part {
                text: Some(format!(
                    "Summarize this conversation between users and an AI assistant. \
                     Preserve who said what (keep speaker names from [Name]: prefixes). \
                     Preserve: data users shared (weight, training, tasks), key context and \
                     facts, preferences and goals. Be very concise.\n\n{conversation_text}"
                )),
                function_call: None,
                function_response: None,
                thought_signature: None,
                inline_data: None,
            }],
        }],
        tools: None,
        tool_config: None,
    };

    let response = client
        .post(gemini_url(
            &ctx.summarize_model,
            ctx.api_key.as_deref().unwrap_or_default(),
        ))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(gemini_error::fail_http(
            pool,
            &ctx.bot_token,
            &ctx.summarize_model,
            "summarize",
            Some(user_id),
            status,
            body,
            "Summarization API error",
        ));
    }
    gemini_circuit::note_success();

    let chat_response: GenerateContentResponse = response.json().await?;
    let usage = chat_response
        .usage_metadata
        .clone()
        .map(Usage::from)
        .unwrap_or_default();
    log_usage(
        pool,
        user_id,
        &ctx.summarize_model,
        &usage,
        crate::db::ai_usage::KIND_COMPRESS,
        request_id,
    )
    .await;
    if let Some(rid) = request_id {
        crate::ai::log::attach_tokens(
            rid,
            crate::db::ai_usage::KIND_COMPRESS,
            &ctx.summarize_model,
            &crate::db::ai_usage::UsageTokens {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cached_tokens: usage.cached_tokens,
                image_tokens: 0,
            },
        );
    }
    let candidate = chat_response
        .candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No candidates in summarization response"))?;

    let summary: String = candidate
        .content
        .parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect();

    if summary.is_empty() {
        return Err(anyhow!("Summarization produced empty result"));
    }

    Ok(summary)
}

async fn log_usage(
    pool: &Db,
    user_id: i64,
    model: &str,
    usage: &Usage,
    kind: &str,
    request_id: Option<&str>,
) {
    crate::db::ai_usage::log_best_effort_detailed(
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
        kind,
        request_id,
    )
    .await;
}

fn lang_name(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "English",
        Lang::Fr => "French",
    }
}

fn web_search_prompt(enabled: bool) -> &'static str {
    if enabled {
        "You also have access to Google Search as a built-in capability (not a function tool). Do not call google_search or web_search as function names. Use it when the user asks about current events, recent facts, or anything that requires up-to-date information beyond their personal data.\n"
    } else {
        ""
    }
}

/// Attach photo-archive pointers only when the turn is about a stored screenshot/log,
/// so physique / progress chats do not surface body photos.
fn should_attach_photo_archive(prompt: &str, selected: Option<&[String]>) -> bool {
    match selected {
        Some(names) => names.iter().any(|n| n == "search_photos"),
        None => prompt_mentions_stored_photo(prompt),
    }
}

fn prompt_mentions_stored_photo(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    [
        "screenshot",
        "strava",
        "garmin",
        "capture",
        "jpeg",
        "étiquette",
        "etiquette",
        "food label",
        "nutrition label",
    ]
    .iter()
    .any(|k| p.contains(k))
}

/// Unprompted schedulers get the full catalog minus photo search, so they cannot
/// propose physique pictures.
fn unprompted_tool_names() -> Vec<String> {
    tools::all_tools()
        .into_iter()
        .map(|t| t.name)
        .filter(|n| n != "search_photos")
        .collect()
}

fn attached_function_tool_names(tools: &[GeminiTool]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|t| t.function_declarations.as_ref())
        .flat_map(|defs| defs.iter().map(|d| d.name.clone()))
        .collect()
}

const MAX_CONSECUTIVE_UNKNOWN_TOOLS: u32 = 3;

async fn get_user_description(pool: &Db, user_id: i64) -> Option<String> {
    match crate::db::users::find_by_id(pool, user_id).await {
        Ok(Some(u)) => u.description.filter(|d| !d.is_empty()),
        _ => None,
    }
}

async fn generate_message(
    pool: &Db,
    ctx: &AiContext,
    user_ctx: &UserContext,
    input: &GenerationInput<'_>,
) -> Result<(String, Usage)> {
    let contents = vec![Content {
        role: "user".into(),
        parts: vec![Part {
            text: Some(input.user_prompt.to_string()),
            function_call: None,
            function_response: None,
            thought_signature: None,
            inline_data: None,
        }],
    }];

    let unprompted_tools = unprompted_tool_names();
    let (tools, tool_config) = build_tools(user_ctx.web_search_enabled, Some(&unprompted_tools));

    run_tool_loop(
        ctx,
        pool,
        user_ctx,
        RunToolLoopInput {
            system_prompt: input.system_prompt,
            contents,
            tools,
            tool_config,
            original_prompt: input.user_prompt,
            kind: input.kind,
            bot: None,
            chat_id: None,
            reply_to_message_id: None,
            image: None,
            prompt_embedding: None,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
    )
    .await
}

pub async fn proactive_message(
    pool: &Db,
    ctx: &AiContext,
    user_ctx: &UserContext,
) -> Result<Option<String>> {
    let description = get_user_description(pool, user_ctx.user_id).await;

    let system_prompt = format!(
        "{persona}\n\
         You are checking in on the user unprompted. Cut the fluff. Stay demanding and short — no theatrical RAPPORT.\n\
         Look at their recent data using the available tools.\n\
         If nothing interesting or relevant comes up today, return exactly: [SKIP]\n\
         If something is relevant, send 1-2 sharp sentences — name the opportunity, demand the next step.\n\
         Don't repeat similar messages from previous days.\n\
         Occasionally (not every day), ask if there's a feature they'd love to see. If they describe one, use create_feature_proposal.\n\
         Never ask them to take a photo of their body or physique.\n\
         {web_search}\
         Current UTC time: {now}.\
         {description}\n\
         Respond ONLY in {language}.",
        persona = ctx.persona,
        web_search = web_search_prompt(user_ctx.web_search_enabled),
        now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        description = description
            .map(|d| format!("\nUser profile: {d}"))
            .unwrap_or_default(),
        language = lang_name(user_ctx.lang),
    );

    let input = GenerationInput {
        system_prompt: &system_prompt,
        user_prompt: "Check in on me — anything worth mentioning today?",
        kind: crate::db::ai_usage::KIND_PROACTIVE,
    };
    let (content, _usage) = generate_message(pool, ctx, user_ctx, &input).await?;

    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed == "[SKIP]" {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

pub async fn daily_recap_message(
    pool: &Db,
    ctx: &AiContext,
    user_ctx: &UserContext,
) -> Result<String> {
    let description = get_user_description(pool, user_ctx.user_id).await;

    let system_prompt = format!(
        "{persona}\n\
         You are delivering the user's daily recap. Look at their recent data using the available tools.\n\
         Give a tight, sober summary of their day: what happened (weight, training), what's coming up (reminders, tasks due).\n\
         If the day was quiet, name it — don't pad it.\n\
         End with one sharp line of momentum aimed at tomorrow's victory.\n\
         Never ask them to take a photo of their body or physique.\n\
         Keep it to 2-4 sentences. At most one dry line of humour; no RAPPORT staging.\n\
         {web_search}\
         Current UTC time: {now}.\
         {description}\n\
         Respond ONLY in {language}.",
        persona = ctx.persona,
        web_search = web_search_prompt(user_ctx.web_search_enabled),
        now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        description = description
            .map(|d| format!("\nUser profile: {d}"))
            .unwrap_or_default(),
        language = lang_name(user_ctx.lang),
    );

    let input = GenerationInput {
        system_prompt: &system_prompt,
        user_prompt: "Give me my daily recap for today.",
        kind: crate::db::ai_usage::KIND_DAILY_RECAP,
    };
    let (content, _usage) = generate_message(pool, ctx, user_ctx, &input).await?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok("Quiet day. Rest is part of the work. Sharpen for tomorrow.".to_string());
    }

    Ok(trimmed.to_string())
}

pub async fn nudge_message(
    pool: &Db,
    ctx: &AiContext,
    user_ctx: &UserContext,
    inactivity_days: i64,
) -> Result<String> {
    let description = get_user_description(pool, user_ctx.user_id).await;

    let system_prompt = format!(
        "{persona}\n\
         The user has been inactive for {days} days. No coddling — call it straight. Stay short; no RAPPORT staging.\n\
         Look at their profile, recent data, and active goals using the available tools.\n\
         Craft a nudge of 1-3 sentences. Name the gap, point at the path, demand the next step.\n\
         Make it land hard enough to move them, not to resent the reminder.\n\
         Never ask them to take a photo of their body or physique.\n\
         {web_search}\
         Current UTC time: {now}.\
         {description}\n\
         Respond ONLY in {language}.",
        persona = ctx.persona,
        days = inactivity_days,
        web_search = web_search_prompt(user_ctx.web_search_enabled),
        now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        description = description
            .map(|d| format!("\nUser profile: {d}"))
            .unwrap_or_default(),
        language = lang_name(user_ctx.lang),
    );

    let input = GenerationInput {
        system_prompt: &system_prompt,
        user_prompt: "Nudge me to get back on track.",
        kind: crate::db::ai_usage::KIND_NUDGE,
    };
    let (content, _usage) = generate_message(pool, ctx, user_ctx, &input).await?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(
            "Hey, you haven't logged any weight or training in a while — want to jump back in?"
                .to_string(),
        );
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        chat_scope_hint, label_user_message, should_attach_photo_archive, unprompted_tool_names,
    };
    use crate::db::group_members::{GroupContext, Participant};

    #[test]
    fn labels_speaker_on_user_message() {
        assert_eq!(label_user_message("@Greg", "salut"), "[@Greg]: salut");
    }

    #[test]
    fn empty_speaker_keeps_raw_prompt() {
        assert_eq!(label_user_message("  ", "salut"), "salut");
    }

    #[test]
    fn group_hint_caps_reply_length() {
        let gc = GroupContext {
            chat_id: -100,
            participants: vec![Participant {
                telegram_user_id: 1,
                username: Some("alice".into()),
                first_name: Some("Alice".into()),
            }],
        };
        let hint = chat_scope_hint(Some(&gc), "@alice");
        assert!(hint.contains("GROUP LENGTH"));
        assert!(hint.contains("3 short sentences"));
        assert!(hint.contains("@alice"));
        assert!(!hint.contains("Private chat"));
    }

    #[test]
    fn private_hint_allows_longer_breakdowns() {
        let hint = chat_scope_hint(None, "@greg");
        assert!(hint.contains("Private chat"));
        assert!(hint.contains("@greg"));
        assert!(!hint.contains("GROUP LENGTH"));
    }

    #[test]
    fn photo_archive_attaches_only_for_screenshot_intent() {
        let selected = vec!["search_photos".to_string()];
        assert!(should_attach_photo_archive(
            "how is my progress",
            Some(&selected)
        ));
        assert!(!should_attach_photo_archive(
            "how is my physique evolving",
            Some(&["log_weight".into()])
        ));
        assert!(should_attach_photo_archive(
            "retrouve la capture Strava",
            None
        ));
        assert!(!should_attach_photo_archive(
            "propose-moi de me prendre en photo pour voir mon évolution",
            None
        ));
        assert!(!should_attach_photo_archive(
            "comment va mon évolution physique",
            None
        ));
    }

    #[test]
    fn unprompted_tools_omit_photo_search() {
        let names = unprompted_tool_names();
        assert!(!names.iter().any(|n| n == "search_photos"));
        assert!(names.iter().any(|n| n == "log_weight"));
        assert!(names.iter().any(|n| n == "list_capabilities"));
    }
}
