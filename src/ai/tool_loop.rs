//! Helpers for the Gemini function-call loop: body-weight validation,
//! once-per-turn write-tool guards, and per-turn read/write-tool caps.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// Adult body-weight range in kilograms. Rejects the 0 / 1 kg hallucinated
/// writes seen when the model loops `log_weight` on a gym session.
pub const MIN_BODY_WEIGHT_KG: f64 = 20.0;
pub const MAX_BODY_WEIGHT_KG: f64 = 400.0;

/// Max successful calls of the same read/search tool in one turn.
/// Prod 2026-09-17/18: `search_photos` with different queries hit the 10-iteration
/// ceiling and failed the whole `/ask` with ERROR.
pub const MAX_READ_TOOL_CALLS_PER_TURN: u32 = 3;

/// Max successful calls of the same destructive write in one turn.
/// Prod 2026-09-23: `remove_training` ×7 then the 10-iteration soft fallback.
pub const MAX_WRITE_TOOL_CALLS_PER_TURN: u32 = 2;

/// Consecutive blocked tool results after which the loop should drop tools
/// and force a text reply.
pub const FORCE_TEXT_AFTER_CONSECUTIVE_BLOCKS: u32 = 2;

/// Total blocked tool results after which the loop should force text even if
/// an allowed write reset the consecutive counter.
/// Prod 2026-09-23: `get_training_entry` N+1 interleaved with `remove_training`.
pub const FORCE_TEXT_AFTER_TOTAL_BLOCKS: u32 = 3;

const ONCE_PER_TURN_WRITES: &[&str] = &["log_weight", "send_coach_image"];

const CAPPED_READ_TOOLS: &[&str] = &[
    "search_photos",
    "search_foods",
    "get_weight_entries",
    "get_training_entries",
    "get_training_entry",
    "get_active_goals",
    "get_group_training_stats",
    "get_tasks",
    "get_reminders",
    "get_day_nutrition",
    "get_image_quota",
    "list_my_proposals",
    "list_capabilities",
];

const CAPPED_WRITE_TOOLS: &[&str] = &[
    "remove_training",
    "remove_meal",
    "remove_task",
    "remove_feature_proposal",
];

pub fn parse_body_weight_kg(value: Option<&Value>) -> Result<f64, String> {
    let Some(weight) = value.and_then(|v| v.as_f64()) else {
        return Err("weight_kg is required and must be a positive number".into());
    };
    if !weight.is_finite() || weight <= 0.0 {
        return Err("weight_kg is required and must be a positive number".into());
    }
    if !(MIN_BODY_WEIGHT_KG..=MAX_BODY_WEIGHT_KG).contains(&weight) {
        return Err(format!(
            "weight_kg must be a realistic body weight between {MIN_BODY_WEIGHT_KG} and {MAX_BODY_WEIGHT_KG} kg (got {weight}). Do not log bar/machine loads as body weight. Use log_training for gym sets."
        ));
    }
    Ok(weight)
}

pub fn is_once_per_turn_write(name: &str) -> bool {
    ONCE_PER_TURN_WRITES.contains(&name)
}

pub fn is_capped_read_tool(name: &str) -> bool {
    CAPPED_READ_TOOLS.contains(&name)
}

pub fn is_capped_write_tool(name: &str) -> bool {
    CAPPED_WRITE_TOOLS.contains(&name)
}

pub fn already_called_this_turn_response(name: &str) -> String {
    serde_json::json!({
        "error": format!("{name} already succeeded this turn"),
        "hint": "Do not call this tool again. Use the previous result and continue (for a workout, use log_training).",
    })
    .to_string()
}

pub fn duplicate_call_response(name: &str) -> String {
    serde_json::json!({
        "error": format!("duplicate {name} call with the same arguments"),
        "hint": "The previous result still applies. Do not retry this tool with the same args.",
    })
    .to_string()
}

pub fn read_tool_cap_response(name: &str) -> String {
    serde_json::json!({
        "error": format!("{name} called too many times this turn"),
        "hint": format!(
            "At most {MAX_READ_TOOL_CALLS_PER_TURN} {name} calls per turn. Use the previous results and answer the user. Do not retry with a slightly different query."
        ),
    })
    .to_string()
}

pub fn write_tool_cap_response(name: &str) -> String {
    serde_json::json!({
        "error": format!("{name} called too many times this turn"),
        "hint": format!(
            "At most {MAX_WRITE_TOOL_CALLS_PER_TURN} {name} calls per turn. Confirm what you already deleted and answer the user. Do not keep deleting."
        ),
    })
    .to_string()
}

pub fn tool_call_fingerprint(name: &str, args: &Value) -> String {
    format!("{name}:{args}")
}

