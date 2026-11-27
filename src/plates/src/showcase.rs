//! A made-up rack, for pictures of Plates and for working on how it looks without real sessions.
//! `plates showcase` opens it, writing none of Plates' data files. Its tiles come from a few
//! weeks of real Claude Code sessions with the names of projects, clients, and people taken out,
//! so it shows at once the states record/areas/rack/02-tiles.md describes: a session asking for
//! approval, one at your turn with a question and a task list, one working with the app's plan for
//! its request and a sub-session under it, your own tile whose timer runs beside one whose reminder is due, and two snoozed tiles
//! behind the moon. It leaves out a finished turn you haven't read, so the rack stays short enough
//! to read at a glance (user, 2026-11-20).

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use plates_core::{
    Rack, SessionEvent, TileId, Timer, Tool, WrapUp,
    desktop::AppSession,
    session::{PlanChange, Step, StepStatus},
    tasks::{Change, State, TaskList},
};

/// A made-up task list for the showcase's open list, shaped like a real session's on
/// 2026-11-20 with its names and details changed: six pieces of a landing page, two waiting on
/// you, two on someone else, and two to do, with nothing under way between turns. The one
/// that changed last did so after the showcase's last visit to the session, 30 minutes ago.
pub fn task_list(now: SystemTime) -> TaskList {
    made_up(
        now,
        &[
            (
                "Category filters",
                State::Waiting,
                "Your take on the 8 categories",
                "you",
                50,
            ),
            (
                "Real visitor locations",
                State::Waiting,
                "City pass, then the query",
                "Sam",
                50,
            ),
            ("Clean thumbnails", State::Waiting, "", "Dana", 50),
            (
                "Results section rework",
                State::Todo,
                "Yearly total vs lifetime estimate",
                "",
                50,
            ),
            (
                "Notes batch",
                State::Todo,
                "End of day, all at once",
                "",
                50,
            ),
            (
                "Icon set",
                State::Waiting,
                "Where the originals live decides",
                "you",
                5,
            ),
        ],
    )
}

fn made_up(now: SystemTime, tasks: &[(&str, State, &str, &str, u64)]) -> TaskList {
    let mut list = TaskList::default();
    for &(title, state, status, waiting_on, minutes) in tasks {
        let change = Change {
            title: Some(title.to_string()),
            state: Some(state),
            status: Some(status.to_string()),
            waiting_on: Some(waiting_on.to_string()),
        };
        list.add(change, now - Duration::from_secs(minutes * 60))
            .expect("each made-up task has a title");
    }
    list
}

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
                pulls: Vec::new(),
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
            at(
                27,
                vec![SessionEvent::PlanChanged(PlanChange::Replace(
                    [
                        ("Reproduce the stuck scroll", StepStatus::Completed),
                        ("Find what holds the page", StepStatus::Completed),
                        ("Fix the list's touch handler", StepStatus::InProgress),
                        ("Test the fix on Android", StepStatus::Pending),
                    ]
                    .into_iter()
                    .map(|(text, status)| Step {
                        text: text.to_string(),
                        status,
                    })
                    .collect(),
                ))],
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
/// rack three times, the second with a tile's task list open and the third with the tile's card
/// open over it as Plates opens it, each with a shadow.
#[cfg(feature = "capture")]
mod canvas {
    /// The canvas's width over its height. The canvas grows to hold the racks with at least
    /// `MARGIN` around them, and then in its other direction to this shape.
    pub const RATIO: f32 = 1.6;
    pub const MARGIN: f32 = 88.;
    /// The gap between two racks.
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
    /// The radius of the canvas's corners, which are see-through past it (user, 2026-11-20).
    pub const RADIUS: f32 = 16.;
}

/// Lays the rack on the canvas, then `list`, the rack with a task list open, and then the rack
/// again with the card over it, `card.1` logical pixels right of the rack's window and `card.2`
/// down, where Plates opened it. The pictures come at the screen's scale, which the canvas takes
/// from the rack's width. The picture it returns has straight alpha, as a PNG stores it, unlike
/// the ones it takes, so it's for saving rather than for laying over another canvas.
#[cfg(feature = "capture")]
pub fn compose(
    rack: &Picture,
    list: Option<&Picture>,
    card: Option<(&Picture, f32, f32)>,
) -> Picture {
    use canvas::*;
    let scale = rack.width as f32 / crate::rack_view::RACK_W;
    let gap = GAP * scale;
    // The racks sit side by side in the canvas's middle with their tops level, as a list opens
    // downward, and the card reaches past the last rack's right edge.
    let mut columns = vec![(rack, rack.width as f32)];
    if let Some(list) = list {
        columns.push((list, list.width as f32));
    }
    if let Some((card, dx, _)) = card {
        let reach = dx * scale + card.width as f32;
        columns.push((rack, (rack.width as f32).max(reach)));
    }
    let group = columns.iter().map(|&(_, w)| w).sum::<f32>() + gap * (columns.len() - 1) as f32;
    let tallest = columns
        .iter()
        .map(|&(picture, _)| picture.height as f32)
        .chain(card.map(|(card, _, dy)| dy * scale + card.height as f32))
        .fold(0., f32::max);
    let margin = MARGIN * scale;
    let (mut w, mut h) = (group + 2. * margin, tallest + 2. * margin);
    if w < h * RATIO {
        w = h * RATIO;
    } else {
        h = w / RATIO;
    }
    let (width, height) = (w.round() as usize, h.round() as usize);
    let mut pixels = vec![[0f32; 3]; width * height];
    for y in 0..height {
        for x in 0..width {
            let t = 0.8 * y as f32 / height as f32 + 0.2 * x as f32 / width as f32;
            pixels[y * width + x] = [0, 1, 2].map(|c| FROM[c] + (TO[c] - FROM[c]) * t);
        }
    }
    let top = (height as f32 - tallest) / 2.;
    let mut x = (width as f32 - group) / 2.;
    let mut placed = Vec::new();
    for &(picture, column_w) in &columns {
        placed.push((picture, x, top));
        x += column_w + gap;
    }
    if let (Some((card, dx, dy)), Some(&(_, last, _))) = (card, placed.last()) {
        placed.push((card, last + dx * scale, top + dy * scale));
    }
    for &(picture, x, y) in &placed {
        shadow(&mut pixels, width, height, picture, x, y, scale);
    }
    for &(picture, x, y) in &placed {
        over(&mut pixels, width, height, picture, x as isize, y as isize);
    }
    let radius = RADIUS * scale;
    Picture {
        width: width as u32,
        height: height as u32,
        rgba: pixels
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                let alpha = inside_corners(i % width, i / width, width, height, radius);
                [p[0], p[1], p[2]]
                    .map(|c| c.round().clamp(0., 255.) as u8)
                    .into_iter()
                    .chain([(alpha * 255.).round() as u8])
            })
            .collect(),
    }
}

