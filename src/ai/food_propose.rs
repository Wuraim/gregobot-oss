use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use crate::ai::food_selection::{MIN_SCORE, RankedFood};
use crate::db::foods::{Food, Macros, QuantityUnit};
use crate::db::meals::MealKind;
use crate::db::training::TrainingEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingBias {
    Strength,
    Endurance,
    Mixed,
    Rest,
}

impl TrainingBias {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Endurance => "endurance",
            Self::Mixed => "mixed",
            Self::Rest => "rest",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProposedItem {
    pub food_id: Option<i64>,
    pub description: String,
    pub quantity: f64,
    pub unit: QuantityUnit,
    pub macros: Option<Macros>,
    pub estimated: bool,
}

#[derive(Debug, Clone)]
pub struct MealOption {
    pub title: String,
    pub items: Vec<ProposedItem>,
    pub macros: Option<Macros>,
    pub day_if_eaten: Option<Macros>,
    pub why: String,
    pub estimated: bool,
}

pub fn classify_training(entries: &[TrainingEntry]) -> TrainingBias {
    let mut strength = false;
    let mut endurance = false;
    for entry in entries {
        match entry.activity_slug.as_str() {
            "gym" => strength = true,
            "run" | "ride" | "walk" | "hike" | "swim" => endurance = true,
            "crossfit" => {
                strength = true;
                endurance = true;
            }
            _ => {}
        }
        if !entry.strength_sets.is_empty() {
            strength = true;
        }
        if entry
            .endurance
            .as_ref()
            .is_some_and(|stats| !stats.is_empty())
        {
            endurance = true;
        }
    }
    match (strength, endurance) {
        (true, true) => TrainingBias::Mixed,
        (true, false) => TrainingBias::Strength,
        (false, true) => TrainingBias::Endurance,
        (false, false) => TrainingBias::Rest,
    }
}

pub fn training_today(
    entries: &[TrainingEntry],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<&TrainingEntry> {
    entries
        .iter()
        .filter(|e| e.recorded_at >= start && e.recorded_at < end)
        .collect()
}

pub fn today_bounds(timezone: &str, now: DateTime<Utc>) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let tz: Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let date = now.with_timezone(&tz).date_naive();
    crate::datetime::local_day_bounds(date, tz)
}

fn serving_macros(food: &Food, servings: f64) -> Option<(f64, QuantityUnit, Macros)> {
    let macros = food.scale(servings, QuantityUnit::Serving).ok()?;
    Some((servings, QuantityUnit::Serving, macros))
}

fn protein_per_100(food: &Food) -> f64 {
    food.protein_g_per_100
}

fn carbs_per_100(food: &Food) -> f64 {
    food.carbs_g_per_100
}

fn suggested_servings(food: &Food, bias: TrainingBias, day: &Macros) -> f64 {
    let high_protein = protein_per_100(food) >= 50.0;
    let small_scoop = food.serving_qty > 0.0 && food.serving_qty <= 40.0;
    if !(high_protein && small_scoop) {
        return 1.0;
    }
    // Already loaded on protein: keep a single scoop even on strength days.
    if day.protein_g >= 120.0 {
        return 1.0;
    }
    if matches!(bias, TrainingBias::Strength | TrainingBias::Mixed) {
        2.0
    } else {
        1.0
    }
}

fn item_from_ranked(ranked: &RankedFood, servings: f64) -> Option<ProposedItem> {
    let (quantity, unit, macros) = serving_macros(&ranked.food, servings)?;
    Some(ProposedItem {
        food_id: Some(ranked.food.id),
        description: ranked.food.embed_text(),
        quantity,
        unit,
        macros: Some(macros),
        estimated: false,
    })
}

fn sum_macros(items: &[ProposedItem]) -> Option<Macros> {
    let mut total = Macros::zero();
    let mut any = false;
    for item in items {
        let m = item.macros?;
        total.add(&m);
        any = true;
    }
    if any { Some(total.rounded()) } else { None }
}

fn day_if_eaten(day: &Macros, meal: Option<&Macros>) -> Option<Macros> {
    let meal = meal?;
    let mut total = *day;
    total.add(meal);
    Some(total.rounded())
}

fn companion(ranked: &[RankedFood], used_id: i64, bias: TrainingBias) -> Option<&RankedFood> {
    let mut candidates: Vec<&RankedFood> = ranked.iter().filter(|r| r.food.id != used_id).collect();
    match bias {
        TrainingBias::Strength => {
            candidates.sort_by(|a, b| {
                protein_per_100(&b.food)
                    .partial_cmp(&protein_per_100(&a.food))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        TrainingBias::Endurance => {
            candidates.sort_by(|a, b| {
                carbs_per_100(&b.food)
                    .partial_cmp(&carbs_per_100(&a.food))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        TrainingBias::Mixed => {
            let top = ranked.iter().find(|r| r.food.id == used_id)?;
            if protein_per_100(&top.food) >= carbs_per_100(&top.food) {
                candidates.sort_by(|a, b| {
                    carbs_per_100(&b.food)
                        .partial_cmp(&carbs_per_100(&a.food))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                candidates.sort_by(|a, b| {
                    protein_per_100(&b.food)
                        .partial_cmp(&protein_per_100(&a.food))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
        TrainingBias::Rest => return None,
    }
    candidates.into_iter().next()
}

fn why_line(
    envie: &str,
    ranked: &RankedFood,
    bias: TrainingBias,
    day: &Macros,
    meal: Option<&Macros>,
    extra: Option<&str>,
    minutes: Option<u32>,
) -> String {
    let mut parts = vec![format!("envie: {envie}")];
    parts.push(format!("catalog: {}", ranked.food.embed_text()));
    match bias {
        TrainingBias::Strength => parts.push("training: strength today, protein bias".into()),
        TrainingBias::Endurance => parts.push("training: endurance today, carb bias".into()),
        TrainingBias::Mixed => parts.push("training: mixed session today".into()),
        TrainingBias::Rest => parts.push("training: rest day, envie first".into()),
    }
    parts.push(format!(
        "diary so far: {:.0} kcal / P {:.0}g / C {:.0}g / F {:.0}g",
        day.kcal, day.protein_g, day.carbs_g, day.fat_g
    ));
    if let Some(m) = meal {
        parts.push(format!(
            "meal: {:.0} kcal / P {:.0}g / C {:.0}g / F {:.0}g",
            m.kcal, m.protein_g, m.carbs_g, m.fat_g
        ));
    }
    if let Some(extra) = extra {
        parts.push(extra.to_string());
    }
    if let Some(m) = minutes {
        parts.push(format!("time: {m} min"));
    }
    parts.join("; ")
}

fn kind_label(kind: Option<MealKind>) -> &'static str {
    match kind {
        Some(MealKind::Breakfast) => "Breakfast",
        Some(MealKind::Lunch) => "Lunch",
        Some(MealKind::Dinner) => "Dinner",
        Some(MealKind::Snack) => "Snack",
        Some(MealKind::Other) | None => "Option",
    }
}

fn estimated_option(envie: &str, kind: Option<MealKind>) -> MealOption {
    MealOption {
        title: format!("~ {} ({envie})", kind_label(kind)),
        items: vec![ProposedItem {
            food_id: None,
            description: envie.to_string(),
            quantity: 1.0,
            unit: QuantityUnit::Serving,
            macros: None,
            estimated: true,
        }],
        macros: None,
        day_if_eaten: None,
        why: format!(
            "envie: {envie}; catalog has no close match. Estimated — do not invent grams; log with estimated macros if they eat this."
        ),
        estimated: true,
    }
}

fn push_option(out: &mut Vec<MealOption>, option: MealOption) {
    if out.len() >= 3 {
        return;
    }
    let signature: Vec<i64> = option.items.iter().filter_map(|i| i.food_id).collect();
    if !signature.is_empty()
        && out.iter().any(|existing| {
            existing
                .items
                .iter()
                .filter_map(|i| i.food_id)
                .eq(signature.iter().copied())
        })
    {
        return;
    }
    out.push(option);
}

/// Build 2–3 meal options from ranked catalog foods. Pure: no Gemini.
pub fn build_options(
    envie: &str,
    ranked: &[RankedFood],
    day_totals: &Macros,
    bias: TrainingBias,
    minutes: Option<u32>,
    kind: Option<MealKind>,
) -> Vec<MealOption> {
    let mut options = Vec::new();
    let quick = minutes.is_some_and(|m| m < 15);
    let catalog_fit = ranked.first().is_some_and(|r| {
        r.envie_score >= MIN_SCORE || crate::ai::food_selection::food_text_matches(&r.food, envie)
    });

    if let Some(top) = ranked.first()
        && let Some(item) = item_from_ranked(top, suggested_servings(&top.food, bias, day_totals))
    {
        let items = vec![item];
        let macros = sum_macros(&items);
        push_option(
            &mut options,
            MealOption {
                title: format!("{}: {}", kind_label(kind), top.food.embed_text()),
                why: why_line(envie, top, bias, day_totals, macros.as_ref(), None, minutes),
                day_if_eaten: day_if_eaten(day_totals, macros.as_ref()),
                macros,
                items,
                estimated: false,
            },
        );
    }

    if !quick
        && let Some(top) = ranked.first()
        && let Some(pair) = companion(ranked, top.food.id, bias)
        && let Some(a) = item_from_ranked(top, suggested_servings(&top.food, bias, day_totals))
        && let Some(b) = item_from_ranked(pair, 1.0)
    {
        let items = vec![a, b];
        let macros = sum_macros(&items);
        push_option(
            &mut options,
            MealOption {
                title: format!(
                    "{}: {} + {}",
                    kind_label(kind),
                    top.food.embed_text(),
                    pair.food.embed_text()
                ),
                why: why_line(
                    envie,
                    top,
                    bias,
                    day_totals,
                    macros.as_ref(),
                    Some(&format!("plus {}", pair.food.embed_text())),
                    minutes,
                ),
                day_if_eaten: day_if_eaten(day_totals, macros.as_ref()),
                macros,
                items,
                estimated: false,
            },
        );
    }

    if let Some(second) = ranked.get(1)
        && let Some(item) =
            item_from_ranked(second, suggested_servings(&second.food, bias, day_totals))
    {
        let items = vec![item];
        let macros = sum_macros(&items);
        push_option(
            &mut options,
            MealOption {
                title: format!("{}: {}", kind_label(kind), second.food.embed_text()),
                why: why_line(
                    envie,
                    second,
                    bias,
                    day_totals,
                    macros.as_ref(),
                    None,
                    minutes,
                ),
                day_if_eaten: day_if_eaten(day_totals, macros.as_ref()),
                macros,
                items,
                estimated: false,
            },
        );
    }

    if !catalog_fit {
        if options.len() >= 3 {
            options.pop();
        }
        options.push(estimated_option(envie, kind));
    }

    if options.is_empty() {
        options.push(estimated_option(envie, kind));
    }

    options.truncate(3);
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::food_selection::RankedFood;
    use crate::db::foods::{Food, FoodSource, QuantityUnit};
    use chrono::Utc;

    fn dummy_food(id: i64, name: &str, protein: f64, carbs: f64, serving_qty: f64) -> Food {
        Food {
            id,
            user_id: 1,
            name: name.into(),
            brand: None,
            barcode: None,
            notes: None,
            kcal_per_100: protein * 4.0 + carbs * 4.0,
            protein_g_per_100: protein,
            carbs_g_per_100: carbs,
            fat_g_per_100: 1.0,
            serving_qty,
            serving_unit: QuantityUnit::G,
            serving_label: None,
            source: FoodSource::Manual,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn ranked(food: Food, envie_score: f32) -> RankedFood {
        let score = envie_score;
        RankedFood {
            food,
            envie_score,
            score,
        }
    }

    #[test]
    fn rest_day_envie_still_returns_options() {
        let rice = dummy_food(1, "rice", 7.0, 78.0, 150.0);
        let oats = dummy_food(2, "oats", 13.0, 60.0, 80.0);
        let options = build_options(
            "carbs",
            &[ranked(rice, 0.8), ranked(oats, 0.6)],
            &Macros::zero(),
            TrainingBias::Rest,
            None,
            Some(MealKind::Dinner),
        );
        assert!(!options.is_empty());
        assert!(options.iter().all(|o| !o.estimated));
        assert!(options.iter().any(|o| o.title.contains("rice")));
    }

    #[test]
    fn strength_bias_keeps_one_scoop_when_diary_protein_is_high() {
        let whey = dummy_food(1, "whey", 80.0, 6.0, 30.0);
        let options = build_options(
            "protein",
            &[ranked(whey, 0.9)],
            &Macros {
                kcal: 1800.0,
                protein_g: 130.0,
                carbs_g: 180.0,
                fat_g: 50.0,
            },
            TrainingBias::Strength,
            None,
            Some(MealKind::Snack),
        );
        let whey_opt = options
            .iter()
            .find(|o| o.items.len() == 1 && o.items[0].food_id == Some(1))
            .expect("single whey option");
        assert_eq!(whey_opt.items[0].quantity, 1.0);
        assert!(whey_opt.why.contains("diary so far"));
    }

    #[test]
    fn strength_bias_scales_two_scoops_of_whey() {
        let whey = dummy_food(1, "whey", 80.0, 6.0, 30.0);
        let rice = dummy_food(2, "rice", 7.0, 78.0, 150.0);
        let options = build_options(
            "protein",
            &[ranked(whey, 0.9), ranked(rice, 0.4)],
            &Macros {
                kcal: 400.0,
                protein_g: 20.0,
                carbs_g: 40.0,
                fat_g: 10.0,
            },
            TrainingBias::Strength,
            None,
            Some(MealKind::Snack),
        );
        let whey_opt = options
            .iter()
            .find(|o| o.items.len() == 1 && o.items[0].food_id == Some(1))
            .expect("single whey option");
        assert_eq!(whey_opt.items[0].quantity, 2.0);
        let macros = whey_opt.macros.expect("catalog macros");
        assert_eq!(macros.protein_g, 48.0);
        let day = whey_opt.day_if_eaten.expect("day totals");
        assert_eq!(day.protein_g, 68.0);
    }

    #[test]
    fn unmatched_envie_adds_estimated_option() {
        let oats = dummy_food(1, "oats", 13.0, 60.0, 80.0);
        let options = build_options(
            "pizza",
            &[ranked(oats, 0.2)],
            &Macros::zero(),
            TrainingBias::Rest,
            None,
            Some(MealKind::Dinner),
        );
        assert!(options.iter().any(|o| o.estimated));
        let est = options.iter().find(|o| o.estimated).unwrap();
        assert!(est.macros.is_none());
        assert!(est.why.contains("Estimated"));
    }

    #[test]
    fn empty_catalog_returns_estimated() {
        let options = build_options(
            "salty",
            &[],
            &Macros::zero(),
            TrainingBias::Rest,
            Some(10),
            None,
        );
        assert_eq!(options.len(), 1);
        assert!(options[0].estimated);
    }
}
