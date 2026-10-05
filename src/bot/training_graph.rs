use anyhow::{Context, Result, anyhow};
use plotters::prelude::*;
use plotters::style::register_font;
use std::io::Cursor;
use std::sync::Once;

use crate::db::training::GroupTrainingStat;

static FONT_INIT: Once = Once::new();

// Dark theme — muted surfaces, single cool accent for bars.
const BG: RGBColor = RGBColor(15, 23, 42); // slate-950
const PANEL: RGBColor = RGBColor(30, 41, 59); // slate-800 track
const GRID: RGBColor = RGBColor(51, 65, 85); // slate-700
const AXIS: RGBColor = RGBColor(148, 163, 184); // slate-400
const TITLE_COLOR: RGBColor = RGBColor(241, 245, 249); // slate-100
const LABEL_COLOR: RGBColor = RGBColor(203, 213, 225); // slate-300
const BAR: RGBColor = RGBColor(56, 189, 248); // sky-400
const BAR_TOP: RGBColor = RGBColor(125, 211, 252); // sky-300 (slightly brighter for #1)

fn ensure_font_registered() {
    FONT_INIT.call_once(|| {
        // Bundled DejaVu Sans — pure Rust path via ab_glyph (no fontconfig).
        let bytes: &'static [u8] = include_bytes!("../../assets/fonts/DejaVuSans.ttf");
        if register_font("sans-serif", FontStyle::Normal, bytes).is_err() {
            tracing::error!("Failed to register chart font (DejaVuSans)");
        }
    });
}

/// Compact chart label from [`GroupTrainingStat::display_name`].
fn display_label(stat: &GroupTrainingStat) -> String {
    truncate_chars(&stat.display_name(), 16)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

fn rank_label(rank_from_top: usize, name: &str) -> String {
    format!("{}. {name}", rank_from_top + 1)
}

/// Bar fill: same accent family; leader slightly brighter.
fn bar_color(rank_from_top: usize) -> RGBColor {
    if rank_from_top == 0 { BAR_TOP } else { BAR }
}

/// Render a dark-themed horizontal bar chart of training sessions per member.
pub fn render_sessions_chart(title: &str, stats: &[GroupTrainingStat]) -> Result<Vec<u8>> {
    if stats.is_empty() {
        return Err(anyhow!("no training stats to chart"));
    }

    ensure_font_registered();

    // Top of chart = most trained (stats already DESC). Reverse so index 0 is bottom.
    let ordered: Vec<&GroupTrainingStat> = stats.iter().rev().collect();
    let n = ordered.len();
    let max_sessions = ordered.iter().map(|s| s.sessions).max().unwrap_or(1).max(1);

    let width = 960u32;
    let row_h = 54u32;
    let height = (120 + n as u32 * row_h).clamp(320, 1200);

    let mut buffer = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        root.fill(&BG).context("failed to fill chart background")?;

        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 28).into_font().color(&TITLE_COLOR))
            .margin(18)
            .x_label_area_size(48)
            .y_label_area_size(190)
            .build_cartesian_2d(0i64..(max_sessions + 1), 0f64..(n as f64))
            .context("failed to build chart")?;

        let x_ticks = (max_sessions as usize + 1).clamp(2, 13);

        chart
            .configure_mesh()
            .disable_y_mesh()
            .light_line_style(GRID.mix(0.45))
            .bold_line_style(GRID)
            .axis_style(AXIS)
            .x_desc("sessions")
            .x_labels(x_ticks)
            .x_label_formatter(&|v| format!("{v}"))
            .y_labels(0)
            .label_style(("sans-serif", 15).into_font().color(&AXIS))
            .draw()
            .context("failed to draw chart mesh")?;

        // Subtle track behind each bar.
        chart
            .draw_series(ordered.iter().enumerate().map(|(i, _)| {
                let y0 = i as f64 + 0.24;
                let y1 = i as f64 + 0.76;
                Rectangle::new([(0, y0), (max_sessions, y1)], PANEL.filled())
            }))
            .context("failed to draw bar tracks")?;

        chart
            .draw_series(ordered.iter().enumerate().map(|(i, stat)| {
                let rank_from_top = n - 1 - i;
                let color = bar_color(rank_from_top);
                let y0 = i as f64 + 0.24;
                let y1 = i as f64 + 0.76;
                Rectangle::new([(0, y0), (stat.sessions, y1)], color.filled())
            }))
            .context("failed to draw bars")?;

        for (i, stat) in ordered.iter().enumerate() {
            let rank_from_top = n - 1 - i;
            let label = rank_label(rank_from_top, &display_label(stat));
            let y_mid = i as f64 + 0.5;
            let (_, py) = chart.backend_coord(&(0, y_mid));
            root.draw(&Text::new(
                label,
                (10, py.saturating_sub(8)),
                ("sans-serif", 15).into_font().color(&LABEL_COLOR),
            ))
            .context("failed to draw name label")?;
        }

        chart
            .draw_series(ordered.iter().enumerate().map(|(i, stat)| {
                Text::new(
                    format!("  {}", stat.sessions),
                    (stat.sessions, i as f64 + 0.5),
                    ("sans-serif", 17).into_font().color(&TITLE_COLOR),
                )
            }))
            .context("failed to draw value labels")?;

        root.present().context("failed to present chart")?;
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

    #[test]
    fn encode_rgb_png_writes_signature_ihdr_idat_iend() {
        let png = encode_rgb_png(&[255, 0, 0], 1, 1).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
        assert!(png.windows(4).any(|w| w == b"IHDR"));
        assert!(png.windows(4).any(|w| w == b"IDAT"));
        assert!(png.windows(4).any(|w| w == b"IEND"));
        assert!(png.len() > 40);
    }

    #[test]
    fn renders_png_for_stats() {
        let stats = vec![
            GroupTrainingStat {
                username: Some("Gregouaer".into()),
                first_name: Some("Greg".into()),
                sessions: 12,
            },
            GroupTrainingStat {
                username: None,
                first_name: Some("Alice".into()),
                sessions: 4,
            },
            GroupTrainingStat {
                username: Some("MaxButor".into()),
                first_name: Some("Max".into()),
                sessions: 3,
            },
            GroupTrainingStat {
                username: Some("Djyki28".into()),
                first_name: None,
                sessions: 1,
            },
        ];
        let png = render_sessions_chart("training — july 2026", &stats).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 100);
    }

    #[test]
    fn uses_first_name_when_username_missing() {
        let s = GroupTrainingStat {
            username: None,
            first_name: Some("Alice".into()),
            sessions: 1,
        };
        assert_eq!(display_label(&s), "Alice");
    }

    #[test]
    fn uses_generic_fallback_when_no_display_name() {
        let s = GroupTrainingStat {
            username: None,
            first_name: None,
            sessions: 1,
        };
        assert_eq!(display_label(&s), "Member");
        assert!(!display_label(&s).chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn lowercases_usernames() {
        let s = GroupTrainingStat {
            username: Some("MaxButor".into()),
            first_name: Some("Max".into()),
            sessions: 1,
        };
        assert_eq!(display_label(&s), "@maxbutor");
    }

    #[test]
    fn prefers_username_over_first_name() {
        let s = GroupTrainingStat {
            username: Some("  @MaxButor  ".into()),
            first_name: Some("Max".into()),
            sessions: 1,
        };
        assert_eq!(display_label(&s), "@maxbutor");
    }
}
