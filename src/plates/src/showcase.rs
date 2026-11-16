//! A made-up rack, for pictures of Plates and for working on how it looks without real sessions.
//! `plates showcase` opens it, writing none of Plates' data files. Its tiles come from a few
//! weeks of real Claude Code sessions with the names of projects, clients, and people taken out,
//! so it shows at once the states record/areas/rack/02-tiles.md describes: a session asking for
//! approval, one at your turn with a question, one working with a sub-session under it, one
//! finished and unread, your own tile whose timer runs beside one whose reminder is due, and two
//! snoozed tiles behind the moon.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use plates_core::{Rack, SessionEvent, TileId, Timer, Tool, WrapUp, desktop::AppSession};

/// Builds the made-up rack as it stands at `now`, and the tile whose hover card the showcase's
/// picture shows.
pub fn rack(now: SystemTime) -> (Rack, TileId) {
    let mut rack = Rack::new();
    let ago = |minutes: u64| now - Duration::from_secs(minutes * 60);
    let mut session = |id: &str,
                       folder: &str,
                       title: &str,
                       parent: Option<&str>,
                       events: Vec<(u64, SessionEvent)>| {
        let mut tile = None;
        for (minutes, event) in events {
            tile = Some(rack.apply(id, &PathBuf::from(folder), event, ago(minutes)));
        }
        rack.set_app_session(
            id,
            &AppSession {
                title: Some(title.to_string()),
                id: Some(format!("local_{id}")),
                focused_at: None,
                parent: parent.map(|p| format!("local_{p}")),
                cli_id: Some(id.to_string()),
            },
        );
        rack.place_under_parent(id);
        tile.expect("a made-up session should have events")
    };
    let tool = |name: &str, command: Option<&str>, target: &str| Tool {
        name: name.to_string(),
        command: command.map(str::to_string),
        file: None,
        target: Some(target.to_string()),
    };
    let did = |activity: &str| SessionEvent::ToolFinished {
        activity: Some(activity.to_string()),
        failed: false,
    };
    let start = |prompt: &str| {
        vec![
            SessionEvent::Started {
                title: None,
                resumed: false,
            },
            SessionEvent::PromptSubmitted {
                title: None,
                prompt: Some(prompt.to_string()),
            },
        ]
    };
    let at = |minutes: u64, events: Vec<SessionEvent>| {
        events
            .into_iter()
            .map(move |e| (minutes, e))
            .collect::<Vec<_>>()
    };
    let ended = |recap: &str, ask: Option<&str>, choices: &[&str], message: &str| {
        vec![
            SessionEvent::WrappedUp(WrapUp {
                recap: Some(recap.to_string()),
                ask: ask.map(str::to_string),
                blocked: false,
                choices: choices.iter().map(|c| c.to_string()).collect(),
            }),
            SessionEvent::TurnEnded {
                last_message: Some(message.to_string()),
                background: Vec::new(),
            },
        ]
    };

    let merge = tool("Bash", Some("gh pr merge"), "Merge the PR");
    session(
        "admin",
        "admin",
        "Magic links admin tool",
        None,
        [
            at(
                35,
                start("Add a tool to the admin app that sends a user a fresh magic link"),
            ),
            at(
                11,
                vec![
                    did("Editing MagicLinks.tsx"),
                    did("Ran the tests"),
                    did("Opened the PR"),
                ],
            ),
            at(
                3,
                vec![
                    SessionEvent::ToolPending {
                        tool: merge.clone(),
                    },
                    SessionEvent::ApprovalAsked { tool: Some(merge) },
                ],
            ),
        ]
        .concat(),
    );
    let card = session(
        "landing",
        "web",
        "V2 landing redesign",
        None,
        [
            at(
                52,
                start("The footnote under the amounts reads wrong. Rewrite it"),
            ),
            at(
                9,
                vec![did("Editing AmountFrame.tsx"), did("Took a screenshot")],
            ),
            at(
                2,
                ended(
                    "I rewrote the footnote under both amount frames three ways, and none is \
                     applied yet.",
                    Some("Which footnote should go in?"),
                    &["A", "B", "C"],
                    "Footnote: A, B, or C?",
                ),
            ),
        ]
        .concat(),
    );
    session(
        "scroll",
        "portal",
        "Mobile scroll bug in portal",
        None,
        [
            at(
                28,
                start("The portal's page stops scrolling on phones after a swipe on the list"),
            ),
            at(6, vec![did("Reading EmbedFrame.tsx")]),
            at(
                0,
                vec![SessionEvent::ToolPending {
                    tool: tool("Edit", None, "EmbedFrame.tsx"),
                }],
            ),
        ]
        .concat(),
    );
    session(
        "swipe-test",
        "portal",
        "Test the embed's fund list swipe on Android",
        Some("scroll"),
        [
            at(
                5,
                start("Test whether a swipe on the fund list scrolls the page on Android"),
            ),
            at(
                1,
                vec![SessionEvent::ToolPending {
                    tool: tool("Bash", Some("npm test"), "Running the tests"),
                }],
            ),
        ]
        .concat(),
    );
    session(
        "loads",
        "analytics",
        "Count widget loads via Cloudflare",
        None,
        [
            at(
                44,
                start("PostHog only shows a floor for the widget. Count its loads in Cloudflare"),
            ),
            at(
                23,
                vec![
                    did("Using query-run"),
                    did("Using graphql_query"),
                    did("Using graphql_query"),
                ],
            ),
            at(
                13,
                ended(
                    "Cloudflare counted the widget's loads, and against them the page converted \
                     about 17% of visits.",
                    None,
                    &[],
                    "Converts ~17% of visits.",
                ),
            ),
        ]
        .concat(),
    );

    // Two finished sessions and an errand you dismissed wait in the archive.
    let finished: Vec<TileId> = [
        (
            "datagrip",
            "DataGrip MCP access",
            "Can you reach the DataGrip MCP?",
        ),
        ("issue", "GitHub issue 470", "What's this issue about?"),
    ]
    .into_iter()
    .map(|(id, title, prompt)| {
        session(
            id,
            "web",
            title,
            None,
            [
                at(200, start(prompt)),
                at(190, ended("Answered.", None, &[], "Done.")),
            ]
            .concat(),
        )
    })
    .collect();
    for tile in finished {
        rack.remove(tile, ago(150));
    }
    let hub = rack.add_own("Order a USB-C hub", ago(300));
    rack.remove(hub, ago(240));

    // Your own tiles wait on something else: one for a PR's checks, with its timer running, and
    // one for a reply on Slack, whose reminder has come due.
    let checks = rack.add_own("Merge PR after checks pass", ago(12));
    rack.start_timer(checks, 15, ago(5));
    let slack = rack.add_own("Respond on Slack", ago(40));
    rack.start_timer(slack, 30, ago(31));
    for title in ["Fix the analytics MCP token", "Customer support docs plan"] {
        let id = rack.add_own(title, ago(90));
        rack.snooze(id, Timer::later(ago(20), 4), ago(20));
    }
    rack.tick(now);
    (rack, card)
}

