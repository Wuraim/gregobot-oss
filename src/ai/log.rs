//! Trace log of every LLM call (`run_tool_loop` execution).
//!
//! Traces are kept in a capped in-memory ring and persisted to SQLite so they
//! survive process/image restarts.
//!
//! Each trace captures the system prompt, initial messages, per-iteration
//! steps (model text, tool calls + results), final response, token usage and
//! duration. Exposed via `GET /api/ai/logs` and the `/ai/logs` viewer.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Default cap for the in-memory buffer and the SQLite persist table.
pub const DEFAULT_MAX_ENTRIES: usize = 200;
/// Per-field size cap (tool args/results, model text) to bound memory usage.
const MAX_FIELD_LEN: usize = 8192;

pub fn max_entries_from_env() -> usize {
    crate::logs::parse_positive_usize("AI_LOG_MAX_ENTRIES", DEFAULT_MAX_ENTRIES)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageView {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallView {
    pub name: String,
    pub args: String,
    pub result: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StepUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub estimated_usd: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepView {
    pub iteration: usize,
    pub model_text: String,
    pub tool_calls: Vec<ToolCallView>,
    pub finish_reason: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub estimated_usd: f64,
}

/// A Gemini call that belongs to a parent `/ask` (or similar) but is not the tool loop.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionCall {
    pub kind: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub image_tokens: i64,
    pub estimated_usd: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiTrace {
    pub id: u64,
    pub timestamp: String,
    pub kind: String,
    pub request_id: String,
    pub user_id: i64,
    pub telegram_user_id: i64,
    pub model: String,
    pub system_prompt: String,
    pub messages: Vec<MessageView>,
    pub steps: Vec<StepView>,
    pub children: Vec<SessionCall>,
    pub final_text: String,
    pub error: Option<String>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub estimated_usd: f64,
    pub session_usd: f64,
    pub cache_hit_pct: f64,
    pub cache_usd_saved: f64,
    pub rates: crate::ai::pricing::ModelRates,
    pub iterations: usize,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AiTraceSummary {
    pub id: u64,
    pub timestamp: String,
    pub kind: String,
    pub user_id: i64,
    pub telegram_user_id: i64,
    pub model: String,
    pub iterations: usize,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub estimated_usd: f64,
    pub session_usd: f64,
    pub cache_hit_pct: f64,
    pub cache_usd_saved: f64,
    pub child_count: usize,
    pub duration_ms: u64,
    pub preview: String,
    pub error: bool,
}

impl AiTrace {
    fn summary(&self) -> AiTraceSummary {
        let raw = if let Some(ref e) = self.error {
            format!("ERROR: {e}")
        } else {
            self.final_text.clone()
        };
        AiTraceSummary {
            id: self.id,
            timestamp: self.timestamp.clone(),
            kind: self.kind.clone(),
            user_id: self.user_id,
            telegram_user_id: self.telegram_user_id,
            model: self.model.clone(),
            iterations: self.iterations,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            cached_tokens: self.cached_tokens,
            estimated_usd: self.estimated_usd,
            session_usd: self.session_usd,
            cache_hit_pct: self.cache_hit_pct,
            cache_usd_saved: self.cache_usd_saved,
            child_count: self.children.len(),
            duration_ms: self.duration_ms,
            preview: truncate(&raw, 160),
            error: self.error.is_some(),
        }
    }
}

/// Server-side filters for `GET /api/ai/logs`. `None` fields match everything.
#[derive(Clone, Debug, Default)]
pub struct AiTraceFilter {
    pub error: Option<bool>,
    pub telegram_user_id: Option<i64>,
    pub kind: Option<String>,
}

impl AiTraceFilter {
    pub fn matches(&self, trace: &AiTrace) -> bool {
        if let Some(want_err) = self.error
            && trace.error.is_some() != want_err
        {
            return false;
        }
        if let Some(uid) = self.telegram_user_id
            && trace.telegram_user_id != uid
        {
            return false;
        }
        if let Some(ref kind) = self.kind
            && !kind.is_empty()
            && !trace.kind.eq_ignore_ascii_case(kind)
        {
            return false;
        }
        true
    }
}

/// Truncate a string to at most `max` chars, appending a marker when cut.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let boundary = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}… [+{} bytes]", &s[..boundary], s.len() - boundary)
}

/// Truncate a captured field (tool args/results, model text) to the global cap.
pub fn truncate_field(s: &str) -> String {
    truncate(s, MAX_FIELD_LEN)
}

pub struct AiLogBuffer {
    entries: VecDeque<AiTrace>,
    pending_children: HashMap<String, Vec<SessionCall>>,
    max: usize,
    next_id: u64,
}

impl AiLogBuffer {
    pub fn new(max: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max),
            pending_children: HashMap::new(),
            max,
            next_id: 1,
        }
    }

    pub fn push(&mut self, mut trace: AiTrace) {
        if let Some(mut kids) = self.pending_children.remove(&trace.request_id) {
            for c in &kids {
                trace.session_usd += c.estimated_usd;
            }
            trace.children.append(&mut kids);
        }
        trace.id = self.next_id;
        self.next_id += 1;
        if self.entries.len() >= self.max {
            self.entries.pop_front();
        }
        self.entries.push_back(trace);
    }

    fn attach_child(&mut self, request_id: &str, call: SessionCall) {
        if let Some(trace) = self
            .entries
            .iter_mut()
            .rev()
            .find(|t| t.request_id == request_id)
        {
            trace.session_usd += call.estimated_usd;
            trace.children.push(call);
            return;
        }
        self.pending_children
            .entry(request_id.to_string())
            .or_default()
            .push(call);
        if self.pending_children.len() > self.max * 2 {
            if let Some(oldest) = self.pending_children.keys().next().cloned() {
                self.pending_children.remove(&oldest);
            }
        }
    }

    /// Newest first.
    pub fn summaries_filtered(&self, limit: usize, filter: &AiTraceFilter) -> Vec<AiTraceSummary> {
        self.entries
            .iter()
            .rev()
            .filter(|t| filter.matches(t))
            .take(limit)
            .map(AiTrace::summary)
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<AiTrace> {
        self.entries.iter().find(|t| t.id == id).cloned()
    }

    fn set_max(&mut self, max: usize) {
        self.max = max.max(1);
        while self.entries.len() > self.max {
            self.entries.pop_front();
        }
    }

    fn load_persisted(&mut self, traces: Vec<AiTrace>) {
        if traces.is_empty() {
            return;
        }
        let current: Vec<AiTrace> = self.entries.drain(..).collect();
        let mut next_id = self.next_id;
        for trace in traces {
            next_id = next_id.max(trace.id.saturating_add(1));
            self.entries.push_back(trace);
        }
        for trace in current {
            next_id = next_id.max(trace.id.saturating_add(1));
            self.entries.push_back(trace);
        }
        self.next_id = next_id;
        while self.entries.len() > self.max {
            self.entries.pop_front();
        }
    }
}

#[derive(Clone)]
pub struct SharedAiLogBuffer {
    inner: Arc<Mutex<AiLogBuffer>>,
    persist: Arc<Mutex<Option<mpsc::Sender<AiTrace>>>>,
}

impl SharedAiLogBuffer {
    pub fn new(max: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AiLogBuffer::new(max.max(1)))),
            persist: Arc::new(Mutex::new(None)),
        }
    }

    pub fn max_entries(&self) -> usize {
        self.inner
            .lock()
            .map(|buf| buf.max)
            .unwrap_or(DEFAULT_MAX_ENTRIES)
    }

    pub fn set_max(&self, max: usize) {
        if let Ok(mut buf) = self.inner.lock() {
            buf.set_max(max);
        }
    }

    pub fn attach_persist(&self, tx: mpsc::Sender<AiTrace>) {
        if let Ok(mut persist) = self.persist.lock() {
            *persist = Some(tx);
        }
    }

    pub fn load_persisted(&self, traces: Vec<AiTrace>) {
        if let Ok(mut buf) = self.inner.lock() {
            buf.load_persisted(traces);
        }
    }

    pub fn push(&self, trace: AiTrace) {
        let stored = if let Ok(mut buf) = self.inner.lock() {
            buf.push(trace);
            buf.entries.back().cloned()
        } else {
            None
        };
        if let Some(trace) = stored {
            self.try_persist(trace);
        }
    }

    pub fn summaries_filtered(&self, limit: usize, filter: &AiTraceFilter) -> Vec<AiTraceSummary> {
        self.inner
            .lock()
            .map(|buf| buf.summaries_filtered(limit, filter))
            .unwrap_or_default()
    }

    pub fn get(&self, id: u64) -> Option<AiTrace> {
        self.inner.lock().ok().and_then(|buf| buf.get(id))
    }

    pub fn attach_child(&self, request_id: &str, call: SessionCall) {
        let updated = if let Ok(mut buf) = self.inner.lock() {
            buf.attach_child(request_id, call);
            buf.entries
                .iter()
                .rev()
                .find(|t| t.request_id == request_id)
                .cloned()
        } else {
            None
        };
        if let Some(trace) = updated {
            self.try_persist(trace);
        }
    }

    fn try_persist(&self, trace: AiTrace) {
        if let Ok(guard) = self.persist.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.try_send(trace);
        }
    }
}

