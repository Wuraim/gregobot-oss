use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use plotters::prelude::*;
use plotters::style::register_font;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use std::io::Cursor;
use std::sync::Once;

use crate::db::foods::Macros;
use crate::db::meals::{DayNutrition, ItemSource, Meal};
use crate::i18n::Msg;

static FONT_INIT: Once = Once::new();

const BG: RGBColor = RGBColor(15, 23, 42);
const GRID: RGBColor = RGBColor(51, 65, 85);
const AXIS: RGBColor = RGBColor(148, 163, 184);
const TITLE_COLOR: RGBColor = RGBColor(241, 245, 249);
const MUTED: RGBColor = RGBColor(148, 163, 184);
const PROTEIN: RGBColor = RGBColor(56, 189, 248);
const CARBS: RGBColor = RGBColor(251, 146, 60);
const FAT: RGBColor = RGBColor(244, 63, 94);
const TOTAL_BG: RGBColor = RGBColor(30, 41, 59);

fn ensure_font_registered() {
    FONT_INIT.call_once(|| {
        let bytes: &'static [u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
        if register_font("sans-serif", FontStyle::Normal, bytes).is_err() {
            tracing::error!("Failed to register chart font (DejaVuSans)");
        }
    });
}

#[derive(Debug, Clone)]
pub struct BuiltNutritionTable {
    pub png: Vec<u8>,
    pub caption: String,
    pub date: NaiveDate,
    pub totals: Macros,
    pub meal_count: usize,
}

#[derive(Debug)]
pub enum BuildNutritionTableError {
    Empty,
    Render(anyhow::Error),
}

pub fn parse_date_arg(arg: &str, timezone: &str, now: DateTime<Utc>) -> Result<NaiveDate, ()> {
    let arg = arg.trim().to_lowercase();
    let tz: Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let today = now.with_timezone(&tz).date_naive();
    if arg.is_empty() || arg == "today" || arg == "aujourd'hui" {
        return Ok(today);
    }
    if arg == "yesterday" || arg == "hier" {
        return Ok(today - chrono::Duration::days(1));
    }
    NaiveDate::parse_from_str(arg.trim(), "%Y-%m-%d").map_err(|_| ())
}

pub fn build_nutrition_table(
    day: &DayNutrition,
    timezone: &str,
    user_lang: &Msg,
) -> Result<BuiltNutritionTable, BuildNutritionTableError> {
    if day.meals.is_empty() {
        return Err(BuildNutritionTableError::Empty);
    }
    let tz: Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let png = render_day_card(day, &tz, user_lang).map_err(BuildNutritionTableError::Render)?;
    let caption = user_lang.nutrition_caption(
        &day.date.format("%Y-%m-%d").to_string(),
        day.totals.kcal,
        day.totals.protein_g,
        day.totals.carbs_g,
        day.totals.fat_g,
    );
    Ok(BuiltNutritionTable {
        png,
        caption,
        date: day.date,
        totals: day.totals,
        meal_count: day.meals.len(),
    })
}