/// A picture as RGBA bytes, row by row, with the color already multiplied by the alpha, as GPUI
/// draws it.
#[cfg(feature = "capture")]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The README's picture, in logical pixels: a canvas in a README's usual 16:10, and on it the
/// rack twice, the second with a tile's card open over it as Plates opens it, each with a shadow.
#[cfg(feature = "capture")]
mod canvas {
    pub const WIDTH: f32 = 1200.;
    pub const HEIGHT: f32 = 750.;
    /// The gap between the two racks.
    pub const GAP: f32 = 64.;
    /// The canvas's color at its top and its bottom, a warm stone gray that darkens a little
    /// downward, as a surface under a light from above would, so the dark rack stands out on
    /// GitHub's light and dark themes alike (user, 2026-11-16).
    pub const FROM: [f32; 3] = [0xD2 as f32, 0xCE as f32, 0xC6 as f32];
    pub const TO: [f32; 3] = [0xBC as f32, 0xB7 as f32, 0xAE as f32];
    /// The shadow's blur, how far down it falls, and how dark it gets.
    pub const BLUR: f32 = 18.;
    pub const DROP: f32 = 16.;
    pub const DARK: f32 = 0.34;
}

/// Lays the rack on the canvas twice, and over the second the card, `card.1` logical pixels right
/// of the rack's window and `card.2` down, where Plates opened it. The pictures come at the
/// screen's scale, which the canvas takes from the rack's width.
#[cfg(feature = "capture")]
pub fn compose(rack: &Picture, card: Option<(&Picture, f32, f32)>) -> Picture {
    use canvas::*;
    let scale = rack.width as f32 / crate::rack_view::RACK_W;
    let (width, height) = ((WIDTH * scale) as usize, (HEIGHT * scale) as usize);
    let mut pixels = vec![[0f32; 3]; width * height];
    for y in 0..height {
        for x in 0..width {
            let t = 0.8 * y as f32 / height as f32 + 0.2 * x as f32 / width as f32;
            pixels[y * width + x] = [0, 1, 2].map(|c| FROM[c] + (TO[c] - FROM[c]) * t);
        }
    }
    // The racks sit side by side in the canvas's middle, the second with the card reaching past
    // its right edge.
    let rack_w = rack.width as f32;
    let group = match card {
        Some((card, dx, _)) => rack_w + GAP * scale + rack_w.max(dx * scale + card.width as f32),
        None => rack_w,
    };
    let rack_x = (width as f32 - group) / 2.;
    let rack_y = (height as f32 - rack.height as f32) / 2.;
    let mut placed = vec![(rack, rack_x, rack_y)];
    if let Some((card, dx, dy)) = card {
        let second = rack_x + rack_w + GAP * scale;
        placed.push((rack, second, rack_y));
        placed.push((card, second + dx * scale, rack_y + dy * scale));
    }
    for &(picture, x, y) in &placed {
        shadow(&mut pixels, width, height, picture, x, y, scale);
    }
    for &(picture, x, y) in &placed {
        over(&mut pixels, width, height, picture, x as isize, y as isize);
    }
    Picture {
        width: width as u32,
        height: height as u32,
        rgba: pixels
            .iter()
            .flat_map(|p| {
                [p[0], p[1], p[2]]
                    .map(|c| c.round().clamp(0., 255.) as u8)
                    .into_iter()
                    .chain([255])
            })
            .collect(),
    }
}

