use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDate, Utc};
use plotters::prelude::*;
use plotters::series::LineSeries;
use plotters::style::register_font;
use std::io::Cursor;
use std::sync::Once;

use crate::db::weight::WeightEntry;
use crate::i18n::Msg;

static FONT_INIT: Once = Once::new();

const BG: RGBColor = RGBColor(15, 23, 42); // slate-950
const GRID: RGBColor = RGBColor(51, 65, 85); // slate-700
const AXIS: RGBColor = RGBColor(148, 163, 184); // slate-400
const TITLE_COLOR: RGBColor = RGBColor(241, 245, 249); // slate-100
const LINE: RGBColor = RGBColor(56, 189, 248); // sky-400
const DOT: RGBColor = RGBColor(125, 211, 252); // sky-300
const GOAL: RGBColor = RGBColor(251, 146, 60); // orange-400

fn ensure_font_registered() {
    FONT_INIT.call_once(|| {
        let bytes: &'static [u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
        if register_font("sans-serif", FontStyle::Normal, bytes).is_err() {
            tracing::error!("Failed to register chart font (DejaVuSans)");
        }
    });
}

#[derive(Debug, Clone, Copy)]
pub struct WeightPoint {
    pub date: NaiveDate,
    pub weight_kg: f64,
}

#[derive(Debug, Clone)]
pub struct BuiltWeightChart {
    pub png: Vec<u8>,
    pub caption: String,
    pub window_label: &'static str,
    pub point_count: usize,
    pub start_kg: f64,
    pub end_kg: f64,
    pub delta_kg: f64,
    pub goal_kg: Option<f64>,
}

#[derive(Debug)]
pub enum BuildWeightChartError {
    NoEntries,
    NoDataInWindow,
    Render(anyhow::Error),
}

/// Aggregate weigh-ins, render PNG, and build a localized caption.
pub fn build_weight_chart(
    entries: &[WeightEntry],
    timezone: &str,
    days: Option<i64>,
    goal_kg: Option<f64>,
    user_lang: &Msg,
) -> Result<BuiltWeightChart, BuildWeightChartError> {
    if entries.is_empty() {
        return Err(BuildWeightChartError::NoEntries);
    }

    let tz: chrono_tz::Tz = timezone.parse().unwrap_or(chrono_tz::UTC);
    let cutoff = days.map(|d| Utc::now() - chrono::Duration::days(d));

    let mut by_day: std::collections::BTreeMap<NaiveDate, (DateTime<Utc>, f64)> =
        std::collections::BTreeMap::new();
    for e in entries {
        if cutoff.is_some_and(|c| e.recorded_at < c) {
            continue;
        }
        let day = e.recorded_at.with_timezone(&tz).date_naive();
        by_day
            .entry(day)
            .and_modify(|(at, kg)| {
                if e.recorded_at > *at {
                    *at = e.recorded_at;
                    *kg = e.weight_kg;
                }
            })
            .or_insert((e.recorded_at, e.weight_kg));
    }

    let points: Vec<WeightPoint> = by_day
        .into_iter()
        .map(|(date, (_, weight_kg))| WeightPoint { date, weight_kg })
        .collect();

    if points.is_empty() {
        return Err(BuildWeightChartError::NoDataInWindow);
    }

    let window_label = match days {
        Some(30) => "30d",
        Some(_) => "90d",
        None => "all",
    };
    let title = format!("weight — {window_label}");

    let png = render_weight_line_chart(&title, &points, goal_kg)
        .map_err(BuildWeightChartError::Render)?;

    let first = points.first().unwrap();
    let last = points.last().unwrap();
    let delta_kg = last.weight_kg - first.weight_kg;
    let caption = user_lang.weight_chart_caption(
        points.len(),
        first.weight_kg,
        last.weight_kg,
        delta_kg,
        goal_kg,
        window_label,
    );

    Ok(BuiltWeightChart {
        png,
        caption,
        window_label,
        point_count: points.len(),
        start_kg: first.weight_kg,
        end_kg: last.weight_kg,
        delta_kg,
        goal_kg,
    })
}

/// Parse chart window: empty/`90` → 90 days; `30`; `all`.
pub fn parse_days_arg(arg: &str) -> Result<Option<i64>, ()> {
    let arg = arg.trim().to_lowercase();
    if arg.is_empty() || arg == "90" {
        return Ok(Some(90));
    }
    if arg == "30" {
        return Ok(Some(30));
    }
    if arg == "all" {
        return Ok(None);
    }
    Err(())
}

