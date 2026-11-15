// What a run of the window test measured, written as a report that stands on its own, so a
// run on another machine needs nothing else to be read.

use std::{
    fmt::Write as _,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::native::{self, MoveRecord};

pub struct Episode {
    pub kind: &'static str,
    pub frames: Vec<Instant>,
    pub moves_from: usize,
    pub moves_to: usize,
}

pub struct FieldTest {
    pub took_focus: Option<bool>,
    pub ended_by: &'static str,
    pub handed_back: Option<bool>,
}

#[derive(Default)]
pub struct Log {
    pub mode: &'static str,
    pub subclass: Option<bool>,
    pub answers_mouse_activate: bool,
    pub frame: Option<(f64, f64, f64, f64)>,
    pub squared_off: Option<(bool, bool)>,
    pub ex_style: u32,
    pub focus_kept_on_open: Option<bool>,
    pub episodes: Vec<Episode>,
    pub clicks: u32,
    pub clicks_kept_focus: u32,
    pub fields: Vec<FieldTest>,
    pub docks: u32,
    pub undocks: u32,
    pub captures: Vec<String>,
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn gaps_ms(times: impl Iterator<Item = Instant>) -> Vec<f64> {
    let times: Vec<Instant> = times.collect();
    let mut gaps: Vec<f64> = times
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0)
        .collect();
    gaps.sort_by(f64::total_cmp);
    gaps
}

// One line per motion: how often the window moved, and the longest wait between two moves.
// A gap longer than one and a half refresh intervals is a frame where the rack didn't move.
pub fn episode_line(e: &Episode, moves: &[MoveRecord], hz: u32) -> String {
    let moves = &moves[e.moves_from.min(moves.len())..e.moves_to.min(moves.len())];
    if moves.len() < 2 {
        return format!("{}: {}", e.kind, plural(moves.len(), "move", "moves"));
    }
    let gaps = gaps_ms(moves.iter().map(|m| m.at));
    let span = (moves[moves.len() - 1].at - moves[0].at).as_secs_f64();
    let limit = if hz > 0 { 1500.0 / hz as f64 } else { 25.0 };
    let long = gaps.iter().filter(|g| **g > limit).count();
    let mut took: Vec<f64> = moves
        .iter()
        .map(|m| m.took.as_secs_f64() * 1000.0)
        .collect();
    took.sort_by(f64::total_cmp);
    let frames = gaps_ms(e.frames.iter().copied());
    format!(
        "{}: {} over {:.2}s, {:.0} per second; between moves, median {:.1}ms, 95th percentile {:.1}ms, longest {:.1}ms; {} longer than {:.1}ms; SetWindowPos took a median {:.2}ms and at most {:.2}ms; frames drawn {:.0} per second",
        e.kind,
        plural(moves.len(), "move", "moves"),
        span,
        (moves.len() - 1) as f64 / span.max(1e-9),
        percentile(&gaps, 0.5),
        percentile(&gaps, 0.95),
        gaps.last().copied().unwrap_or(0.0),
        plural(long, "gap", "gaps"),
        limit,
        percentile(&took, 0.5),
        took.last().copied().unwrap_or(0.0),
        frames.len() as f64 / (frames.iter().sum::<f64>() / 1000.0).max(1e-9),
    )
}

// Days since 1970-01-01 to a civil date, after Howard Hinnant's days_from_civil inverse.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn stamp() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{:02}{:02}{:02}", t / 3600, t % 3600 / 60, t % 60),
    )
}

pub fn write(log: &Log, moves: &[MoveRecord], rack_center: (f64, f64), scale: f64) -> String {
    let display = native::display_at(rack_center);
    let (date, _) = stamp();
    let mut s = String::new();
    let _ = writeln!(s, "Plates window test, {} run", log.mode);
    let _ = writeln!(s, "Date (UTC): {date}");
    let _ = writeln!(s, "OS: {}", native::os_version());
    let _ = writeln!(
        s,
        "Displays: {}; the rack's is {}x{} physical pixels at {:.0}% scale and {} Hz",
        native::monitor_count(),
        display.width,
        display.height,
        scale * 100.0,
        display.hz
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Window");
    let _ = writeln!(
        s,
        "- extended styles: {}",
        native::describe_ex_style(log.ex_style)
    );
    let _ = writeln!(
        s,
        "- window subclass installed: {}",
        log.subclass.map_or("not yet", yes)
    );
    let _ = writeln!(
        s,
        "- the subclass answers WM_MOUSEACTIVATE with MA_NOACTIVATE: {}",
        if log.answers_mouse_activate {
            "yes"
        } else {
            "no, left to GPUI (--style-only)"
        }
    );
    if let Some((ww, wh, cw, ch)) = log.frame {
        let _ = writeln!(
            s,
            "- window rectangle {ww}x{wh}, client area {cw}x{ch}, in physical pixels"
        );
    }
    if let Some((corners, border)) = log.squared_off {
        let _ = writeln!(
            s,
            "- DWM corner rounding turned off: {}; DWM border turned off: {}",
            yes(corners),
            yes(border)
        );
    }
    if let Some(kept) = log.focus_kept_on_open {
        let _ = writeln!(
            s,
            "- keyboard focus stayed in the app that had it when the rack opened: {}",
            yes(kept)
        );
    }
    for c in &log.captures {
        let _ = writeln!(s, "- screen capture: {c}");
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "Motion");
    if log.episodes.is_empty() {
        let _ = writeln!(s, "- none");
    }
    for e in &log.episodes {
        let _ = writeln!(s, "- {}", episode_line(e, moves, display.hz));
    }
    let altered = moves.iter().filter(|m| m.altered).count();
    let _ = writeln!(
        s,
        "- docked {} and undocked {}",
        plural(log.docks as usize, "time", "times"),
        plural(log.undocks as usize, "time", "times")
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "The OS");
    if log.subclass == Some(true) {
        let loops = native::OS_MOVE_LOOPS.load(std::sync::atomic::Ordering::Relaxed);
        let _ = writeln!(
            s,
            "- entered its own move or size loop on the rack: {}",
            plural(loops as usize, "time", "times")
        );
        let acts = native::MOUSE_ACTIVATIONS.load(std::sync::atomic::Ordering::Relaxed);
        let _ = writeln!(
            s,
            "- sent WM_MOUSEACTIVATE: {}",
            plural(acts as usize, "time", "times")
        );
    }
    let _ = writeln!(
        s,
        "- put the rack somewhere other than where it was moved: {} of {}",
        altered,
        plural(moves.len(), "move", "moves")
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Focus");
    let _ = writeln!(
        s,
        "- clicks on tiles and the strip: {}, after which keyboard focus stayed in the app that had it: {}",
        log.clicks, log.clicks_kept_focus
    );
    for (i, f) in log.fields.iter().enumerate() {
        let took = f.took_focus.map_or("not measured", yes);
        let back = f.handed_back.map_or("not handed back", yes);
        let _ = writeln!(
            s,
            "- inline field {}: took keyboard focus: {took}; ended by {}; focus went back to the app you were in: {back}",
            i + 1,
            f.ended_by
        );
    }
    s
}
