use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

use crate::db::strength_exercises::StrengthExercise;

pub const SLUG_OTHER: &str = "other";

/// Double-tap / webhook / LLM calling log_training twice.
const DEDUP_BURST: Duration = Duration::seconds(45);
/// Same normalized description as the latest session, within this window.
const DEDUP_SAME_DESC: Duration = Duration::hours(2);

#[derive(Debug, Clone)]
pub enum TrainingInsert {
    Created(TrainingEntry),
    Duplicate(TrainingEntry),
}

impl TrainingInsert {
    #[cfg(test)]
    pub fn entry(self) -> TrainingEntry {
        match self {
            Self::Created(e) | Self::Duplicate(e) => e,
        }
    }

    #[cfg(test)]
    pub fn is_duplicate(&self) -> bool {
        matches!(self, Self::Duplicate(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityType {
    pub id: i64,
    pub slug: String,
    pub uses_endurance: bool,
    pub uses_strength: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnduranceStats {
    pub duration_sec: Option<i64>,
    pub distance_m: Option<f64>,
    pub elevation_m: Option<f64>,
    pub avg_hr: Option<i64>,
    pub calories: Option<i64>,
}

impl EnduranceStats {
    pub fn is_empty(&self) -> bool {
        self.duration_sec.is_none()
            && self.distance_m.is_none()
            && self.elevation_m.is_none()
            && self.avg_hr.is_none()
            && self.calories.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrengthSet {
    pub id: i64,
    pub exercise: StrengthExercise,
    pub set_index: i64,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStrengthSet {
    pub exercise: StrengthExercise,
    pub set_index: i64,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
}

impl NewStrengthSet {
    pub fn new(
        exercise: StrengthExercise,
        set_index: i64,
        reps: Option<i64>,
        weight_kg: Option<f64>,
    ) -> Self {
        Self {
            exercise,
            set_index,
            reps,
            weight_kg,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingEntry {
    pub id: i64,
    pub user_id: i64,
    pub description: String,
    pub recorded_at: DateTime<Utc>,
    pub activity_type_id: i64,
    pub activity_slug: String,
    pub endurance: Option<EnduranceStats>,
    pub strength_sets: Vec<StrengthSet>,
}

#[derive(Debug, Deserialize)]
pub struct NewTrainingEntry {
    pub user_id: i64,
    pub description: String,
    pub recorded_at: Option<DateTime<Utc>>,
    pub activity_slug: Option<String>,
    pub endurance: Option<EnduranceStats>,
    pub strength_sets: Vec<NewStrengthSet>,
}

impl NewTrainingEntry {
    pub fn simple(user_id: i64, description: String, recorded_at: Option<DateTime<Utc>>) -> Self {
        Self {
            user_id,
            description,
            recorded_at,
            activity_slug: None,
            endurance: None,
            strength_sets: Vec::new(),
        }
    }
}

pub async fn find_activity_by_slug(pool: &SqlitePool, slug: &str) -> Result<Option<ActivityType>> {
    let row = sqlx::query!(
        r#"
        SELECT id, slug, uses_endurance, uses_strength
        FROM activity_types
        WHERE slug = ?
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ActivityType {
        id: r.id.expect("activity_types.id"),
        slug: r.slug,
        uses_endurance: r.uses_endurance != 0,
        uses_strength: r.uses_strength != 0,
    }))
}

async fn require_activity(pool: &SqlitePool, slug: &str) -> Result<ActivityType> {
    if let Some(at) = find_activity_by_slug(pool, slug).await? {
        return Ok(at);
    }
    find_activity_by_slug(pool, SLUG_OTHER)
        .await?
        .ok_or_else(|| anyhow::anyhow!("activity type '{SLUG_OTHER}' is missing"))
}

#[cfg(test)]
pub async fn insert(pool: &SqlitePool, entry: NewTrainingEntry) -> Result<TrainingEntry> {
    Ok(try_insert(pool, entry).await?.entry())
}

/// Whole-word hints. A bare `km` is not a run: gym notes use it too.
const ACTIVITY_HINTS: &[(&str, &[&str])] = &[
    (
        "run",
        &[
            "course", "courses", "courir", "footing", "jogging", "running", "trail",
        ],
    ),
    (
        "ride",
        &["velo", "vtt", "bike", "biking", "cycling", "cyclisme"],
    ),
    ("hike", &["randonnee", "hike", "hiking"]),
    ("walk", &["walk", "walking", "promenade"]),
];

fn fold_mark(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'ï' | 'î' | 'í' => 'i',
        'ô' | 'ö' | 'ó' | 'õ' => 'o',
        'ù' | 'û' | 'ü' | 'ú' => 'u',
        'ç' => 'c',
        'ÿ' => 'y',
        other => other,
    }
}

fn description_tokens(description: &str) -> Vec<String> {
    description
        .to_lowercase()
        .chars()
        .map(fold_mark)
        .collect::<String>()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// When the model omits the activity, or stores `other`, pick run/ride/hike/walk
/// from an unambiguous description. Two different sports in one note stay `other`.
fn infer_activity_slug(description: &str) -> Option<&'static str> {
    let tokens = description_tokens(description);
    let mut found: Option<&'static str> = None;
    for (slug, words) in ACTIVITY_HINTS {
        if words
            .iter()
            .any(|word| tokens.iter().any(|token| token == word))
        {
            if found.is_some_and(|previous| previous != *slug) {
                return None;
            }
            found = Some(*slug);
        }
    }
    found
}

fn activity_slug_is_other(slug: Option<&str>) -> bool {
    match slug.map(str::trim).filter(|s| !s.is_empty()) {
        None => true,
        Some(s) => s.eq_ignore_ascii_case(SLUG_OTHER),
    }
}

fn with_inferred_activity(mut entry: NewTrainingEntry) -> NewTrainingEntry {
    if activity_slug_is_other(entry.activity_slug.as_deref())
        && let Some(slug) = infer_activity_slug(&entry.description)
    {
        entry.activity_slug = Some(slug.to_string());
    }
    entry
}

/// `other` cannot store sets. A payload that brings loads becomes gym, or
/// crossfit when it also brings endurance stats.
fn promote_other_for_sets(slug: &str, has_endurance: bool, has_sets: bool) -> String {
    if !activity_slug_is_other(Some(slug)) || !has_sets {
        return slug.to_string();
    }
    if has_endurance {
        "crossfit".to_string()
    } else {
        "gym".to_string()
    }
}

/// Classify a brand-new row. Duplicate merges keep the caller's slug so a
/// guessed run cannot overwrite the previous gym session.
fn classify_new_session(entry: NewTrainingEntry) -> NewTrainingEntry {
    let mut entry = with_inferred_activity(entry);
    let slug = entry
        .activity_slug
        .clone()
        .unwrap_or_else(|| SLUG_OTHER.to_string());
    let has_endurance = entry.endurance.as_ref().is_some_and(|e| !e.is_empty());
    entry.activity_slug = Some(promote_other_for_sets(
        &slug,
        has_endurance,
        !entry.strength_sets.is_empty(),
    ));
    entry
}

/// Move existing `other` rows onto run/ride/hike/walk when the description is obvious.
pub async fn reclassify_other_activities(pool: &SqlitePool) -> Result<u64> {
    let rows = sqlx::query(
        r#"SELECT te.id AS id, te.description AS description
           FROM training_entries te
           INNER JOIN activity_types a ON a.id = te.activity_type_id
           WHERE a.slug = 'other'"#,
    )
    .fetch_all(pool)
    .await?;

    let mut updated = 0u64;
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let description: String = row.try_get("description")?;
        let Some(slug) = infer_activity_slug(&description) else {
            continue;
        };
        let result = sqlx::query(
            r#"UPDATE training_entries
               SET activity_type_id = (SELECT id FROM activity_types WHERE slug = ?)
               WHERE id = ?
                 AND activity_type_id = (SELECT id FROM activity_types WHERE slug = 'other')"#,
        )
        .bind(slug)
        .bind(id)
        .execute(pool)
        .await?;
        updated += result.rows_affected();
    }
    Ok(updated)
}

/// Insert a session, or reuse the latest one when it looks like a consecutive duplicate.
pub async fn try_insert(pool: &SqlitePool, entry: NewTrainingEntry) -> Result<TrainingInsert> {
    let recorded_at = entry.recorded_at.unwrap_or_else(Utc::now);
    if let Some(existing) = latest_for_user(pool, entry.user_id).await?
        && is_consecutive_duplicate(&existing, &entry.description, recorded_at)
    {
        let merged = merge_duplicate_details(pool, &existing, &entry).await?;
        return Ok(TrainingInsert::Duplicate(merged));
    }

    let entry = classify_new_session(entry);

    let recorded_at_str = recorded_at.to_rfc3339();
    let slug = entry.activity_slug.as_deref().unwrap_or(SLUG_OTHER);
    let activity = require_activity(pool, slug).await?;

    let id = sqlx::query!(
        "INSERT INTO training_entries (user_id, description, recorded_at, activity_type_id) VALUES (?, ?, ?, ?)",
        entry.user_id,
        entry.description,
        recorded_at_str,
        activity.id,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    write_satellites(
        pool,
        id,
        &activity,
        entry.endurance.as_ref(),
        &entry.strength_sets,
        true,
    )
    .await?;

    let created = find_by_id(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("inserted training {id} not found"))?;
    Ok(TrainingInsert::Created(created))
}

fn normalize_description(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn is_consecutive_duplicate(
    last: &TrainingEntry,
    new_description: &str,
    new_at: DateTime<Utc>,
) -> bool {
    let delta = (new_at - last.recorded_at).num_seconds().abs();
    if delta < DEDUP_BURST.num_seconds() {
        return true;
    }
    delta < DEDUP_SAME_DESC.num_seconds()
        && last.recorded_at.date_naive() == new_at.date_naive()
        && normalize_description(&last.description) == normalize_description(new_description)
}

async fn merge_duplicate_details(
    pool: &SqlitePool,
    existing: &TrainingEntry,
    incoming: &NewTrainingEntry,
) -> Result<TrainingEntry> {
    let activity_slug = incoming
        .activity_slug
        .as_deref()
        .filter(|s| !s.is_empty() && *s != SLUG_OTHER && *s != existing.activity_slug.as_str())
        .map(|s| s.to_string());
    let endurance = incoming
        .endurance
        .as_ref()
        .filter(|e| !e.is_empty())
        .cloned();
    let strength_sets = if incoming.strength_sets.is_empty() {
        None
    } else {
        Some(incoming.strength_sets.clone())
    };

    if activity_slug.is_none() && endurance.is_none() && strength_sets.is_none() {
        return Ok(existing.clone());
    }

    Ok(update(
        pool,
        existing.id,
        existing.user_id,
        true,
        UpdateTraining {
            activity_slug,
            endurance,
            strength_sets,
            ..Default::default()
        },
    )
    .await?
    .unwrap_or_else(|| existing.clone()))
}

async fn write_satellites(
    pool: &SqlitePool,
    training_id: i64,
    activity: &ActivityType,
    endurance: Option<&EnduranceStats>,
    sets: &[NewStrengthSet],
    replace: bool,
) -> Result<()> {
    if replace {
        sqlx::query!(
            "DELETE FROM training_endurance WHERE training_id = ?",
            training_id
        )
        .execute(pool)
        .await?;
        sqlx::query!(
            "DELETE FROM training_strength_sets WHERE training_id = ?",
            training_id
        )
        .execute(pool)
        .await?;
    }

    if activity.uses_endurance
        && let Some(e) = endurance
        && !e.is_empty()
    {
        sqlx::query!(
            r#"
            INSERT INTO training_endurance
                (training_id, duration_sec, distance_m, elevation_m, avg_hr, calories)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            training_id,
            e.duration_sec,
            e.distance_m,
            e.elevation_m,
            e.avg_hr,
            e.calories,
        )
        .execute(pool)
        .await?;
    }

    if activity.uses_strength {
        let catalog = load_exercise_catalog(pool).await?;
        let other_id = catalog
            .get(StrengthExercise::Other.slug())
            .copied()
            .unwrap_or(30);
        for set in sets {
            let exercise_id = catalog
                .get(set.exercise.slug())
                .copied()
                .unwrap_or(other_id);
            let stored_name = set.exercise.display_name();
            sqlx::query!(
                r#"
                INSERT INTO training_strength_sets
                    (training_id, exercise_name, set_index, reps, weight_kg, exercise_id)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
                training_id,
                stored_name,
                set.set_index,
                set.reps,
                set.weight_kg,
                exercise_id,
            )
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

fn parse_recorded_at(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
}

async fn hydrate(pool: &SqlitePool, mut entries: Vec<TrainingEntry>) -> Result<Vec<TrainingEntry>> {
    if entries.is_empty() {
        return Ok(entries);
    }

    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
    let endurance_map = load_endurance_for_ids(pool, &ids).await?;
    let sets_map = load_sets_for_ids(pool, &ids).await?;

    for entry in &mut entries {
        entry.endurance = endurance_map.get(&entry.id).cloned();
        entry.strength_sets = sets_map.get(&entry.id).cloned().unwrap_or_default();
    }
    Ok(entries)
}

async fn load_endurance_for_ids(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<HashMap<i64, EnduranceStats>> {
    let mut map = HashMap::new();
    for id in ids {
        let row = sqlx::query!(
            r#"
            SELECT training_id, duration_sec, distance_m, elevation_m, avg_hr, calories
            FROM training_endurance
            WHERE training_id = ?
            "#,
            id,
        )
        .fetch_optional(pool)
        .await?;
        if let Some(r) = row {
            map.insert(
                r.training_id,
                EnduranceStats {
                    duration_sec: r.duration_sec,
                    distance_m: r.distance_m,
                    elevation_m: r.elevation_m,
                    avg_hr: r.avg_hr,
                    calories: r.calories,
                },
            );
        }
    }
    Ok(map)
}

async fn load_exercise_catalog(pool: &SqlitePool) -> Result<HashMap<String, i64>> {
    let rows = sqlx::query!("SELECT id, slug FROM strength_exercises")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.slug, r.id)).collect())
}

async fn load_sets_for_ids(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<StrengthSet>>> {
    let mut map: HashMap<i64, Vec<StrengthSet>> = HashMap::new();
    for id in ids {
        let rows = sqlx::query!(
            r#"
            SELECT s.id, s.training_id, s.set_index, s.reps, s.weight_kg,
                   e.slug AS exercise_slug
            FROM training_strength_sets s
            JOIN strength_exercises e ON e.id = s.exercise_id
            WHERE s.training_id = ?
            ORDER BY s.set_index ASC, s.id ASC
            "#,
            id,
        )
        .fetch_all(pool)
        .await?;
        let sets: Vec<StrengthSet> = rows
            .into_iter()
            .map(|r| StrengthSet {
                id: r.id.expect("training_strength_sets.id"),
                exercise: StrengthExercise::from_slug(&r.exercise_slug)
                    .unwrap_or(StrengthExercise::Other),
                set_index: r.set_index,
                reps: r.reps,
                weight_kg: r.weight_kg,
            })
            .collect();
        if !sets.is_empty() {
            map.insert(*id, sets);
        }
    }
    Ok(map)
}

fn map_entry_row(
    id: i64,
    user_id: i64,
    description: String,
    recorded_at: String,
    activity_type_id: i64,
    activity_slug: String,
) -> TrainingEntry {
    TrainingEntry {
        id,
        user_id,
        description,
        recorded_at: parse_recorded_at(&recorded_at),
        activity_type_id,
        activity_slug,
        endurance: None,
        strength_sets: Vec::new(),
    }
}

pub async fn latest_for_user(pool: &SqlitePool, user_id: i64) -> Result<Option<TrainingEntry>> {
    let row = sqlx::query!(
        r#"
        SELECT te.id, te.user_id, te.description, te.recorded_at, te.activity_type_id,
               at.slug AS activity_slug
        FROM training_entries te
        JOIN activity_types at ON at.id = te.activity_type_id
        WHERE te.user_id = ?
        ORDER BY te.recorded_at DESC
        LIMIT 1
        "#,
        user_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };
    let entry = map_entry_row(
        r.id,
        r.user_id,
        r.description,
        r.recorded_at,
        r.activity_type_id,
        r.activity_slug,
    );
    Ok(hydrate(pool, vec![entry]).await?.into_iter().next())
}

pub async fn list_for_user(pool: &SqlitePool, user_id: i64) -> Result<Vec<TrainingEntry>> {
    let rows = sqlx::query!(
        r#"
        SELECT te.id, te.user_id, te.description, te.recorded_at, te.activity_type_id,
               at.slug AS activity_slug
        FROM training_entries te
        JOIN activity_types at ON at.id = te.activity_type_id
        WHERE te.user_id = ?
        ORDER BY te.recorded_at DESC
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await?;

    let entries = rows
        .into_iter()
        .map(|r| {
            map_entry_row(
                r.id,
                r.user_id,
                r.description,
                r.recorded_at,
                r.activity_type_id,
                r.activity_slug,
            )
        })
        .collect();
    hydrate(pool, entries).await
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<TrainingEntry>> {
    let row = sqlx::query!(
        r#"
        SELECT te.id, te.user_id, te.description, te.recorded_at, te.activity_type_id,
               at.slug AS activity_slug
        FROM training_entries te
        JOIN activity_types at ON at.id = te.activity_type_id
        WHERE te.id = ?
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(None);
    };
    let entry = map_entry_row(
        r.id,
        r.user_id,
        r.description,
        r.recorded_at,
        r.activity_type_id,
        r.activity_slug,
    );
    Ok(hydrate(pool, vec![entry]).await?.into_iter().next())
}

fn can_modify(entry: &TrainingEntry, actor_user_id: i64, is_admin: bool) -> bool {
    is_admin || entry.user_id == actor_user_id
}

#[derive(Debug, Default)]
pub struct UpdateTraining {
    pub description: Option<String>,
    pub recorded_at: Option<DateTime<Utc>>,
    pub activity_slug: Option<String>,
    pub endurance: Option<EnduranceStats>,
    pub strength_sets: Option<Vec<NewStrengthSet>>,
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    actor_user_id: i64,
    is_admin: bool,
    updates: UpdateTraining,
) -> Result<Option<TrainingEntry>> {
    let Some(existing) = find_by_id(pool, id).await? else {
        return Ok(None);
    };
    if !can_modify(&existing, actor_user_id, is_admin) {
        return Ok(None);
    }

    let description = updates
        .description
        .unwrap_or_else(|| existing.description.clone());
    let recorded_at = updates.recorded_at.unwrap_or(existing.recorded_at);
    let recorded_at_str = recorded_at.to_rfc3339();
    let requested = updates
        .activity_slug
        .clone()
        .unwrap_or_else(|| existing.activity_slug.clone());
    let adding_sets = updates
        .strength_sets
        .as_ref()
        .is_some_and(|sets| !sets.is_empty());
    let has_endurance = updates
        .endurance
        .as_ref()
        .is_some_and(|stats| !stats.is_empty())
        || existing.endurance.is_some();
    let slug = promote_other_for_sets(&requested, has_endurance, adding_sets);
    let activity = require_activity(pool, &slug).await?;

    sqlx::query!(
        "UPDATE training_entries SET description = ?, recorded_at = ?, activity_type_id = ? WHERE id = ?",
        description,
        recorded_at_str,
        activity.id,
        id,
    )
    .execute(pool)
    .await?;

    let replace_sats = updates.endurance.is_some()
        || updates.strength_sets.is_some()
        || activity.id != existing.activity_type_id;

    if replace_sats {
        let endurance = updates.endurance.or(existing.endurance.clone());
        let sets: Vec<NewStrengthSet> = updates.strength_sets.unwrap_or_else(|| {
            existing
                .strength_sets
                .iter()
                .map(|s| NewStrengthSet {
                    exercise: s.exercise,
                    set_index: s.set_index,
                    reps: s.reps,
                    weight_kg: s.weight_kg,
                })
                .collect()
        });
        write_satellites(pool, id, &activity, endurance.as_ref(), &sets, true).await?;
    }

    find_by_id(pool, id).await
}

pub async fn delete(
    pool: &SqlitePool,
    id: i64,
    actor_user_id: i64,
    is_admin: bool,
) -> Result<bool> {
    let Some(existing) = find_by_id(pool, id).await? else {
        return Ok(false);
    };
    if !can_modify(&existing, actor_user_id, is_admin) {
        return Ok(false);
    }

    sqlx::query!("DELETE FROM training_endurance WHERE training_id = ?", id)
        .execute(pool)
        .await?;
    sqlx::query!(
        "DELETE FROM training_strength_sets WHERE training_id = ?",
        id
    )
    .execute(pool)
    .await?;
    sqlx::query!("DELETE FROM training_entries WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(true)
}

/// Human-readable stats after the description (empty if none).
pub fn stats_suffix(entry: &TrainingEntry) -> String {
    let mut parts = Vec::new();
    if let Some(e) = &entry.endurance {
        if let Some(m) = e.distance_m {
            parts.push(format_distance_km(m));
        }
        if let Some(sec) = e.duration_sec {
            parts.push(format_duration(sec));
        }
        if let Some(elev) = e.elevation_m
            && elev.abs() >= 0.5
        {
            parts.push(format!("D+{:.0}", elev));
        }
    }
    if !entry.strength_sets.is_empty() {
        parts.push(summarize_sets(&entry.strength_sets));
    }
    parts.join(" · ")
}

/// Description always first, then optional structured stats.
pub fn summary_line(entry: &TrainingEntry) -> String {
    let suffix = stats_suffix(entry);
    if suffix.is_empty() {
        entry.description.clone()
    } else {
        format!("{} · {suffix}", entry.description)
    }
}

/// Full session card: description, meta, optional endurance + grouped sets.
pub fn format_card(entry: &TrainingEntry) -> String {
    let mut lines = vec![
        entry.description.clone(),
        format!(
            "#{} · {} · {}",
            entry.id,
            entry.recorded_at.format("%Y-%m-%d"),
            entry.activity_slug
        ),
    ];

    let mut body: Vec<String> = Vec::new();
    if let Some(e) = &entry.endurance
        && let Some(line) = format_endurance_card_line(e)
    {
        body.push(line);
    }
    body.extend(format_grouped_sets(&entry.strength_sets));

    if !body.is_empty() {
        lines.push(String::new());
        lines.extend(body);
    }
    lines.join("\n")
}

fn format_distance_km(meters: f64) -> String {
    let km = meters / 1000.0;
    if (km - km.round()).abs() < 1e-6 {
        format!("{km:.0} km")
    } else {
        format!("{km:.1} km")
    }
}

fn format_duration(sec: i64) -> String {
    let sec = sec.max(0);
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    let s = sec % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn format_weight_kg(w: f64) -> String {
    if (w - w.round()).abs() < 1e-6 {
        format!("{}kg", w.round() as i64)
    } else {
        format!("{w}kg")
    }
}

fn format_pace(duration_sec: i64, distance_m: f64) -> Option<String> {
    let km = distance_m / 1000.0;
    if km <= 0.0 || duration_sec <= 0 {
        return None;
    }
    let total = (duration_sec as f64 / km).round() as i64;
    let m = total / 60;
    let s = total % 60;
    Some(format!("{m}:{s:02}/km"))
}

fn format_endurance_card_line(e: &EnduranceStats) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(m) = e.distance_m {
        parts.push(format_distance_km(m));
    }
    if let Some(sec) = e.duration_sec {
        parts.push(format_duration(sec));
    }
    if let (Some(sec), Some(dist)) = (e.duration_sec, e.distance_m)
        && let Some(pace) = format_pace(sec, dist)
    {
        parts.push(pace);
    }
    if let Some(elev) = e.elevation_m
        && elev.abs() >= 0.5
    {
        parts.push(format!("D+{:.0}", elev));
    }
    if let Some(hr) = e.avg_hr {
        parts.push(format!("{hr} bpm"));
    }
    if let Some(cal) = e.calories {
        parts.push(format!("{cal} kcal"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn sets_identical(group: &[&StrengthSet]) -> bool {
    let Some(first) = group.first() else {
        return true;
    };
    group.iter().all(|s| {
        s.reps == first.reps
            && match (s.weight_kg, first.weight_kg) {
                (None, None) => true,
                (Some(a), Some(b)) => (a - b).abs() < 1e-6,
                _ => false,
            }
    })
}

fn format_one_set(set: &StrengthSet) -> String {
    match (set.reps, set.weight_kg) {
        (Some(r), Some(w)) => format!("{r} @ {}", format_weight_kg(w)),
        (Some(r), None) => r.to_string(),
        (None, Some(w)) => format!("@ {}", format_weight_kg(w)),
        (None, None) => "?".into(),
    }
}

fn format_identical_group(n: usize, sample: &StrengthSet) -> String {
    match (sample.reps, sample.weight_kg) {
        (Some(r), Some(w)) => format!("{n}×{r} @ {}", format_weight_kg(w)),
        (Some(r), None) => format!("{n}×{r}"),
        (None, Some(w)) => format!("{n}× @ {}", format_weight_kg(w)),
        (None, None) => format!("{n}×?"),
    }
}

fn format_grouped_sets(sets: &[StrengthSet]) -> Vec<String> {
    if sets.is_empty() {
        return Vec::new();
    }

    let mut order: Vec<StrengthExercise> = Vec::new();
    let mut by_ex: HashMap<StrengthExercise, Vec<&StrengthSet>> = HashMap::new();
    for set in sets {
        if !order.contains(&set.exercise) {
            order.push(set.exercise);
        }
        by_ex.entry(set.exercise).or_default().push(set);
    }

    let labels: Vec<(&str, String)> = order
        .iter()
        .map(|ex| {
            let group = by_ex.get(ex).expect("grouped sets");
            let detail = if sets_identical(group) {
                format_identical_group(group.len(), group[0])
            } else {
                group
                    .iter()
                    .map(|s| format_one_set(s))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            (ex.display_name(), detail)
        })
        .collect();

    let width = labels
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);

    labels
        .into_iter()
        .map(|(name, detail)| format!("{name:<width$}  {detail}"))
        .collect()
}

fn summarize_sets(sets: &[StrengthSet]) -> String {
    let mut names: Vec<String> = Vec::new();
    for set in sets {
        let label = set.exercise.display_name().to_string();
        if !names.iter().any(|n| n == &label) {
            names.push(label);
        }
    }
    if names.len() == 1 {
        let name = &names[0];
        let of_ex: Vec<&StrengthSet> = sets
            .iter()
            .filter(|s| s.exercise == sets[0].exercise)
            .collect();
        let n = of_ex.len();
        let reps = of_ex.iter().find_map(|s| s.reps);
        let weight = of_ex.iter().rev().find_map(|s| s.weight_kg);
        let mut s = format!(
            "{name} {n}×{}",
            reps.map(|r| r.to_string()).unwrap_or_else(|| "?".into())
        );
        if let Some(w) = weight {
            s.push_str(&format!(" @ {w}kg"));
        }
        return s;
    }
    const MAX: usize = 4;
    if names.len() > MAX {
        format!("{}, …", names[..MAX].join(", "))
    } else {
        names.join(", ")
    }
}

#[derive(Debug, Clone)]
pub struct GroupTrainingStat {
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub sessions: i64,
}

impl GroupTrainingStat {
    /// Prefer `@username`, then Telegram first name, then a generic fallback.
    /// Never uses the numeric Telegram / user id.
    pub fn display_name(&self) -> String {
        if let Some(username) = self
            .username
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
        {
            let bare = username.trim_start_matches('@').to_lowercase();
            return format!("@{bare}");
        }
        if let Some(first_name) = self
            .first_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            return first_name.to_string();
        }
        "Member".to_string()
    }
}

/// Session counts for group members in `[start, end)` (RFC3339 timestamps), most sessions first.
pub async fn sessions_by_group_member(
    pool: &SqlitePool,
    chat_id: i64,
    start: &str,
    end: &str,
) -> Result<Vec<GroupTrainingStat>> {
    let rows = sqlx::query!(
        r#"
        SELECT u.id AS user_id,
               u.username AS username,
               u.first_name AS first_name,
               COUNT(te.id) AS "sessions!: i64"
        FROM group_members gm
        JOIN users u ON u.telegram_user_id = gm.telegram_user_id
        JOIN training_entries te ON te.user_id = u.id
        WHERE gm.chat_id = ?
          AND te.recorded_at >= ?
          AND te.recorded_at < ?
        GROUP BY u.id
        ORDER BY COUNT(te.id) DESC
        "#,
        chat_id,
        start,
        end,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| GroupTrainingStat {
            username: r.username,
            first_name: r.first_name,
            sessions: r.sessions,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use chrono::{TimeZone, Utc};

    fn dummy_entry(
        description: &str,
        slug: &str,
        endurance: Option<EnduranceStats>,
        strength_sets: Vec<StrengthSet>,
    ) -> TrainingEntry {
        TrainingEntry {
            id: 1,
            user_id: 1,
            description: description.into(),
            recorded_at: Utc::now(),
            activity_type_id: 1,
            activity_slug: slug.into(),
            endurance,
            strength_sets,
        }
    }

    fn set(
        exercise: StrengthExercise,
        set_index: i64,
        reps: Option<i64>,
        weight_kg: Option<f64>,
    ) -> StrengthSet {
        StrengthSet {
            id: set_index,
            exercise,
            set_index,
            reps,
            weight_kg,
        }
    }

    #[test]
    fn summary_line_keeps_description_then_stats() {
        let run = dummy_entry(
            "Sortie tempo",
            "run",
            Some(EnduranceStats {
                duration_sec: Some(2520),
                distance_m: Some(8200.0),
                elevation_m: Some(85.0),
                avg_hr: None,
                calories: None,
            }),
            vec![],
        );
        assert_eq!(summary_line(&run), "Sortie tempo · 8.2 km · 42:00 · D+85");

        let gym = dummy_entry(
            "Full body",
            "gym",
            None,
            vec![
                set(StrengthExercise::Squat, 1, Some(8), Some(80.0)),
                set(StrengthExercise::Squat, 2, Some(8), Some(80.0)),
                set(StrengthExercise::Squat, 3, Some(8), Some(80.0)),
            ],
        );
        assert_eq!(summary_line(&gym), "Full body · squat 3×8 @ 80kg");

        let hybrid = dummy_entry(
            "Cindy",
            "crossfit",
            Some(EnduranceStats {
                duration_sec: Some(1200),
                ..Default::default()
            }),
            vec![
                set(StrengthExercise::PullUp, 1, Some(5), None),
                set(StrengthExercise::Squat, 1, Some(10), None),
                set(StrengthExercise::PushUp, 1, Some(15), None),
            ],
        );
        assert_eq!(
            summary_line(&hybrid),
            "Cindy · 20:00 · pull-up, squat, push-up"
        );

        let generic = dummy_entry("Sport", "other", None, vec![]);
        assert_eq!(summary_line(&generic), "Sport");
    }

    #[test]
    fn format_card_gym_groups_identical_and_mixed_sets() {
        let mut gym = dummy_entry(
            "Full body",
            "gym",
            None,
            vec![
                set(StrengthExercise::Squat, 1, Some(8), Some(80.0)),
                set(StrengthExercise::Squat, 2, Some(8), Some(80.0)),
                set(StrengthExercise::Squat, 3, Some(8), Some(80.0)),
                set(StrengthExercise::BenchPress, 1, Some(5), Some(80.0)),
                set(StrengthExercise::BenchPress, 2, Some(5), Some(82.5)),
                set(StrengthExercise::BenchPress, 3, Some(5), Some(85.0)),
                set(
                    StrengthExercise::DumbbellBicepsCurl,
                    1,
                    Some(10),
                    Some(12.0),
                ),
                set(
                    StrengthExercise::DumbbellBicepsCurl,
                    2,
                    Some(10),
                    Some(12.0),
                ),
                set(
                    StrengthExercise::DumbbellBicepsCurl,
                    3,
                    Some(10),
                    Some(12.0),
                ),
            ],
        );
        gym.id = 13;
        gym.recorded_at = Utc.with_ymd_and_hms(2026, 8, 14, 10, 0, 0).unwrap();

        assert_eq!(
            format_card(&gym),
            "Full body\n\
             #13 · 2026-08-14 · gym\n\
             \n\
             squat                 3×8 @ 80kg\n\
             bench press           5 @ 80kg, 5 @ 82.5kg, 5 @ 85kg\n\
             dumbbell biceps curl  3×10 @ 12kg"
        );
    }

    #[test]
    fn format_card_endurance_includes_pace_and_omits_missing() {
        let mut run = dummy_entry(
            "Sortie tempo",
            "run",
            Some(EnduranceStats {
                duration_sec: Some(2520),
                distance_m: Some(8200.0),
                elevation_m: Some(85.0),
                avg_hr: None,
                calories: None,
            }),
            vec![],
        );
        run.recorded_at = Utc.with_ymd_and_hms(2026, 8, 14, 10, 0, 0).unwrap();

        assert_eq!(
            format_card(&run),
            "Sortie tempo\n\
             #1 · 2026-08-14 · run\n\
             \n\
             8.2 km · 42:00 · 5:07/km · D+85"
        );
    }

    #[test]
    fn format_card_without_satellites_is_description_and_meta() {
        let mut generic = dummy_entry("Sport", "other", None, vec![]);
        generic.recorded_at = Utc.with_ymd_and_hms(2026, 8, 14, 10, 0, 0).unwrap();
        assert_eq!(format_card(&generic), "Sport\n#1 · 2026-08-14 · other");
    }

    async fn insert_other_row(
        pool: &SqlitePool,
        user_id: i64,
        description: &str,
        day: &str,
    ) -> i64 {
        sqlx::query(
            r#"INSERT INTO training_entries (user_id, description, recorded_at, activity_type_id)
               SELECT ?, ?, ?, id FROM activity_types WHERE slug = 'other'"#,
        )
        .bind(user_id)
        .bind(description)
        .bind(day)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    fn simple(
        user_id: i64,
        description: &str,
        recorded_at: Option<DateTime<Utc>>,
    ) -> NewTrainingEntry {
        NewTrainingEntry::simple(user_id, description.into(), recorded_at)
    }

    #[tokio::test]
    async fn insert_and_list_training() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99301,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let entry = insert(&pool, simple(user.id, "Morning run 5km", None))
            .await
            .unwrap();
        assert_eq!(entry.description, "Morning run 5km");
        assert_eq!(entry.activity_slug, SLUG_OTHER);

        let list = list_for_user(&pool, user.id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].description, "Morning run 5km");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn latest_training() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99302,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let older = Utc::now() - chrono::Duration::days(2);
        let newer = Utc::now() - chrono::Duration::hours(1);
        insert(&pool, simple(user.id, "old", Some(older)))
            .await
            .unwrap();
        insert(&pool, simple(user.id, "new", Some(newer)))
            .await
            .unwrap();

        let latest = latest_for_user(&pool, user.id).await.unwrap().unwrap();
        assert_eq!(latest.description, "new");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn sportive_can_update_and_delete_own_training() {
        let (pool, path) = test_db().await;
        let owner = insert_user(
            &pool,
            99303,
            Role::Sportive,
            Some("owner".into()),
            None,
            None,
        )
        .await
        .unwrap();

        let entry = insert(&pool, simple(owner.id, "legs", None)).await.unwrap();

        let updated = update(
            &pool,
            entry.id,
            owner.id,
            false,
            UpdateTraining {
                description: Some("legs + core".into()),
                recorded_at: None,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.description, "legs + core");
        assert_eq!(
            find_by_id(&pool, entry.id)
                .await
                .unwrap()
                .unwrap()
                .description,
            "legs + core"
        );

        assert!(delete(&pool, entry.id, owner.id, false).await.unwrap());
        assert!(find_by_id(&pool, entry.id).await.unwrap().is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn sportive_cannot_update_or_delete_others_training() {
        let (pool, path) = test_db().await;
        let owner = insert_user(
            &pool,
            99304,
            Role::Sportive,
            Some("owner".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let other = insert_user(
            &pool,
            99305,
            Role::Sportive,
            Some("other".into()),
            None,
            None,
        )
        .await
        .unwrap();

        let entry = insert(&pool, simple(owner.id, "legs", None)).await.unwrap();

        assert!(
            update(
                &pool,
                entry.id,
                other.id,
                false,
                UpdateTraining {
                    description: Some("hacked".into()),
                    recorded_at: None,
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(!delete(&pool, entry.id, other.id, false).await.unwrap());

        let still_there = find_by_id(&pool, entry.id).await.unwrap().unwrap();
        assert_eq!(still_there.description, "legs");
        assert_eq!(still_there.user_id, owner.id);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn admin_can_update_and_delete_others_training() {
        let (pool, path) = test_db().await;
        let owner = insert_user(
            &pool,
            99306,
            Role::Sportive,
            Some("owner".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let admin = insert_user(&pool, 99307, Role::Admin, Some("admin".into()), None, None)
            .await
            .unwrap();

        let entry = insert(&pool, simple(owner.id, "legs", None)).await.unwrap();

        let updated = update(
            &pool,
            entry.id,
            admin.id,
            true,
            UpdateTraining {
                description: Some("admin fix".into()),
                recorded_at: None,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.description, "admin fix");
        assert_eq!(updated.user_id, owner.id);

        assert!(delete(&pool, entry.id, admin.id, true).await.unwrap());
        assert!(find_by_id(&pool, entry.id).await.unwrap().is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn run_stores_endurance_not_sets() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99310, Role::Sportive, Some("r".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Tempo".into(),
                recorded_at: None,
                activity_slug: Some("run".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(2520),
                    distance_m: Some(8200.0),
                    elevation_m: Some(85.0),
                    avg_hr: None,
                    calories: None,
                }),
                strength_sets: vec![NewStrengthSet::new(
                    StrengthExercise::Squat,
                    1,
                    Some(8),
                    Some(80.0),
                )],
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.activity_slug, "run");
        assert!(entry.endurance.is_some());
        assert!(entry.strength_sets.is_empty());
        let line = summary_line(&entry);
        assert!(line.starts_with("Tempo"));
        assert!(line.contains("8.2 km"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn gym_stores_sets_not_endurance() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99313, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Full body".into(),
                recorded_at: None,
                activity_slug: Some("gym".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(3600),
                    distance_m: Some(5000.0),
                    ..Default::default()
                }),
                strength_sets: vec![
                    NewStrengthSet::new(StrengthExercise::Squat, 1, Some(8), Some(80.0)),
                    NewStrengthSet::new(StrengthExercise::Squat, 2, Some(8), Some(80.0)),
                    NewStrengthSet::new(StrengthExercise::Squat, 3, Some(8), Some(80.0)),
                ],
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.activity_slug, "gym");
        assert!(entry.endurance.is_none());
        assert_eq!(entry.strength_sets.len(), 3);
        assert_eq!(summary_line(&entry), "Full body · squat 3×8 @ 80kg");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn crossfit_can_store_both_profiles() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99311, Role::Sportive, Some("c".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Cindy".into(),
                recorded_at: None,
                activity_slug: Some("crossfit".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(1200),
                    ..Default::default()
                }),
                strength_sets: vec![
                    NewStrengthSet::new(StrengthExercise::PullUp, 1, Some(5), None),
                    NewStrengthSet::new(StrengthExercise::Squat, 2, Some(10), None),
                ],
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.activity_slug, "crossfit");
        assert_eq!(entry.endurance.as_ref().unwrap().duration_sec, Some(1200));
        assert_eq!(entry.strength_sets.len(), 2);
        let line = summary_line(&entry);
        assert!(line.contains("Cindy"));
        assert!(line.contains("20:00"));
        assert!(line.contains("pull-up"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn gym_maps_named_exercises_to_catalog() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99312, Role::Sportive, Some("g".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Push".into(),
                recorded_at: None,
                activity_slug: Some("gym".into()),
                endurance: None,
                strength_sets: vec![
                    NewStrengthSet::new(StrengthExercise::BenchPress, 1, Some(5), Some(80.0)),
                    NewStrengthSet::new(
                        StrengthExercise::DumbbellBicepsCurl,
                        1,
                        Some(10),
                        Some(12.0),
                    ),
                    NewStrengthSet::new(StrengthExercise::Other, 1, Some(40), Some(24.0)),
                ],
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.activity_slug, "gym");
        assert_eq!(entry.strength_sets.len(), 3);
        assert_eq!(
            entry.strength_sets[0].exercise,
            StrengthExercise::BenchPress
        );
        assert_eq!(
            entry.strength_sets[1].exercise,
            StrengthExercise::DumbbellBicepsCurl
        );
        assert_eq!(entry.strength_sets[2].exercise, StrengthExercise::Other);
        let line = summary_line(&entry);
        assert!(line.contains("bench press"));
        assert!(line.contains("dumbbell biceps curl"));
        assert!(line.contains("other"));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn delete_clears_endurance_and_strength_satellites() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99313, Role::Sportive, Some("d".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Cindy".into(),
                recorded_at: None,
                activity_slug: Some("crossfit".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(1200),
                    distance_m: Some(1000.0),
                    ..Default::default()
                }),
                strength_sets: vec![
                    NewStrengthSet::new(StrengthExercise::PullUp, 1, Some(5), None),
                    NewStrengthSet::new(StrengthExercise::Squat, 2, Some(10), None),
                    NewStrengthSet::new(StrengthExercise::PushUp, 3, Some(15), None),
                ],
            },
        )
        .await
        .unwrap();
        let id = entry.id;

        assert!(delete(&pool, id, user.id, false).await.unwrap());
        assert!(find_by_id(&pool, id).await.unwrap().is_none());

        let endurance_left: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_endurance WHERE training_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let sets_left: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM training_strength_sets WHERE training_id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(endurance_left, 0);
        assert_eq!(sets_left, 0);

        let _ = std::fs::remove_file(&path);
    }

    async fn insert_crossfit(pool: &SqlitePool, user_id: i64) -> TrainingEntry {
        insert(
            pool,
            NewTrainingEntry {
                user_id,
                description: "Cindy".into(),
                recorded_at: None,
                activity_slug: Some("crossfit".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(1200),
                    distance_m: Some(1000.0),
                    elevation_m: Some(10.0),
                    avg_hr: Some(150),
                    calories: Some(200),
                }),
                strength_sets: vec![
                    NewStrengthSet::new(StrengthExercise::PullUp, 1, Some(5), None),
                    NewStrengthSet::new(StrengthExercise::Squat, 2, Some(10), None),
                ],
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn description_only_update_keeps_satellites() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99320, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert_crossfit(&pool, user.id).await;

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                description: Some("Cindy AMRAP".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.description, "Cindy AMRAP");
        assert_eq!(updated.activity_slug, "crossfit");
        assert_eq!(updated.recorded_at, entry.recorded_at);
        let e = updated.endurance.as_ref().unwrap();
        assert_eq!(e.duration_sec, Some(1200));
        assert_eq!(e.distance_m, Some(1000.0));
        assert_eq!(e.avg_hr, Some(150));
        assert_eq!(updated.strength_sets.len(), 2);
        assert_eq!(updated.strength_sets[0].exercise, StrengthExercise::PullUp);
        assert_eq!(updated.strength_sets[1].exercise, StrengthExercise::Squat);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_endurance_keeps_existing_sets() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99321, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert_crossfit(&pool, user.id).await;

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                endurance: Some(EnduranceStats {
                    duration_sec: Some(900),
                    distance_m: Some(1500.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.description, "Cindy");
        let e = updated.endurance.as_ref().unwrap();
        assert_eq!(e.duration_sec, Some(900));
        assert_eq!(e.distance_m, Some(1500.0));
        assert_eq!(updated.strength_sets.len(), 2);
        assert_eq!(updated.strength_sets[0].exercise, StrengthExercise::PullUp);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_endurance_replaces_whole_object_not_field_merge() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99322, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert_crossfit(&pool, user.id).await;

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                endurance: Some(EnduranceStats {
                    distance_m: Some(2000.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        let e = updated.endurance.as_ref().unwrap();
        assert_eq!(e.distance_m, Some(2000.0));
        assert_eq!(e.duration_sec, None);
        assert_eq!(e.elevation_m, None);
        assert_eq!(e.avg_hr, None);
        assert_eq!(e.calories, None);
        assert_eq!(updated.strength_sets.len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_strength_sets_replaces_all_and_keeps_endurance() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99323, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert_crossfit(&pool, user.id).await;

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                strength_sets: Some(vec![
                    NewStrengthSet::new(StrengthExercise::BenchPress, 1, Some(5), Some(80.0)),
                    NewStrengthSet::new(StrengthExercise::BenchPress, 2, Some(5), Some(82.5)),
                ]),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.endurance.as_ref().unwrap().duration_sec, Some(1200));
        assert_eq!(updated.strength_sets.len(), 2);
        assert_eq!(
            updated.strength_sets[0].exercise,
            StrengthExercise::BenchPress
        );
        assert_eq!(updated.strength_sets[0].weight_kg, Some(80.0));
        assert_eq!(updated.strength_sets[1].weight_kg, Some(82.5));
        assert!(
            !updated
                .strength_sets
                .iter()
                .any(|s| s.exercise == StrengthExercise::PullUp)
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn changing_activity_gym_to_run_drops_sets() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99324, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Full body".into(),
                recorded_at: None,
                activity_slug: Some("gym".into()),
                endurance: None,
                strength_sets: vec![NewStrengthSet::new(
                    StrengthExercise::Squat,
                    1,
                    Some(8),
                    Some(80.0),
                )],
            },
        )
        .await
        .unwrap();
        assert_eq!(entry.strength_sets.len(), 1);

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                activity_slug: Some("run".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(1800),
                    distance_m: Some(5000.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.activity_slug, "run");
        assert_eq!(updated.description, "Full body");
        assert!(updated.strength_sets.is_empty());
        let e = updated.endurance.as_ref().unwrap();
        assert_eq!(e.duration_sec, Some(1800));
        assert_eq!(e.distance_m, Some(5000.0));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn changing_activity_run_to_gym_drops_endurance() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99325, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Tempo".into(),
                recorded_at: None,
                activity_slug: Some("run".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(2520),
                    distance_m: Some(8200.0),
                    ..Default::default()
                }),
                strength_sets: vec![],
            },
        )
        .await
        .unwrap();

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                activity_slug: Some("gym".into()),
                strength_sets: Some(vec![NewStrengthSet::new(
                    StrengthExercise::Squat,
                    1,
                    Some(8),
                    Some(80.0),
                )]),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.activity_slug, "gym");
        assert!(updated.endurance.is_none());
        assert_eq!(updated.strength_sets.len(), 1);
        assert_eq!(updated.strength_sets[0].exercise, StrengthExercise::Squat);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_empty_sets_clears_strength() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99326, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let entry = insert_crossfit(&pool, user.id).await;

        let updated = update(
            &pool,
            entry.id,
            user.id,
            false,
            UpdateTraining {
                strength_sets: Some(vec![]),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert!(updated.strength_sets.is_empty());
        assert_eq!(updated.endurance.as_ref().unwrap().duration_sec, Some(1200));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn update_unknown_id_returns_none() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99327, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        assert!(
            update(
                &pool,
                999_999,
                user.id,
                false,
                UpdateTraining {
                    description: Some("nope".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn try_insert_rejects_burst_duplicate() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99340, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let first = try_insert(&pool, simple(user.id, "Muscu", None))
            .await
            .unwrap();
        assert!(!first.is_duplicate());
        let second = try_insert(&pool, simple(user.id, "Sortie vélo", None))
            .await
            .unwrap();
        assert!(second.is_duplicate());
        assert_eq!(second.clone().entry().id, first.entry().id);
        assert_eq!(list_for_user(&pool, user.id).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn try_insert_rejects_same_description_within_two_hours() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99341, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 8, 1, 10, 0, 0).unwrap();
        let t1 = t0 + Duration::minutes(30);
        assert!(
            !try_insert(&pool, simple(user.id, "Course 5km", Some(t0)))
                .await
                .unwrap()
                .is_duplicate()
        );
        let again = try_insert(&pool, simple(user.id, "course  5km", Some(t1)))
            .await
            .unwrap();
        assert!(again.is_duplicate());
        assert_eq!(list_for_user(&pool, user.id).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn try_insert_allows_different_workout_after_burst() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99342, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 8, 1, 10, 0, 0).unwrap();
        let t1 = t0 + Duration::minutes(10);
        assert!(
            !try_insert(&pool, simple(user.id, "Muscu", Some(t0)))
                .await
                .unwrap()
                .is_duplicate()
        );
        let second = try_insert(&pool, simple(user.id, "Course", Some(t1)))
            .await
            .unwrap();
        assert!(!second.is_duplicate());
        assert_eq!(list_for_user(&pool, user.id).await.unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn try_insert_allows_same_description_next_day() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99343, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let day1 = Utc.with_ymd_and_hms(2026, 8, 1, 10, 0, 0).unwrap();
        let day2 = Utc.with_ymd_and_hms(2026, 8, 2, 10, 0, 0).unwrap();
        assert!(
            !try_insert(&pool, simple(user.id, "Muscu", Some(day1)))
                .await
                .unwrap()
                .is_duplicate()
        );
        assert!(
            !try_insert(&pool, simple(user.id, "Muscu", Some(day2)))
                .await
                .unwrap()
                .is_duplicate()
        );
        assert_eq!(list_for_user(&pool, user.id).await.unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn try_insert_merges_stats_into_duplicate() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99344, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let first = try_insert(&pool, simple(user.id, "Tempo", None))
            .await
            .unwrap()
            .entry();
        let second = try_insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Tempo".into(),
                recorded_at: None,
                activity_slug: Some("run".into()),
                endurance: Some(EnduranceStats {
                    duration_sec: Some(2520),
                    distance_m: Some(8200.0),
                    ..Default::default()
                }),
                strength_sets: vec![],
            },
        )
        .await
        .unwrap();
        assert!(second.is_duplicate());
        let entry = second.entry();
        assert_eq!(entry.id, first.id);
        assert_eq!(entry.activity_slug, "run");
        assert_eq!(entry.endurance.as_ref().unwrap().distance_m, Some(8200.0));
        assert_eq!(list_for_user(&pool, user.id).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn one_per_utc_day_sql_keeps_richest_row() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99345, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let day = "2026-07-01T08:00:00+00:00";
        let other_id = 8i64;
        let plain = sqlx::query(
            "INSERT INTO training_entries (user_id, description, recorded_at, activity_type_id) VALUES (?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind("x")
        .bind(day)
        .bind(other_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let rich = sqlx::query(
            "INSERT INTO training_entries (user_id, description, recorded_at, activity_type_id) VALUES (?, ?, ?, ?)",
        )
        .bind(user.id)
        .bind("Sortie tempo 8km")
        .bind(day)
        .bind(other_id)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO training_endurance (training_id, distance_m, duration_sec) VALUES (?, ?, ?)",
        )
        .bind(rich)
        .bind(8200.0)
        .bind(2520i64)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("CREATE TABLE _training_keep AS SELECT id FROM (SELECT te.id, ROW_NUMBER() OVER (PARTITION BY te.user_id, date(te.recorded_at) ORDER BY CASE WHEN e.training_id IS NOT NULL THEN 1 ELSE 0 END DESC, length(te.description) DESC, te.id DESC) AS rn FROM training_entries te LEFT JOIN training_endurance e ON e.training_id = te.id) WHERE rn = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM training_endurance WHERE training_id NOT IN (SELECT id FROM _training_keep)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM training_entries WHERE id NOT IN (SELECT id FROM _training_keep)")
            .execute(&pool)
            .await
            .unwrap();

        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM training_entries WHERE user_id = ?")
                .bind(user.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(left, 1);
        let kept: i64 = sqlx::query_scalar("SELECT id FROM training_entries WHERE user_id = ?")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kept, rich);
        assert_ne!(kept, plain);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn infer_activity_from_obvious_descriptions() {
        assert_eq!(infer_activity_slug("Course à pied 10 km"), Some("run"));
        assert_eq!(infer_activity_slug("Sortie footing"), Some("run"));
        assert_eq!(infer_activity_slug("Je vais courir"), Some("run"));
        assert_eq!(infer_activity_slug("Jogging"), Some("run"));
        assert_eq!(infer_activity_slug("Trail dimanche"), Some("run"));
        assert_eq!(infer_activity_slug("Vélo 40km"), Some("ride"));
        assert_eq!(infer_activity_slug("Randonnée en montagne"), Some("hike"));
        assert_eq!(infer_activity_slug("Evening walk"), Some("walk"));
    }

    #[test]
    fn infer_activity_stays_quiet_when_unclear() {
        assert_eq!(infer_activity_slug("5 km"), None);
        assert_eq!(infer_activity_slug("Muscu jambes"), None);
        assert_eq!(infer_activity_slug("parcours"), None);
        assert_eq!(infer_activity_slug("ça marche"), None);
        assert_eq!(infer_activity_slug("Course puis vélo"), None);
    }

    #[tokio::test]
    async fn try_insert_infers_run_when_activity_missing_or_other() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99350, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 8, 0, 0).unwrap();
        let footing = try_insert(&pool, simple(user.id, "Footing du matin", Some(t0)))
            .await
            .unwrap()
            .entry();
        assert_eq!(footing.activity_slug, "run");

        let t1 = Utc.with_ymd_and_hms(2026, 9, 2, 8, 0, 0).unwrap();
        let other = try_insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Trail 15".into(),
                recorded_at: Some(t1),
                activity_slug: Some("other".into()),
                endurance: None,
                strength_sets: vec![],
            },
        )
        .await
        .unwrap()
        .entry();
        assert_eq!(other.activity_slug, "run");

        let t2 = Utc.with_ymd_and_hms(2026, 9, 3, 8, 0, 0).unwrap();
        let gym = try_insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Footing? non, muscu".into(),
                recorded_at: Some(t2),
                activity_slug: Some("gym".into()),
                endurance: None,
                strength_sets: vec![],
            },
        )
        .await
        .unwrap()
        .entry();
        assert_eq!(gym.activity_slug, "gym");

        let t3 = Utc.with_ymd_and_hms(2026, 9, 4, 8, 0, 0).unwrap();
        let plain = try_insert(&pool, simple(user.id, "Muscu jambes", Some(t3)))
            .await
            .unwrap()
            .entry();
        assert_eq!(plain.activity_slug, "other");

        let t4 = Utc.with_ymd_and_hms(2026, 9, 5, 8, 0, 0).unwrap();
        let km = try_insert(&pool, simple(user.id, "5 km", Some(t4)))
            .await
            .unwrap()
            .entry();
        assert_eq!(km.activity_slug, "other");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn burst_duplicate_does_not_apply_inferred_activity() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99352, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 9, 1, 10, 0, 0).unwrap();
        let first = try_insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Muscu".into(),
                recorded_at: Some(t0),
                activity_slug: Some("gym".into()),
                endurance: None,
                strength_sets: vec![NewStrengthSet::new(
                    StrengthExercise::Squat,
                    1,
                    Some(5),
                    Some(80.0),
                )],
            },
        )
        .await
        .unwrap()
        .entry();
        assert_eq!(first.activity_slug, "gym");
        assert_eq!(first.strength_sets.len(), 1);

        let second = try_insert(
            &pool,
            simple(user.id, "Footing", Some(t0 + Duration::seconds(10))),
        )
        .await
        .unwrap();
        assert!(second.is_duplicate());
        let kept = second.entry();
        assert_eq!(kept.id, first.id);
        assert_eq!(kept.activity_slug, "gym");
        assert_eq!(kept.strength_sets.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn sets_on_other_promote_to_gym() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99353, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let created = try_insert(&pool, simple(user.id, "Séance dos", None))
            .await
            .unwrap()
            .entry();
        assert_eq!(created.activity_slug, "other");

        let updated = update(
            &pool,
            created.id,
            user.id,
            false,
            UpdateTraining {
                strength_sets: Some(vec![NewStrengthSet::new(
                    StrengthExercise::Squat,
                    1,
                    Some(8),
                    Some(60.0),
                )]),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.activity_slug, "gym");
        assert_eq!(updated.strength_sets.len(), 1);
        assert_eq!(updated.strength_sets[0].reps, Some(8));

        let logged = try_insert(
            &pool,
            NewTrainingEntry {
                user_id: user.id,
                description: "Pecs".into(),
                recorded_at: Some(Utc.with_ymd_and_hms(2026, 9, 2, 18, 0, 0).unwrap()),
                activity_slug: Some("other".into()),
                endurance: None,
                strength_sets: vec![NewStrengthSet::new(
                    StrengthExercise::BenchPress,
                    1,
                    Some(5),
                    Some(70.0),
                )],
            },
        )
        .await
        .unwrap()
        .entry();
        assert_eq!(logged.activity_slug, "gym");
        assert_eq!(logged.strength_sets.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn reclassify_other_rows_uses_the_same_hints() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 99351, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        let day = "2026-09-10T08:00:00+00:00";
        let ride = insert_other_row(&pool, user.id, "Sortie vélo", day).await;
        let gym = insert_other_row(&pool, user.id, "Muscu", day).await;
        let mixed = insert_other_row(&pool, user.id, "Course puis vélo", day).await;

        let updated = reclassify_other_activities(&pool).await.unwrap();
        assert_eq!(updated, 1);
        assert_eq!(
            find_by_id(&pool, ride)
                .await
                .unwrap()
                .unwrap()
                .activity_slug,
            "ride"
        );
        assert_eq!(
            find_by_id(&pool, gym).await.unwrap().unwrap().activity_slug,
            "other"
        );
        assert_eq!(
            find_by_id(&pool, mixed)
                .await
                .unwrap()
                .unwrap()
                .activity_slug,
            "other"
        );

        let _ = std::fs::remove_file(&path);
    }
}