/// Dark-themed weight history line chart (optional horizontal goal line).
pub fn render_weight_line_chart(
    title: &str,
    points: &[WeightPoint],
    goal_kg: Option<f64>,
) -> Result<Vec<u8>> {
    if points.is_empty() {
        return Err(anyhow!("no weight points to chart"));
    }

    ensure_font_registered();

    let mut sorted = points.to_vec();
    sorted.sort_by_key(|p| p.date);

    let start = sorted.first().unwrap().date;
    let end = sorted.last().unwrap().date;
    let end = if end == start {
        start
            .checked_add_signed(chrono::Duration::days(1))
            .unwrap_or(start)
    } else {
        end
    };

    let mut y_min = sorted
        .iter()
        .map(|p| p.weight_kg)
        .fold(f64::INFINITY, f64::min);
    let mut y_max = sorted
        .iter()
        .map(|p| p.weight_kg)
        .fold(f64::NEG_INFINITY, f64::max);
    if let Some(goal) = goal_kg {
        y_min = y_min.min(goal);
        y_max = y_max.max(goal);
    }
    if !y_min.is_finite() || !y_max.is_finite() {
        return Err(anyhow!("invalid weight values"));
    }
    if (y_max - y_min).abs() < 0.01 {
        y_min -= 1.0;
        y_max += 1.0;
    } else {
        let pad = ((y_max - y_min) * 0.12).max(0.5);
        y_min -= pad;
        y_max += pad;
    }

    let width = 960u32;
    let height = 480u32;
    let mut buffer = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        root.fill(&BG).context("failed to fill chart background")?;

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 26).into_font().color(&TITLE_COLOR))
            .margin(18)
            .x_label_area_size(42)
            .y_label_area_size(56)
            .build_cartesian_2d(start..end, y_min..y_max)
            .context("failed to build weight chart")?;

        chart
            .configure_mesh()
            .light_line_style(GRID.mix(0.45))
            .bold_line_style(GRID)
            .axis_style(AXIS)
            .x_labels(6)
            .y_labels(6)
            .x_label_formatter(&|d| d.format("%d/%m").to_string())
            .y_label_formatter(&|v| format!("{v:.1}"))
            .label_style(("sans-serif", 14).into_font().color(&AXIS))
            .draw()
            .context("failed to draw weight chart mesh")?;

        if let Some(goal) = goal_kg {
            chart
                .draw_series(LineSeries::new(
                    vec![(start, goal), (end, goal)],
                    GOAL.stroke_width(2),
                ))
                .context("failed to draw goal line")?
                .label(format!("goal {goal:.1} kg"))
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GOAL.stroke_width(2)));
        }

        let series: Vec<(NaiveDate, f64)> = sorted.iter().map(|p| (p.date, p.weight_kg)).collect();
        chart
            .draw_series(LineSeries::new(series.clone(), LINE.stroke_width(3)))
            .context("failed to draw weight line")?
            .label("weight")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], LINE.stroke_width(3)));

        chart
            .draw_series(
                series
                    .iter()
                    .map(|&(d, w)| Circle::new((d, w), 4, DOT.filled())),
            )
            .context("failed to draw weight dots")?;

        chart
            .configure_series_labels()
            .background_style(BG.mix(0.8))
            .border_style(AXIS)
            .label_font(("sans-serif", 14).into_font().color(&TITLE_COLOR))
            .draw()
            .context("failed to draw legend")?;

        root.present().context("failed to present weight chart")?;
    }

    encode_rgb_png(&buffer, width, height)
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
    use chrono::NaiveDate;

    #[test]
    fn renders_png_for_weight_series() {
        let points = vec![
            WeightPoint {
                date: NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
                weight_kg: 82.0,
            },
            WeightPoint {
                date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
                weight_kg: 81.4,
            },
            WeightPoint {
                date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
                weight_kg: 80.9,
            },
            WeightPoint {
                date: NaiveDate::from_ymd_opt(2026, 6, 22).unwrap(),
                weight_kg: 80.5,
            },
        ];
        let png = render_weight_line_chart("weight — 90d", &points, Some(78.0)).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 100);
    }

    #[test]
    fn renders_single_point() {
        let points = vec![WeightPoint {
            date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            weight_kg: 80.0,
        }];
        let png = render_weight_line_chart("weight", &points, None).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn parse_days_arg_values() {
        assert_eq!(parse_days_arg("").unwrap(), Some(90));
        assert_eq!(parse_days_arg("30").unwrap(), Some(30));
        assert_eq!(parse_days_arg("all").unwrap(), None);
        assert!(parse_days_arg("7").is_err());
    }
}