/// How much of the pixel at (x, y) lies inside a `width` by `height` picture with corners
/// rounded to `radius`: all of it away from the corners, and a part along each corner's arc, so
/// the arc's edge comes out smooth.
#[cfg(feature = "capture")]
pub fn inside_corners(x: usize, y: usize, width: usize, height: usize, radius: f32) -> f32 {
    let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
    let dx = (radius - cx).max(cx - (width as f32 - radius)).max(0.);
    let dy = (radius - cy).max(cy - (height as f32 - radius)).max(0.);
    if dx == 0. || dy == 0. {
        return 1.;
    }
    (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0., 1.)
}

#[cfg(all(test, feature = "capture"))]
mod tests {
    use super::{Picture, canvas, compose, inside_corners};

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> Picture {
        Picture {
            width,
            height,
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn three_racks_sit_level_with_the_card_over_the_last() {
        // Stand-ins at 2x, the size the code gives the real pictures: the showcase's 6 tiles with
        // the list's closed bar, 537 logical pixels tall, the rack with the list open, 770, and a
        // card 360 wide, 13 logical pixels right of its rack and 105 down.
        let rack = solid(608, 1074, [255, 0, 0]);
        let list = solid(608, 1540, [0, 255, 0]);
        let card = solid(720, 570, [0, 0, 255]);
        let p = compose(&rack, Some(&list), Some((&card, 13., 105.)));
        let (w, h) = (p.width as usize, p.height as usize);
        assert!(
            (w as f32 / h as f32 - canvas::RATIO).abs() < 0.01,
            "{w}x{h}"
        );

        let kind = |x: usize, y: usize| {
            let i = (y * w + x) * 4;
            match (p.rgba[i], p.rgba[i + 1], p.rgba[i + 2]) {
                (255, 0, 0) => 'R',
                (0, 255, 0) => 'G',
                (0, 0, 255) => 'B',
                _ => '.',
            }
        };
        // The list's rack is the tallest, so its top and bottom keep the margin, and the other
        // racks' tops are level with it.
        let margin = (canvas::MARGIN * 2.) as usize;
        let column = |x: usize| (0..h).filter(|&y| kind(x, y) != '.').collect::<Vec<_>>();
        let row: String = (0..w).map(|x| kind(x, margin + 10)).collect();
        let first_red = row.find('R').unwrap();
        let green = row.find('G').unwrap();
        let tops: Vec<usize> = [first_red, green].map(|x| column(x)[0]).to_vec();
        assert_eq!(tops[0], tops[1]);
        assert!(tops[0] >= margin && column(green).last().unwrap() + margin < h);
        assert!(first_red >= margin);

        // Left to right: the rack, a gap, the list, a gap, the rack the card opens over.
        let runs: Vec<(char, usize)> = row.chars().fold(Vec::new(), |mut runs, c| {
            match runs.last_mut() {
                Some((k, n)) if *k == c => *n += 1,
                _ => runs.push((c, 1)),
            }
            runs
        });
        let kinds: String = runs.iter().map(|r| r.0).collect();
        assert_eq!(kinds, ".R.G.R.");
        assert_eq!((runs[2].1, runs[4].1), (128, 128));
        let last = first_red + 608 + 128 + 608 + 128;
        assert_eq!(kind(last + 26 + 5, tops[0] + 210 + 5), 'B');
        assert_eq!(kind(last + 26 - 5, tops[0] + 210 + 5), 'R');

        assert_eq!(p.rgba[3], 0, "the top left corner is see-through");
        assert_eq!(p.rgba[(w * h / 2 + w / 2) * 4 + 3], 255);
    }

    #[test]
    fn the_canvas_corners_are_rounded_and_its_edges_are_not() {
        let (w, h, r) = (200, 100, 16.);
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            assert_eq!(inside_corners(x, y, w, h, r), 0., "corner ({x}, {y})");
        }
        for (x, y) in [
            (w / 2, 0),
            (0, h / 2),
            (w - 1, h / 2),
            (w / 2, h - 1),
            (16, 16),
        ] {
            assert_eq!(inside_corners(x, y, w, h, r), 1., "({x}, {y})");
        }
        // A pixel the arc crosses is partly inside.
        let edge = inside_corners(4, 4, w, h, r);
        assert!(edge > 0. && edge < 1., "{edge}");
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