pub fn tool_result_succeeded(result: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return false;
    };
    if value.get("error").is_some() {
        return false;
    }
    match value.get("success") {
        Some(flag) => flag.as_bool().unwrap_or(false),
        None => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolGuard {
    Allow,
    Block(String),
}

#[derive(Debug, Default)]
pub struct TurnGuards {
    last_fingerprint: Option<String>,
    succeeded_once_per_turn: HashSet<String>,
    read_counts: HashMap<String, u32>,
    write_counts: HashMap<String, u32>,
    consecutive_blocked: u32,
    total_blocked: u32,
    /// Successful mutations this turn, as `tool #id` when the result has an id.
    succeeded_writes: Vec<String>,
}

impl TurnGuards {
    pub fn decide(&self, name: &str, args: &Value) -> ToolGuard {
        let fingerprint = tool_call_fingerprint(name, args);
        if self.last_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            return ToolGuard::Block(duplicate_call_response(name));
        }
        if is_once_per_turn_write(name) && self.succeeded_once_per_turn.contains(name) {
            return ToolGuard::Block(already_called_this_turn_response(name));
        }
        if is_capped_read_tool(name)
            && self.read_counts.get(name).copied().unwrap_or(0) >= MAX_READ_TOOL_CALLS_PER_TURN
        {
            return ToolGuard::Block(read_tool_cap_response(name));
        }
        if is_capped_write_tool(name)
            && self.write_counts.get(name).copied().unwrap_or(0) >= MAX_WRITE_TOOL_CALLS_PER_TURN
        {
            return ToolGuard::Block(write_tool_cap_response(name));
        }
        ToolGuard::Allow
    }

    pub fn record(&mut self, name: &str, args: &Value, executed: bool, result: &str) {
        self.last_fingerprint = Some(tool_call_fingerprint(name, args));
        if executed {
            self.consecutive_blocked = 0;
            if is_capped_read_tool(name) {
                *self.read_counts.entry(name.to_string()).or_insert(0) += 1;
            }
            if is_capped_write_tool(name) {
                *self.write_counts.entry(name.to_string()).or_insert(0) += 1;
            }
            if is_once_per_turn_write(name) && tool_result_succeeded(result) {
                self.succeeded_once_per_turn.insert(name.to_string());
            }
            if let Some(receipt) = mutation_receipt(name, result) {
                self.succeeded_writes.push(receipt);
            }
        } else {
            self.consecutive_blocked += 1;
            self.total_blocked += 1;
        }
    }

    pub fn should_force_text(&self) -> bool {
        self.consecutive_blocked >= FORCE_TEXT_AFTER_CONSECUTIVE_BLOCKS
            || self.total_blocked >= FORCE_TEXT_AFTER_TOTAL_BLOCKS
    }

    /// Comma-separated successful writes, capped so a runaway turn stays short.
    pub fn writes_summary(&self) -> Option<String> {
        if self.succeeded_writes.is_empty() {
            return None;
        }
        let mut summary = self
            .succeeded_writes
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if self.succeeded_writes.len() > 8 {
            summary.push_str(", …");
        }
        Some(summary)
    }
}

fn is_mutation_tool(name: &str) -> bool {
    name.starts_with("log_")
        || name.starts_with("remove_")
        || name.starts_with("create_")
        || name.starts_with("update_")
        || name == "send_coach_image"
}

/// `remove_training #261` when a mutating tool succeeded and the result names an id.
pub fn mutation_receipt(name: &str, result: &str) -> Option<String> {
    if !is_mutation_tool(name) || !tool_result_succeeded(result) {
        return None;
    }
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return Some(name.to_string());
    };
    match extract_entity_id(&value) {
        Some(id) => Some(format!("{name} #{id}")),
        None => Some(name.to_string()),
    }
}

fn extract_entity_id(value: &Value) -> Option<i64> {
    if let Some(message) = value.get("message").and_then(|v| v.as_str())
        && let Some(id) = id_after_hash(message)
    {
        return Some(id);
    }
    for key in [
        "meal",
        "training_entry",
        "weight_entry",
        "reminder",
        "task",
        "proposal",
        "food",
    ] {
        if let Some(id) = value
            .get(key)
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_i64())
        {
            return Some(id);
        }
    }
    value.get("id").and_then(|v| v.as_i64())
}