pub fn render_day_card(day: &DayNutrition, tz: &Tz, user_lang: &Msg) -> Result<Vec<u8>> {
    ensure_font_registered();

    let item_count: usize = day.meals.iter().map(|m| m.items.len().max(1)).sum();
    let width = 920u32;
    let header_h = 86i32;
    let bar_h = 36i32;
    let meal_h = 28i32;
    let row_h = 22i32;
    let footer_h = 40i32;
    let height = (header_h
        + bar_h
        + 16
        + day.meals.len() as i32 * meal_h
        + item_count as i32 * row_h
        + footer_h
        + 28)
        .clamp(360, 1800) as u32;

    let mut buffer = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        root.fill(&BG)
            .context("failed to fill nutrition background")?;

        let title = format!(
            "{} — {}",
            user_lang.nutrition_title(),
            day.date.format("%Y-%m-%d")
        );
        draw_text(&root, &title, 24, 18, 26, TITLE_COLOR, HPos::Left)?;

        let totals = day.totals;
        draw_text(
            &root,
            &format!(
                "{:.0} kcal   P {:.0}g   {} {:.0}g   {} {:.0}g",
                totals.kcal,
                totals.protein_g,
                user_lang.nutrition_carbs_short(),
                totals.carbs_g,
                user_lang.nutrition_fat_short(),
                totals.fat_g
            ),
            24,
            50,
            16,
            AXIS,
            HPos::Left,
        )?;

        draw_macro_bar(&root, 24, 72, (width as i32) - 48, 18, &totals)?;

        let col_p = (width as i32) - 280;
        let col_c = (width as i32) - 210;
        let col_f = (width as i32) - 140;
        let col_k = (width as i32) - 60;

        draw_text(
            &root,
            "P",
            col_p,
            header_h + bar_h - 4,
            13,
            MUTED,
            HPos::Right,
        )?;
        draw_text(
            &root,
            user_lang.nutrition_carbs_short(),
            col_c,
            header_h + bar_h - 4,
            13,
            MUTED,
            HPos::Right,
        )?;
        draw_text(
            &root,
            user_lang.nutrition_fat_short(),
            col_f,
            header_h + bar_h - 4,
            13,
            MUTED,
            HPos::Right,
        )?;
        draw_text(
            &root,
            "kcal",
            col_k,
            header_h + bar_h - 4,
            13,
            MUTED,
            HPos::Right,
        )?;

        let mut y = header_h + bar_h + 16;
        for meal in &day.meals {
            let header = meal_header(meal, tz, user_lang);
            draw_text(&root, &header, 24, y, 16, TITLE_COLOR, HPos::Left)?;
            let mt = meal.totals();
            draw_macros(&root, y, col_p, col_c, col_f, col_k, &mt, 14, AXIS)?;
            y += meal_h;
            for item in &meal.items {
                let prefix = if item.source == ItemSource::Estimated {
                    "~ "
                } else {
                    ""
                };
                let qty = format_qty(item.quantity, item.unit.as_str());
                let line = format!("{prefix}{qty} {}", item.description);
                draw_text(&root, &truncate(&line, 52), 36, y, 14, MUTED, HPos::Left)?;
                draw_macros(
                    &root,
                    y,
                    col_p,
                    col_c,
                    col_f,
                    col_k,
                    &item.macros,
                    13,
                    MUTED,
                )?;
                y += row_h;
            }
        }

        let fy = (height as i32) - 36;
        root.draw(&Rectangle::new(
            [(16, fy - 8), (width as i32 - 16, fy + 22)],
            TOTAL_BG.filled(),
        ))
        .context("failed to draw totals bar")?;
        draw_text(
            &root,
            user_lang.nutrition_total(),
            28,
            fy,
            16,
            TITLE_COLOR,
            HPos::Left,
        )?;
        draw_macros(
            &root,
            fy,
            col_p,
            col_c,
            col_f,
            col_k,
            &totals,
            16,
            TITLE_COLOR,
        )?;

        root.present()
            .context("failed to present nutrition table")?;
    }

    encode_rgb_png(&buffer, width, height)
}

fn meal_header(meal: &Meal, tz: &Tz, user_lang: &Msg) -> String {
    let kind = user_lang.nutrition_kind(meal.kind);
    let time = meal.eaten_at.with_timezone(tz).format("%H:%M");
    match meal
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(label) => format!("{kind}  {time}  ·  {label}"),
        None => format!("{kind}  {time}"),
    }
}

fn format_qty(qty: f64, unit: &str) -> String {
    let n = if (qty - qty.round()).abs() < 0.05 {
        format!("{:.0}", qty)
    } else {
        format!("{qty:.1}")
    };
    match unit {
        "g" | "ml" => format!("{n}{unit}"),
        _ => format!("{n} {unit}"),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn draw_macros<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    y: i32,
    col_p: i32,
    col_c: i32,
    col_f: i32,
    col_k: i32,
    macros: &Macros,
    size: i32,
    color: RGBColor,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    draw_text(
        root,
        &format!("{:.0}", macros.protein_g),
        col_p,
        y,
        size,
        color,
        HPos::Right,
    )?;
    draw_text(
        root,
        &format!("{:.0}", macros.carbs_g),
        col_c,
        y,
        size,
        color,
        HPos::Right,
    )?;
    draw_text(
        root,
        &format!("{:.0}", macros.fat_g),
        col_f,
        y,
        size,
        color,
        HPos::Right,
    )?;
    draw_text(
        root,
        &format!("{:.0}", macros.kcal),
        col_k,
        y,
        size,
        color,
        HPos::Right,
    )?;
    Ok(())
}

fn draw_text<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    text: &str,
    x: i32,
    y: i32,
    size: i32,
    color: RGBColor,
    hpos: HPos,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let style = ("sans-serif", size)
        .into_font()
        .color(&color)
        .pos(Pos::new(hpos, VPos::Top));
    root.draw(&Text::new(text.to_string(), (x, y), style))
        .map_err(|e| anyhow!("failed to draw text: {e}"))?;
    Ok(())
}