pub fn session_call(
    kind: &str,
    model: &str,
    tokens: &crate::db::ai_usage::UsageTokens,
) -> SessionCall {
    SessionCall {
        kind: kind.to_string(),
        model: model.to_string(),
        prompt_tokens: tokens.prompt_tokens,
        completion_tokens: tokens.completion_tokens,
        total_tokens: tokens.total_tokens,
        cached_tokens: tokens.cached_tokens,
        image_tokens: tokens.image_tokens,
        estimated_usd: crate::ai::pricing::estimate_tokens_usd(model, tokens),
    }
}

pub fn attach_tokens(
    request_id: &str,
    kind: &str,
    model: &str,
    tokens: &crate::db::ai_usage::UsageTokens,
) {
    if request_id.is_empty() {
        return;
    }
    global().attach_child(request_id, session_call(kind, model, tokens));
}

pub async fn persist_child(
    pool: &crate::db::Db,
    user_id: i64,
    model: &str,
    tokens: crate::db::ai_usage::UsageTokens,
    kind: &str,
    request_id: Option<&str>,
) {
    crate::db::ai_usage::log_best_effort_detailed(pool, user_id, model, tokens, kind, request_id)
        .await;
    if let Some(rid) = request_id.filter(|s| !s.is_empty()) {
        attach_tokens(rid, kind, model, &tokens);
    }
}