fn id_after_hash(message: &str) -> Option<i64> {
    let (_, rest) = message.split_once('#')?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_missing_and_non_positive_weight() {
        assert!(parse_body_weight_kg(None).is_err());
        assert!(parse_body_weight_kg(Some(&json!(0))).is_err());
        assert!(parse_body_weight_kg(Some(&json!(-10))).is_err());
        assert!(parse_body_weight_kg(Some(&json!("82.5"))).is_err());
    }

    #[test]
    fn rejects_hallucinated_one_kg() {
        let err = parse_body_weight_kg(Some(&json!(1.0))).unwrap_err();
        assert!(err.contains("realistic body weight"));
        assert!(err.contains("log_training"));
    }

    #[test]
    fn accepts_typical_body_weight() {
        assert_eq!(parse_body_weight_kg(Some(&json!(82.5))).unwrap(), 82.5);
        assert_eq!(parse_body_weight_kg(Some(&json!(20.0))).unwrap(), 20.0);
        assert_eq!(parse_body_weight_kg(Some(&json!(400.0))).unwrap(), 400.0);
    }

    #[test]
    fn once_per_turn_covers_log_weight() {
        assert!(is_once_per_turn_write("log_weight"));
        assert!(is_once_per_turn_write("send_coach_image"));
        assert!(!is_once_per_turn_write("get_training_entries"));
        assert!(!is_once_per_turn_write("log_training"));
        assert!(!is_once_per_turn_write("search_photos"));
    }

    #[test]
    fn capped_reads_include_search_photos() {
        assert!(is_capped_read_tool("search_photos"));
        assert!(is_capped_read_tool("search_foods"));
        assert!(is_capped_read_tool("get_training_entries"));
        assert!(!is_capped_read_tool("log_training"));
        assert!(!is_capped_read_tool("log_weight"));
        assert!(!is_capped_read_tool("log_meal"));
    }

    #[test]
    fn capped_writes_are_destructive_only() {
        assert!(is_capped_write_tool("remove_training"));
        assert!(is_capped_write_tool("remove_meal"));
        assert!(is_capped_write_tool("remove_task"));
        assert!(is_capped_write_tool("remove_feature_proposal"));
        assert!(!is_capped_write_tool("log_training"));
        assert!(!is_capped_write_tool("log_meal"));
        assert!(!is_capped_write_tool("search_photos"));
    }

    #[test]
    fn success_detection() {
        assert!(tool_result_succeeded(r#"{"success":true,"id":1}"#));
        assert!(!tool_result_succeeded(
            r#"{"error":"weight_kg is required and must be a positive number"}"#
        ));
        assert!(tool_result_succeeded(r#"{"weight_entries":[]}"#));
        assert!(!tool_result_succeeded("not-json"));
    }

    fn allow_and_record(guards: &mut TurnGuards, name: &str, args: &Value) {
        assert_eq!(guards.decide(name, args), ToolGuard::Allow);
        guards.record(name, args, true, r#"{"hits":[]}"#);
    }

    #[test]
    fn search_photos_cap_matches_prod_loop() {
        let mut guards = TurnGuards::default();
        for query in ["Haut B", "9 septembre", "épaules"] {
            allow_and_record(&mut guards, "search_photos", &json!({"query": query}));
        }
        let fourth = guards.decide("search_photos", &json!({"query": "triceps"}));
        match fourth {
            ToolGuard::Block(body) => {
                assert!(body.contains("too many times"));
                assert!(body.contains("search_photos"));
            }
            ToolGuard::Allow => panic!("fourth distinct search_photos must be blocked"),
        }
        guards.record("search_photos", &json!({"query": "triceps"}), false, "");
        let fifth = guards.decide("search_photos", &json!({"query": "oiseau"}));
        assert!(matches!(fifth, ToolGuard::Block(_)));
        guards.record("search_photos", &json!({"query": "oiseau"}), false, "");
        assert!(guards.should_force_text());
    }

    #[test]
    fn read_cap_is_per_tool_name() {
        let mut guards = TurnGuards::default();
        for i in 0..MAX_READ_TOOL_CALLS_PER_TURN {
            allow_and_record(
                &mut guards,
                "get_training_entries",
                &json!({"limit": i + 1}),
            );
        }
        assert!(matches!(
            guards.decide("get_training_entries", &json!({"limit": 99})),
            ToolGuard::Block(_)
        ));
        assert_eq!(
            guards.decide("search_photos", &json!({"query": "garmin"})),
            ToolGuard::Allow
        );
    }

    #[test]
    fn write_tools_are_not_read_capped() {
        let mut guards = TurnGuards::default();
        for i in 0..5 {
            let args = json!({"exercise": format!("set-{i}")});
            assert_eq!(guards.decide("log_training", &args), ToolGuard::Allow);
            guards.record("log_training", &args, true, r#"{"success":true}"#);
        }
        assert!(!guards.should_force_text());
    }

    #[test]
    fn duplicate_fingerprint_still_blocks() {
        let mut guards = TurnGuards::default();
        let args = json!({"query": "strava"});
        allow_and_record(&mut guards, "search_photos", &args);
        match guards.decide("search_photos", &args) {
            ToolGuard::Block(body) => assert!(body.contains("duplicate")),
            ToolGuard::Allow => panic!("identical search_photos must be a duplicate"),
        }
    }

    #[test]
    fn once_per_turn_write_still_blocks_second_success() {
        let mut guards = TurnGuards::default();
        let first = json!({"weight_kg": 82.5});
        assert_eq!(guards.decide("log_weight", &first), ToolGuard::Allow);
        guards.record("log_weight", &first, true, r#"{"success":true,"id":1}"#);
        let second = json!({"weight_kg": 83.0});
        match guards.decide("log_weight", &second) {
            ToolGuard::Block(body) => assert!(body.contains("already succeeded")),
            ToolGuard::Allow => panic!("second log_weight must be blocked"),
        }
    }

    #[test]
    fn execute_after_block_resets_force_text() {
        let mut guards = TurnGuards::default();
        let args = json!({"query": "a"});
        guards.record("search_photos", &args, false, "");
        guards.record("search_photos", &json!({"query": "b"}), false, "");
        assert!(guards.should_force_text());
        allow_and_record(&mut guards, "get_tasks", &json!({}));
        assert!(!guards.should_force_text());
    }

    #[test]
    fn remove_training_cap_matches_prod_loop() {
        let mut guards = TurnGuards::default();
        for id in [101, 102] {
            let args = json!({"id": id});
            assert_eq!(guards.decide("remove_training", &args), ToolGuard::Allow);
            guards.record("remove_training", &args, true, r#"{"success":true}"#);
        }
        let third = guards.decide("remove_training", &json!({"id": 103}));
        match third {
            ToolGuard::Block(body) => {
                assert!(body.contains("too many times"));
                assert!(body.contains("remove_training"));
            }
            ToolGuard::Allow => panic!("third remove_training must be blocked"),
        }
        guards.record("remove_training", &json!({"id": 103}), false, "");
        let fourth = guards.decide("remove_training", &json!({"id": 104}));
        assert!(matches!(fourth, ToolGuard::Block(_)));
        guards.record("remove_training", &json!({"id": 104}), false, "");
        assert!(guards.should_force_text());
    }

    #[test]
    fn interleaved_get_training_entry_hits_total_block_cap() {
        let mut guards = TurnGuards::default();
        for i in 0..MAX_READ_TOOL_CALLS_PER_TURN {
            allow_and_record(&mut guards, "get_training_entry", &json!({"id": i + 1}));
        }
        let blocked = guards.decide("get_training_entry", &json!({"id": 99}));
        assert!(matches!(blocked, ToolGuard::Block(_)));
        guards.record("get_training_entry", &json!({"id": 99}), false, "");
        assert!(!guards.should_force_text());

        let remove = json!({"id": 7});
        assert_eq!(guards.decide("remove_training", &remove), ToolGuard::Allow);
        guards.record("remove_training", &remove, true, r#"{"success":true}"#);
        assert!(!guards.should_force_text());

        guards.record("get_training_entry", &json!({"id": 98}), false, "");
        assert!(!guards.should_force_text());
        allow_and_record(
            &mut guards,
            "get_group_training_stats",
            &json!({"period": "week"}),
        );
        guards.record("get_training_entry", &json!({"id": 97}), false, "");
        assert!(guards.should_force_text());
    }

    #[test]
    fn successful_deletes_are_listed_for_the_fallback() {
        let mut guards = TurnGuards::default();
        guards.record(
            "remove_training",
            &json!({"training_id": 261}),
            true,
            r#"{"success":true,"message":"Training #261 deleted"}"#,
        );
        guards.record(
            "remove_training",
            &json!({"training_id": 303}),
            true,
            r#"{"success":true,"message":"Training #303 deleted"}"#,
        );
        guards.record(
            "get_training_entries",
            &json!({"limit": 10}),
            true,
            r#"{"training_entries":[]}"#,
        );
        guards.record(
            "remove_training",
            &json!({"training_id": 9}),
            true,
            r#"{"error":"Training not found"}"#,
        );
        assert_eq!(
            guards.writes_summary().as_deref(),
            Some("remove_training #261, remove_training #303")
        );
    }

    #[test]
    fn meal_and_training_receipts_use_nested_ids() {
        assert_eq!(
            mutation_receipt("log_meal", r#"{"success":true,"meal":{"id":104}}"#).as_deref(),
            Some("log_meal #104")
        );
        assert_eq!(
            mutation_receipt(
                "log_training",
                r#"{"success":true,"training_entry":{"id":310}}"#,
            )
            .as_deref(),
            Some("log_training #310")
        );
        assert!(mutation_receipt("search_photos", r#"{"photos":[]}"#).is_none());
    }
}
