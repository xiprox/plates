//! The rack's icons, as the mockups draw them, served to GPUI's `svg()` under `plates/`.
//! GPUI draws an SVG as a mask in one color, so each icon here is one shape, and a two-color icon
//! such as the running timer's hourglass, half filled with yellow, is two icons laid over each
//! other.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path.strip_prefix("plates/") {
            Some(name) => Ok(icon(name).map(|s| Cow::Owned(s.into_bytes()))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

const STROKE: &str = r#"fill="none" stroke="black" stroke-linecap="round" stroke-linejoin="round""#;

/// A bell's outline: its body flaring to the rim, the clapper under it, and the loop on top.
const BELL: &str = r#"<path d="M6 17c1.2-1.4 1.5-3.3 1.5-6.5a4.5 4.5 0 0 1 9 0c0 3.2.3 5.1 1.5 6.5z"/><path d="M10.2 19.8a2 2 0 0 0 3.6 0"/><path d="M12 4v2"/>"#;

fn icon(name: &str) -> Option<String> {
    let svg = |view: &str, body: String| {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}">{body}</svg>"#)
    };
    Some(match name {
        "grip.svg" => svg(
            "0 0 16 10",
            [(3, 2.5), (8, 2.5), (13, 2.5), (3, 7.5), (8, 7.5), (13, 7.5)]
                .iter()
                .map(|(x, y)| format!(r#"<circle cx="{x}" cy="{y}" r="1.3"/>"#))
                .collect(),
        ),
        "hourglass.svg" => svg(
            "0 0 24 24",
            format!(
                r#"<polygon points="7,4 17,4 12,12" fill-opacity="0.45"/><g {STROKE} stroke-width="1.6"><polygon points="7,4 17,4 12,12 17,20 7,20 12,12"/><line x1="6" y1="3.5" x2="18" y2="3.5"/><line x1="6" y1="20.5" x2="18" y2="20.5"/></g>"#
            ),
        ),
        // A box with its lid, for the archive of tiles you removed.
        "archive.svg" => svg(
            "0 0 24 24",
            format!(
                r#"<g {STROKE} stroke-width="1.7"><rect x="3.5" y="4.5" width="17" height="4.5" rx="1.2"/><path d="M5 9v9.3a1.2 1.2 0 0 0 1.2 1.2h11.6a1.2 1.2 0 0 0 1.2-1.2V9"/><path d="M10 13h4"/></g>"#
            ),
        ),
        "x.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M7 7l10 10M17 7L7 17" {STROKE} stroke-width="1.8"/>"#),
        ),
        "check.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M5 12.5l4.5 4.5L19 7.5" {STROKE} stroke-width="1.8"/>"#),
        ),
        // A crescent, for a reminder for tomorrow. Every place that draws it centers it against
        // something else, such as the count of snoozed tiles beside it, and a crescent's weight
        // sits in its lower curve, so centering its outline left it looking 1.4px low at 12px.
        // So it's drawn 2.86 units higher than its outline would sit, which puts the center of its
        // area, at y 14.86 of 24 before the shift, at the middle of its box.
        "moon.svg" => svg(
            "0 0 24 24",
            r#"<path d="M20 11.64A8.5 8.5 0 1 1 9.5 1.14a6.6 6.6 0 0 0 10.5 10.5z"/>"#.into(),
        ),
        // A bell for the tone a timer rings as it runs out, and the same bell struck through
        // while the tone is off.
        "bell.svg" => svg(
            "0 0 24 24",
            format!(r#"<g {STROKE} stroke-width="1.7">{BELL}</g>"#),
        ),
        // The strike cuts a gap through the bell's outline on either side of it, so the two
        // don't run together at 14px.
        "bell-off.svg" => svg(
            "0 0 24 24",
            format!(
                r#"<mask id="gap"><rect width="24" height="24" fill="white"/><path d="M4.5 4.5l15 15" stroke="black" stroke-width="5"/></mask><g {STROKE} stroke-width="1.7"><g mask="url(#gap)">{BELL}</g><path d="M4.5 4.5l15 15"/></g>"#
            ),
        ),
        "plus.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M12 5v14M5 12h14" {STROKE} stroke-width="1.8"/>"#),
        ),
        _ => return None,
    })
}