fn draw_macro_bar<DB: DrawingBackend>(
    root: &DrawingArea<DB, plotters::coord::Shift>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    totals: &Macros,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let p_kcal = totals.protein_g * 4.0;
    let c_kcal = totals.carbs_g * 4.0;
    let f_kcal = totals.fat_g * 9.0;
    let sum = (p_kcal + c_kcal + f_kcal).max(1.0);
    root.draw(&Rectangle::new(
        [(x, y), (x + width, y + height)],
        GRID.filled(),
    ))
    .map_err(|e| anyhow!("failed to draw bar track: {e}"))?;
    let mut cursor = x;
    let segments = [(p_kcal, PROTEIN), (c_kcal, CARBS), (f_kcal, FAT)];
    for (kcal, color) in segments {
        let w = ((kcal / sum) * width as f64).round() as i32;
        if w > 0 {
            root.draw(&Rectangle::new(
                [(cursor, y), (cursor + w, y + height)],
                color.filled(),
            ))
            .map_err(|e| anyhow!("failed to draw bar segment: {e}"))?;
            cursor += w;
        }
    }
    Ok(())
}

fn encode_rgb_png(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut cursor, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .context("failed to write PNG header")?;
        writer
            .write_image_data(rgb)
            .context("failed to write PNG image data")?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::foods::{Macros, QuantityUnit};
    use crate::db::meals::{ItemSource, MealItem, MealKind};
    use crate::i18n::{Lang, Msg};
    use chrono::TimeZone;

    fn sample_day() -> DayNutrition {
        let eaten = chrono_tz::Europe::Paris
            .with_ymd_and_hms(2026, 8, 18, 8, 12, 0)
            .unwrap()
            .with_timezone(&Utc);
        let meal = Meal {
            id: 1,
            user_id: 1,
            kind: MealKind::Breakfast,
            label: Some("post-workout".into()),
            eaten_at: eaten,
            notes: None,
            items: vec![
                MealItem {
                    id: 1,
                    meal_id: 1,
                    food_id: Some(1),
                    description: "Whey Isolate".into(),
                    quantity: 2.0,
                    unit: QuantityUnit::Serving,
                    macros: Macros {
                        kcal: 222.0,
                        protein_g: 48.0,
                        carbs_g: 3.6,
                        fat_g: 2.4,
                    },
                    source: ItemSource::Catalog,
                },
                MealItem {
                    id: 2,
                    meal_id: 1,
                    food_id: None,
                    description: "pizza".into(),
                    quantity: 1.0,
                    unit: QuantityUnit::Piece,
                    macros: Macros {
                        kcal: 700.0,
                        protein_g: 28.0,
                        carbs_g: 80.0,
                        fat_g: 28.0,
                    },
                    source: ItemSource::Estimated,
                },
            ],
        };
        DayNutrition::from_meals(
            NaiveDate::from_ymd_opt(2026, 8, 18).unwrap(),
            "Europe/Paris",
            vec![meal],
        )
    }

    #[test]
    fn renders_png_for_day() {
        let day = sample_day();
        let lang = Msg::new(Lang::En);
        let png = render_day_card(&day, &chrono_tz::Europe::Paris, &lang).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 100);
        if let Ok(path) = std::env::var("WRITE_NUTRITION_PNG") {
            std::fs::write(path, &png).unwrap();
        }
    }

    #[test]
    fn parse_date_arg_today_and_iso() {
        let now = Utc.with_ymd_and_hms(2026, 8, 18, 20, 0, 0).unwrap();
        assert_eq!(
            parse_date_arg("", "Europe/Paris", now).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 18).unwrap()
        );
        assert_eq!(
            parse_date_arg("yesterday", "Europe/Paris", now).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 17).unwrap()
        );
        assert_eq!(
            parse_date_arg("2026-08-01", "UTC", now).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()
        );
        assert!(parse_date_arg("nope", "UTC", now).is_err());
    }
}