pub async fn persist_embed(
    pool: &crate::db::Db,
    user_id: i64,
    model: &str,
    usage: crate::embeddings::EmbedUsage,
    kind: &str,
    request_id: Option<&str>,
) {
    persist_child(
        pool,
        user_id,
        model,
        crate::db::ai_usage::UsageTokens::from(usage),
        kind,
        request_id,
    )
    .await;
}

static GLOBAL: OnceLock<SharedAiLogBuffer> = OnceLock::new();

/// Process-wide trace buffer (lazily initialized).
pub fn global() -> &'static SharedAiLogBuffer {
    GLOBAL.get_or_init(|| SharedAiLogBuffer::new(DEFAULT_MAX_ENTRIES))
}

/// Accumulates an [`AiTrace`] while `run_tool_loop` executes.
pub struct TraceBuilder {
    kind: &'static str,
    request_id: String,
    user_id: i64,
    telegram_user_id: i64,
    model: String,
    system_prompt: String,
    messages: Vec<MessageView>,
    steps: Vec<StepView>,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    cached_tokens: i64,
    start: Instant,
}

impl TraceBuilder {
    pub fn new(
        kind: &'static str,
        user_id: i64,
        telegram_user_id: i64,
        model: &str,
        system_prompt: &str,
        messages: Vec<MessageView>,
    ) -> Self {
        Self {
            kind,
            request_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            telegram_user_id,
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            messages,
            steps: Vec::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_tokens: 0,
            start: Instant::now(),
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn record_step(
        &mut self,
        iteration: usize,
        model_text: String,
        tool_calls: Vec<ToolCallView>,
        finish_reason: Option<String>,
        usage: StepUsage,
    ) {
        self.steps.push(StepView {
            iteration,
            model_text: truncate_field(&model_text),
            tool_calls,
            finish_reason,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            estimated_usd: usage.estimated_usd,
        });
    }

    pub fn add_usage(
        &mut self,
        prompt: i64,
        completion: i64,
        total: i64,
        cached: i64,
    ) -> StepUsage {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += total;
        self.cached_tokens += cached;
        StepUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: total,
            cached_tokens: cached,
            estimated_usd: crate::ai::pricing::estimate_call_usd(
                &self.model,
                prompt,
                completion,
                cached,
            ),
        }
    }

    pub fn finish(self, final_text: String, error: Option<String>) -> AiTrace {
        let iterations = self.steps.iter().map(|s| s.iteration).max().unwrap_or(0);
        let estimated_usd = crate::ai::pricing::estimate_call_usd(
            &self.model,
            self.prompt_tokens,
            self.completion_tokens,
            self.cached_tokens,
        );
        let rates = crate::ai::pricing::rates_for(&self.model);
        let cache_hit_pct =
            crate::ai::pricing::cache_hit_pct(self.prompt_tokens, self.cached_tokens);
        let cache_usd_saved = crate::ai::pricing::cache_usd_saved(
            &self.model,
            self.prompt_tokens,
            self.completion_tokens,
            self.cached_tokens,
        );
        AiTrace {
            id: 0, // assigned by the buffer on push
            timestamp: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
            kind: self.kind.to_string(),
            request_id: self.request_id,
            user_id: self.user_id,
            telegram_user_id: self.telegram_user_id,
            model: self.model,
            system_prompt: self.system_prompt,
            messages: self.messages,
            steps: self.steps,
            children: Vec::new(),
            final_text,
            error,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            cached_tokens: self.cached_tokens,
            estimated_usd,
            session_usd: estimated_usd,
            cache_hit_pct,
            cache_usd_saved,
            rates,
            iterations,
            duration_ms: self.start.elapsed().as_millis() as u64,
        }
    }
}

/// Sample traces for local `/ai/logs` review (`SEED_AI_TRACES=1`).
pub fn seed_preview_traces() {
    let mut lite = TraceBuilder::new(
        "nudge",
        7,
        7007,
        "gemini-2.5-flash-lite",
        "Be brief.",
        vec![MessageView {
            role: "user".into(),
            text: "Still there?".into(),
        }],
    );
    let usage = lite.add_usage(800, 120, 920, 0);
    lite.record_step(1, "Go lift.".into(), Vec::new(), None, usage);
    global().push(lite.finish("Go lift.".into(), None));

    let request_id = uuid::Uuid::new_v4().to_string();
    attach_tokens(
        &request_id,
        crate::db::ai_usage::KIND_EMBED_QUERY,
        &format!("gemini-embedding-{}", 2),
        &crate::db::ai_usage::UsageTokens {
            prompt_tokens: 12,
            completion_tokens: 0,
            total_tokens: 12,
            cached_tokens: 0,
            image_tokens: 0,
        },
    );
    attach_tokens(
        &request_id,
        crate::db::ai_usage::KIND_COMPRESS,
        "gemini-2.5-flash-lite",
        &crate::db::ai_usage::UsageTokens {
            prompt_tokens: 2_000,
            completion_tokens: 200,
            total_tokens: 2_200,
            cached_tokens: 0,
            image_tokens: 0,
        },
    );
    let mut ask = TraceBuilder::new(
        "ask",
        42,
        4242,
        "gemini-2.5-flash",
        "You are a coach.",
        vec![MessageView {
            role: "user".into(),
            text: "What did I train this week?".into(),
        }],
    )
    .with_request_id(request_id);
    let usage = ask.add_usage(12_000, 1_800, 13_800, 4_000);
    ask.record_step(
        1,
        String::new(),
        vec![ToolCallView {
            name: "list_training".into(),
            args: "{}".into(),
            result: "[two runs, one bench]".into(),
        }],
        Some("STOP".into()),
        usage,
    );
    global().push(ask.finish("You ran twice and hit bench on Thursday.".into(), None));

    let mut image = TraceBuilder::new(
        "ask",
        42,
        4242,
        "gemini-2.5-flash-image",
        "Make a poster.",
        vec![MessageView {
            role: "user".into(),
            text: "Celebrate the PR.".into(),
        }],
    );
    let usage = image.add_usage(12, 0, 12, 0);
    image.record_step(1, "[image]".into(), Vec::new(), None, usage);
    global().push(image.finish("[image]".into(), None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trace_request_id_is_hyphenated_uuid_v4() {
        let trace =
            TraceBuilder::new("ask", 1, 1, "m", "sys", Vec::new()).finish(String::new(), None);
        let id = uuid::Uuid::parse_str(&trace.request_id).expect("request_id must parse as UUID");
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
        assert_eq!(trace.request_id, id.hyphenated().to_string());
        assert_eq!(trace.request_id.len(), 36);
        assert_eq!(trace.request_id.chars().filter(|c| *c == '-').count(), 4);
    }

    #[test]
    fn finish_attaches_estimated_usd_and_rates() {
        let mut builder = TraceBuilder::new(
            "ask",
            42,
            4242,
            "gemini-2.5-flash",
            "You are a coach.",
            vec![MessageView {
                role: "user".into(),
                text: "hi".into(),
            }],
        );
        let usage = builder.add_usage(1_000_000, 1_000_000, 2_000_000, 0);
        builder.record_step(1, "done".into(), Vec::new(), None, usage);
        let trace = builder.finish("done".into(), None);
        assert!((trace.estimated_usd - 2.80).abs() < 1e-9);
        assert_eq!(trace.rates.input_per_million, 0.30);
        assert_eq!(trace.rates.output_per_million, 2.50);
        assert_eq!(trace.telegram_user_id, 4242);
        assert_eq!(trace.steps[0].prompt_tokens, 1_000_000);
        assert!((trace.steps[0].estimated_usd - 2.80).abs() < 1e-9);
        let summary = trace.summary();
        assert!((summary.estimated_usd - 2.80).abs() < 1e-9);
        assert_eq!(summary.prompt_tokens, 1_000_000);
        assert_eq!(summary.cached_tokens, 0);
        assert_eq!(summary.telegram_user_id, 4242);
    }

    #[test]
    fn cached_tokens_reduce_trace_cost() {
        let mut builder = TraceBuilder::new("ask", 1, 11, "gemini-2.5-flash", "sys", Vec::new());
        builder.add_usage(1_000_000, 0, 1_000_000, 1_000_000);
        let trace = builder.finish(String::new(), None);
        assert!((trace.estimated_usd - 0.03).abs() < 1e-9);
        assert_eq!(trace.cached_tokens, 1_000_000);
        assert!((trace.cache_hit_pct - 100.0).abs() < 1e-9);
        assert!((trace.cache_usd_saved - 0.27).abs() < 1e-9);
    }

    #[test]
    fn session_children_roll_into_session_usd() {
        let mut builder =
            TraceBuilder::new("ask", 99142, 4242, "gemini-2.5-flash", "sys", Vec::new())
                .with_request_id("sess-1");
        builder.add_usage(1_000_000, 0, 1_000_000, 0);
        attach_tokens(
            "sess-1",
            "embed_query",
            &format!("gemini-embedding-{}", 2),
            &crate::db::ai_usage::UsageTokens {
                prompt_tokens: 1_000_000,
                completion_tokens: 0,
                total_tokens: 1_000_000,
                cached_tokens: 0,
                image_tokens: 0,
            },
        );
        global().push(builder.finish("ok".into(), None));
        let summaries = global().summaries_filtered(50, &AiTraceFilter::default());
        let row = summaries.iter().find(|s| s.user_id == 99142).unwrap();
        assert!(row.session_usd > row.estimated_usd);
        assert!(row.child_count >= 1);
        let trace = global().get(row.id).unwrap();
        assert_eq!(trace.children[0].kind, "embed_query");
        // flash 1M in $0.30 + embed 1M text $0.20
        assert!((trace.session_usd - 0.50).abs() < 1e-9);
    }

    #[test]
    fn preview_traces_include_estimated_usd() {
        seed_preview_traces();
        let summaries = global().summaries_filtered(50, &AiTraceFilter::default());
        assert!(summaries.len() >= 3);
        assert!(summaries.iter().all(|s| s.estimated_usd >= 0.0));
        assert!(summaries.iter().any(|s| s.estimated_usd > 0.03));
        assert!(summaries.iter().any(|s| s.session_usd >= s.estimated_usd));
        assert!(summaries.iter().any(|s| s.telegram_user_id == 4242));
        assert!(summaries.iter().any(|s| s.child_count >= 1));
        if std::env::var("GREGOBOT_DUMP_AI_UI").ok().as_deref() == Some("1") {
            let dir = "/tmp/gregobot_ai_ui";
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(
                format!("{dir}/summaries.json"),
                serde_json::to_string_pretty(&summaries).unwrap(),
            )
            .unwrap();
            std::fs::write(
                format!("{dir}/catalog.json"),
                serde_json::to_string_pretty(&crate::ai::pricing::pricing_catalog()).unwrap(),
            )
            .unwrap();
            for s in &summaries {
                let trace = global().get(s.id).expect("trace in buffer");
                std::fs::write(
                    format!("{dir}/trace-{}.json", s.id),
                    serde_json::to_string_pretty(&trace).unwrap(),
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn summaries_filtered_by_error_user_and_kind() {
        let mut buf = AiLogBuffer::new(10);
        buf.push(
            TraceBuilder::new("ask", 1, 100, "m", "sys", Vec::new()).finish("ok".into(), None),
        );
        buf.push(
            TraceBuilder::new("ask", 1, 200, "m", "sys", Vec::new())
                .finish("fail".into(), Some("boom".into())),
        );
        buf.push(
            TraceBuilder::new("nudge", 1, 200, "m", "sys", Vec::new()).finish("hi".into(), None),
        );

        let errors = buf.summaries_filtered(
            10,
            &AiTraceFilter {
                error: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].error);
        assert_eq!(errors[0].telegram_user_id, 200);

        let user = buf.summaries_filtered(
            10,
            &AiTraceFilter {
                telegram_user_id: Some(200),
                ..Default::default()
            },
        );
        assert_eq!(user.len(), 2);

        let kind = buf.summaries_filtered(
            10,
            &AiTraceFilter {
                kind: Some("nudge".into()),
                ..Default::default()
            },
        );
        assert_eq!(kind.len(), 1);
        assert_eq!(kind[0].kind, "nudge");
    }

    #[test]
    fn load_persisted_restores_ids_and_respects_max() {
        let mut buf = AiLogBuffer::new(2);
        let mut a = TraceBuilder::new("ask", 1, 10, "m", "sys", Vec::new())
            .with_request_id("a")
            .finish("a".into(), None);
        a.id = 40;
        let mut b = TraceBuilder::new("nudge", 1, 11, "m", "sys", Vec::new())
            .with_request_id("b")
            .finish("b".into(), None);
        b.id = 41;
        let mut c = TraceBuilder::new("ask", 1, 12, "m", "sys", Vec::new())
            .with_request_id("c")
            .finish("c".into(), None);
        c.id = 42;
        buf.load_persisted(vec![a, b, c]);
        assert_eq!(buf.entries.len(), 2);
        assert_eq!(buf.entries[0].request_id, "b");
        assert_eq!(buf.entries[1].request_id, "c");
        assert_eq!(buf.next_id, 43);
        buf.push(TraceBuilder::new("ask", 1, 13, "m", "sys", Vec::new()).finish("d".into(), None));
        assert_eq!(buf.entries.back().unwrap().id, 43);
    }
}
