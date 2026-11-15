// The rack's drag, throw, and dock numbers from record/areas/rack/01-window.md.
// That page gives lengths and speeds in logical pixels, and the window test moves the window in
// physical pixels, so every caller multiplies them by the display's scale factor.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const DRAG_THRESHOLD: f64 = 5.0;
pub const SAMPLE_SPAN: Duration = Duration::from_millis(90);
pub const MAX_THROW: f64 = 3200.0;
pub const FRICTION: f64 = 3.2;
pub const BOUNCE: f64 = 0.45;
pub const STOP_SPEED: f64 = 6.0;
pub const HOLD: Duration = Duration::from_millis(1500);
pub const UNDOCK_SPEED: f64 = 950.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

// A screen edge the rack stops at is one with no other monitor beyond it, so a rack thrown or
// dragged toward a second monitor carries on into it.
#[derive(Clone, Copy, Debug)]
pub struct Edges {
    pub area: Rect,
    pub open_left: bool,
    pub open_right: bool,
    pub open_top: bool,
    pub open_bottom: bool,
}

// The throw takes the rack's speed over the drag's last 90ms, as the mockups' prototype does, so
// a drag that stops before the release throws nothing.
#[derive(Default)]
pub struct Samples(VecDeque<(Instant, f64, f64)>);

impl Samples {
    pub fn push(&mut self, at: Instant, pos: (f64, f64)) {
        self.0.push_back((at, pos.0, pos.1));
        while self.0.len() > 2 && at - self.0[0].0 > SAMPLE_SPAN {
            self.0.pop_front();
        }
    }

    pub fn throw_velocity(&self, now: Instant, scale: f64) -> (f64, f64) {
        let (Some(a), Some(b)) = (self.0.front(), self.0.back()) else {
            return (0.0, 0.0);
        };
        let span = (b.0 - a.0).as_secs_f64();
        if span <= 0.008 || now - b.0 >= Duration::from_millis(70) {
            return (0.0, 0.0);
        }
        let (vx, vy) = ((b.1 - a.1) / span, (b.2 - a.2) / span);
        let speed = vx.hypot(vy);
        let cap = MAX_THROW * scale;
        if speed > cap {
            (vx * cap / speed, vy * cap / speed)
        } else {
            (vx, vy)
        }
    }
}

// Where the dragged rack goes, and which side edge it's held against, if any.
// It counts as held while the pointer would carry it past the edge, so moving back off the edge
// cancels the hold.
pub fn clamp_drag(want: (f64, f64), size: (f64, f64), e: &Edges) -> ((f64, f64), Option<Side>) {
    let (mut x, mut y) = want;
    let mut held = None;
    if !e.open_left && x <= e.area.left {
        x = e.area.left;
        held = Some(Side::Left);
    }
    if !e.open_right && x + size.0 >= e.area.right {
        x = e.area.right - size.0;
        held = Some(Side::Right);
    }
    if !e.open_top && y < e.area.top {
        y = e.area.top;
    }
    if !e.open_bottom && y + size.1 > e.area.bottom {
        y = e.area.bottom - size.1;
    }
    ((x, y), held)
}

// One frame of a throw: the rack keeps e^-3.2 of its speed per second and bounces off a closed
// edge with 45% of it, and returns false once it has stopped.
pub fn step_throw(
    pos: (f64, f64),
    vel: (f64, f64),
    size: (f64, f64),
    e: &Edges,
    dt: f64,
    scale: f64,
) -> ((f64, f64), (f64, f64), bool) {
    let (mut x, mut y) = (pos.0 + vel.0 * dt, pos.1 + vel.1 * dt);
    let k = (-FRICTION * dt).exp();
    let (mut vx, mut vy) = (vel.0 * k, vel.1 * k);
    if !e.open_left && x < e.area.left {
        x = e.area.left;
        vx = -vx * BOUNCE;
    }
    if !e.open_right && x + size.0 > e.area.right {
        x = e.area.right - size.0;
        vx = -vx * BOUNCE;
    }
    if !e.open_top && y < e.area.top {
        y = e.area.top;
        vy = -vy * BOUNCE;
    }
    if !e.open_bottom && y + size.1 > e.area.bottom {
        y = e.area.bottom - size.1;
        vy = -vy * BOUNCE;
    }
    let stop = STOP_SPEED * scale;
    if vx.abs() < stop && vy.abs() < stop {
        return ((x, y), (0.0, 0.0), false);
    }
    ((x, y), (vx, vy), true)
}