// Darkens the canvas under a picture's shape, blurred and dropped a little, as a light from above
// would. Three box blurs in each direction come close to a Gaussian blur.
#[cfg(feature = "capture")]
fn shadow(
    pixels: &mut [[f32; 3]],
    width: usize,
    height: usize,
    picture: &Picture,
    x: f32,
    y: f32,
    scale: f32,
) {
    use canvas::*;
    let radius = (BLUR * scale) as usize;
    let pad = radius * 3;
    let (w, h) = (
        picture.width as usize + 2 * pad,
        picture.height as usize + 2 * pad,
    );
    let mut mask = vec![0f32; w * h];
    for py in 0..picture.height as usize {
        for px in 0..picture.width as usize {
            let alpha = picture.rgba[(py * picture.width as usize + px) * 4 + 3];
            mask[(py + pad) * w + px + pad] = f32::from(alpha) / 255.;
        }
    }
    for _ in 0..3 {
        blur(&mut mask, w, h, radius, 1, w);
        blur(&mut mask, h, w, radius, w, 1);
    }
    let left = x as isize - pad as isize;
    let top = (y + DROP * scale) as isize - pad as isize;
    for my in 0..h {
        for mx in 0..w {
            let (cx, cy) = (left + mx as isize, top + my as isize);
            if cx < 0 || cy < 0 || cx as usize >= width || cy as usize >= height {
                continue;
            }
            let keep = 1. - DARK * mask[my * w + mx];
            let p = &mut pixels[cy as usize * width + cx as usize];
            *p = p.map(|c| c * keep);
        }
    }
}

// A box blur of `radius` along lines of `len` values `step` apart, for `count` lines whose first
// values are `next` apart.
#[cfg(feature = "capture")]
fn blur(values: &mut [f32], len: usize, count: usize, radius: usize, step: usize, next: usize) {
    let mut sums = vec![0f32; len + 1];
    let mut line = vec![0f32; len];
    for i in 0..count {
        let start = i * next;
        for j in 0..len {
            sums[j + 1] = sums[j] + values[start + j * step];
        }
        for (j, v) in line.iter_mut().enumerate() {
            let (lo, hi) = (j.saturating_sub(radius), (j + radius + 1).min(len));
            *v = (sums[hi] - sums[lo]) / (2 * radius + 1) as f32;
        }
        for (j, v) in line.iter().enumerate() {
            values[start + j * step] = *v;
        }
    }
}

// Lays a picture over the canvas at (x, y). Its color is already multiplied by its alpha, so the
// canvas behind it keeps what the picture's alpha leaves.
#[cfg(feature = "capture")]
fn over(
    pixels: &mut [[f32; 3]],
    width: usize,
    height: usize,
    picture: &Picture,
    x: isize,
    y: isize,
) {
    for py in 0..picture.height as usize {
        for px in 0..picture.width as usize {
            let (cx, cy) = (x + px as isize, y + py as isize);
            if cx < 0 || cy < 0 || cx as usize >= width || cy as usize >= height {
                continue;
            }
            let i = (py * picture.width as usize + px) * 4;
            let src = &picture.rgba[i..i + 4];
            let keep = 1. - f32::from(src[3]) / 255.;
            let p = &mut pixels[cy as usize * width + cx as usize];
            *p = [0, 1, 2].map(|c| f32::from(src[c]) + p[c] * keep);
        }
    }
}
