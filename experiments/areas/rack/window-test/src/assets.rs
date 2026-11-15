// The rack's icons, as the mockups draw them, served to GPUI's `svg()` under `plates/`.
// GPUI draws an SVG as a mask in one color, so each icon here is one shape, and a two-color icon
// such as the running timer's ring is two icons laid over each other.

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
        "ring-track.svg" => svg(
            "0 0 24 24",
            r#"<circle cx="12" cy="12" r="9" fill="none" stroke="black" stroke-width="2.4"/>"#
                .into(),
        ),
        "ring-x.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M10 10l4 4M14 10l-4 4" {STROKE} stroke-width="1.6"/>"#),
        ),
        "x.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M7 7l10 10M17 7L7 17" {STROKE} stroke-width="1.8"/>"#),
        ),
        "plus.svg" => svg(
            "0 0 24 24",
            format!(r#"<path d="M12 5v14M5 12h14" {STROKE} stroke-width="1.8"/>"#),
        ),
        // `ring-60.svg` is the arc of a timer with 60% of its time left.
        _ => {
            let left: f32 = name
                .strip_prefix("ring-")?
                .strip_suffix(".svg")?
                .parse()
                .ok()?;
            let offset = 56.55 * (1.0 - left / 100.0);
            svg(
                "0 0 24 24",
                format!(
                    r#"<circle cx="12" cy="12" r="9" transform="rotate(-90 12 12)" fill="none" stroke="black" stroke-width="2.4" stroke-linecap="round" stroke-dasharray="56.55" stroke-dashoffset="{offset:.2}"/>"#
                ),
            )
        }
    })
}
