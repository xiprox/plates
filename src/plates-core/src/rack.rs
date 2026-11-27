//! The rack's tiles in their order, and the rules from record/areas/core/01-core.md that change
//! them.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use crate::{
    desktop::AppSession,
    session::{
        COMPACTED, CarriedWait, RECENT_TOOLS, Recent, Session, SessionEvent, SessionState, Step,
        finished, first_line, prompt_title,
    },
    timer::{DEFAULT_MINUTES, Timer},
};

/// A look at a session at your turn, from a click on its tile or from opening it in the app. The
/// first look reads the turn and stops its timer, and nothing starts a timer again until the
/// session's next turn, since a look is enough to read it (user, 2026-11-17). A timer you set or
/// canceled yourself during the turn is yours, and no look touches it. It returns whether the tile
/// changed.
fn look(s: &mut Session, timer: &mut Timer) -> bool {
    if s.read {
        return false;
    }
    s.read = true;
    if !s.own_timer {
        timer.stop();
    }
    true
}

/// Whether a tile off the rack is snoozed: it waits for a reminder for later, rather than for its
/// session to need you.
fn snoozed(tile: &Tile) -> bool {
    tile.timer.is_tomorrow()
}

/// Whether a session off the rack is one you dismissed that the archive lists.
fn dismissed(tile: &Tile) -> bool {
    tile.session().is_some_and(|s| s.dismissed_at.is_some())
}

/// Notes that you set or canceled a session tile's timer yourself, which no look stops until the
/// session's next state.
fn take_in_hand(tile: &mut Tile) {
    if let Body::Session(s) = &mut tile.body {
        s.own_timer = true;
    }
}

/// Clears what a session keeps of its last turn, as a new turn starts.
fn begin_turn(s: &mut Session) {
    s.summary = None;
    s.message = None;
    s.wrap_up = None;
    s.wait = None;
    s.ping = None;
    s.recent.clear();
    // A plan with a step still pending carries on into the turn, where Claude can advance it.
    // `transcript::plan` drops a plan at each turn's start the same way.
    if finished(&s.plan) {
        s.plan.clear();
    }
    s.background.clear();
    s.status = None;
    s.activity = None;
    s.tool_running = false;
    s.pending_tool = None;
    s.asks = None;
    s.compacting = false;
}

/// How long an ended session's tile stays before it clears itself.
pub const ENDED_CLEARS_AFTER: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Own {
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "a rack holds a few dozen tiles at most, so boxing the session would save nothing that matters"
)]
pub enum Body {
    Session(Session),
    Own(Own),
}

/// Why a tile needs you. A change of reason counts as starting to need you again, since the
/// tile's status color changes with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Approval,
    YourTurn,
    /// A finished turn you haven't read, which asks for your attention but not a response.
    Unread,
    TimeUp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub id: TileId,
    pub body: Body,
    pub timer: Timer,
    /// Why the tile needs you and since when, which the rack uses to brighten it once.
    pub need: Option<(Need, SystemTime)>,
}

impl Tile {
    pub fn title(&self) -> String {
        match &self.body {
            Body::Session(s) => s.title(),
            Body::Own(o) => o.title.clone(),
        }
    }

    pub fn session(&self) -> Option<&Session> {
        match &self.body {
            Body::Session(s) => Some(s),
            Body::Own(_) => None,
        }
    }

    pub fn is_own(&self) -> bool {
        matches!(self.body, Body::Own(_))
    }

    fn current_need(&self) -> Option<Need> {
        if self.timer.is_time_up() {
            return Some(Need::TimeUp);
        }
        match self.session().map(|s| s.state) {
            Some(SessionState::NeedsApproval) => Some(Need::Approval),
            // A turn you've read still waits for your reply, which its tile shows, but it no
            // longer asks for your attention (user, 2026-11-15).
            Some(SessionState::YourTurn) if !self.session().is_some_and(|s| s.read) => {
                Some(Need::YourTurn)
            }
            Some(SessionState::Unread) => Some(Need::Unread),
            // A session that's done, one you've read, waits for you without needing you.
            _ => None,
        }
    }

    fn refresh_need(&mut self, now: SystemTime) {
        self.need = match (self.current_need(), self.need) {
            (Some(need), Some((was, since))) if need == was => Some((was, since)),
            (Some(need), _) => Some((need, now)),
            (None, _) => None,
        };
    }
}

/// A tile taken off the rack, with its place among the tiles of its own group, session tiles or
/// your own, so an Undo puts it back in its group even after the other group changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed {
    pub tile: Tile,
    pub index_in_group: usize,
    /// The sub-sessions dismissed with a session, in their order under it.
    pub family: Vec<Tile>,
}

impl Removed {
    /// The list keys of the ended sessions among the removed tiles, whose tiles go for good once
    /// no Undo can bring them back.
    pub fn ended_lists(&self) -> Vec<String> {
        std::iter::once(&self.tile)
            .chain(&self.family)
            .filter_map(Tile::session)
            .filter(|s| s.state == SessionState::Ended)
            .map(|s| s.list_key().to_string())
            .collect()
    }
}

/// How many of your own removed tiles the archive keeps, dropping the oldest past it.
pub const ARCHIVE_KEEPS: usize = 50;

/// Where one of your own tiles is, as `store` saves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnPlace {
    Shown,
    Snoozed,
    /// Removed, at that time, and kept in the archive.
    Archived(SystemTime),
}

/// One of your own tiles you removed, kept for the archive, and when you removed it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Archived {
    tile: Tile,
    at: SystemTime,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rack {
    tiles: Vec<Tile>,
    /// The tiles of sessions you dismissed, kept up to date off the rack until one needs you
    /// again or you send it a new prompt, and the tiles you snoozed, which wait for their time.
    hidden: Vec<Tile>,
    /// Your own tiles you removed, newest first, until you bring one back or clear the archive.
    /// The sessions you dismissed are in the archive too, from `hidden`.
    archive: Vec<Archived>,
    next_id: u64,
    /// The list keys of sessions whose tiles went for good since the app last took them, so the
    /// app deletes their task lists.
    gone: Vec<String>,
}

/// A dismissed session that leaves `hidden` for the rack leaves the archive with it.
fn returned(tile: &mut Tile) {
    if let Body::Session(s) = &mut tile.body {
        s.dismissed_at = None;
    }
}

fn holds(tile: &Tile, session_id: &str) -> bool {
    tile.session().is_some_and(|s| s.id == session_id)
}

/// Where a session's tile is.
enum Place {
    Shown(usize),
    Hidden(usize),
}

impl Rack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// The list keys of sessions whose tiles went for good since the last call: an ended tile the
    /// rack cleared, and a dismissed or snoozed session that ended. An ended tile you remove
    /// yourself isn't among them, since an Undo can bring it back, so the app takes it from the
    /// `Removed` once the Undo is gone (`Removed::ended_lists`).
    pub fn take_gone(&mut self) -> Vec<String> {
        std::mem::take(&mut self.gone)
    }

    pub fn tile(&self, id: TileId) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.id == id)
    }

    fn tile_mut(&mut self, id: TileId) -> Option<&mut Tile> {
        self.tiles.iter_mut().find(|t| t.id == id)
    }

    pub fn needs_you(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.iter().filter(|t| t.need.is_some())
    }

    fn new_id(&mut self) -> TileId {
        self.next_id += 1;
        TileId(self.next_id)
    }

    /// Applies one of a session's hook events, adding the session's tile if it isn't on the rack,
    /// and returns the tile's id.
    pub fn apply(
        &mut self,
        session_id: &str,
        folder: &Path,
        event: SessionEvent,
        now: SystemTime,
    ) -> TileId {
        let place = match self.place_of(session_id) {
            Some(place) => place,
            None => Place::Shown(self.add_session(session_id, folder, &event, now)),
        };
        // A prompt you typed brings a dismissed session back, and a task notification's doesn't,
        // so a dismissed session that waits on a background command stays off the rack when the
        // command exits and the turn waits again.
        let new_prompt = event.is_typed_prompt();
        let tile = match place {
            Place::Shown(i) => &mut self.tiles[i],
            Place::Hidden(i) => &mut self.hidden[i],
        };
        let Body::Session(s) = &mut tile.body else {
            unreachable!("the tile found by its session id holds that session");
        };
        let was = s.state;
        s.folder = folder.to_path_buf();

        // Puts the tile in `to` from now, with the timer that state starts with.
        let enter = |s: &mut Session, timer: &mut Timer, to: SessionState| {
            s.state = to;
            s.state_since = now;
            s.read = false;
            s.own_timer = false;
            // A reminder for tomorrow is for when you're back, so the session's turns leave it.
            if timer.is_tomorrow() {
                return;
            }
            *timer = match to {
                SessionState::YourTurn | SessionState::NeedsApproval => {
                    Timer::start(now, DEFAULT_MINUTES)
                }
                SessionState::Working
                | SessionState::Unread
                | SessionState::Done
                | SessionState::Waiting
                | SessionState::Ended => Timer::Idle,
                SessionState::New => *timer,
            };
        };
        let set_state = |s: &mut Session, timer: &mut Timer, to: SessionState| {
            if s.state != to {
                enter(s, timer, to);
            }
        };
        // Whether a turn's end moved the tile, even into the state it was in.
        let mut turned = false;

        // A waiting tile waits until its session's next turn begins. A turn that a background
        // command's exit starts fired UserPromptSubmit on macOS, and whether it does on Windows is
        // unmeasured, so the turn's first tool, question, or `/compact` ends the wait too
        // (record/areas/tasks/02-waits-and-pings.md, "Left open").
        let starts_turn = matches!(
            event,
            SessionEvent::ToolPending { .. }
                | SessionEvent::WrappedUp(_)
                | SessionEvent::PlanChanged(_)
                | SessionEvent::WaitCalled { .. }
                | SessionEvent::PingCalled { .. }
                | SessionEvent::InputAsked { .. }
                | SessionEvent::CompactStarted { manual: true }
        );
        if starts_turn && s.state == SessionState::Waiting {
            begin_turn(s);
            set_state(s, &mut tile.timer, SessionState::Working);
        }

        match event {
            SessionEvent::Started { title, .. } => {
                if title.is_some() {
                    s.custom_title = title;
                }
                if s.state == SessionState::Ended {
                    s.ended_at = None;
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::New);
                }
            }
            SessionEvent::PromptSubmitted { title, prompt } => {
                if title.is_some() {
                    s.custom_title = title;
                }
                if s.first_prompt.is_none() {
                    s.first_prompt = prompt.filter(|p| !prompt_title(p).is_empty());
                }
                begin_turn(s);
                set_state(s, &mut tile.timer, SessionState::Working);
            }
            SessionEvent::WrappedUp(wrap_up) => {
                s.asks = Some(wrap_up.asks());
                s.wrap_up = Some(wrap_up);
            }
            // Claude calls the app's tool only while it works, so like any tool, a plan's change
            // ends a compaction whose PostCompact never came.
            SessionEvent::PlanChanged(change) => {
                change.apply(&mut s.plan);
                s.compacting = false;
            }
            SessionEvent::WaitCalled { on } => s.wait = Some(on),
            SessionEvent::PingCalled { message } => s.ping = Some(message),
            SessionEvent::ToolPending { tool } => {
                let activity = tool.activity();
                if s.recent.last().map(|r| &r.text) != Some(&activity) {
                    if s.recent.len() == RECENT_TOOLS {
                        s.recent.remove(0);
                    }
                    s.recent.push(Recent {
                        text: activity.clone(),
                        failed: false,
                    });
                }
                s.activity = Some(activity);
                s.activity_at = now;
                s.tool_running = true;
                s.pending_tool = Some(tool);
                // Claude calls no tool while Claude Code compacts, so a compaction whose
                // PostCompact never came is over (unverified: the hooks of a turn whose context
                // fills, kept in order, would settle it, and a background agent's tool would at
                // worst end the line early).
                s.compacting = false;
            }
            SessionEvent::ApprovalAsked { tool } => {
                if tool.is_some() {
                    s.pending_tool = tool;
                }
                if s.state != SessionState::NeedsApproval {
                    s.summary = s.pending_tool.as_ref().map(|t| t.summary());
                }
                s.question = false;
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            // The notification also comes about 6s into a question, which it leaves a question.
            SessionEvent::ApprovalNotified { message } => {
                if s.state != SessionState::NeedsApproval {
                    s.summary = s.pending_tool.as_ref().map(|t| t.summary());
                    s.question = false;
                }
                if s.summary.is_none() {
                    s.summary = Some(first_line(&message));
                }
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            SessionEvent::ToolFinished { activity, failed } => {
                // Tools can run side by side, so a failure marks the newest of the turn's tools
                // that has the failed tool's words.
                if failed
                    && let Some(r) = s
                        .recent
                        .iter_mut()
                        .rev()
                        .find(|r| activity.as_ref().is_none_or(|a| *a == r.text))
                {
                    r.failed = true;
                }
                s.pending_tool = None;
                s.tool_running = false;
                if s.state == SessionState::NeedsApproval {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
            }
            // A turn that pinged is your turn, with the ping's message, since you requested the
            // ping. A turn whose wrap-up asked you something is your turn too. Otherwise a turn
            // that left background agents running stays working, since Claude takes it up again
            // when they report back, a turn that called `wait` waits, and a turn whose wrap-up
            // asked you nothing is unread until you look at it. Any other turn's end is your
            // turn, so a session with no wrap-up still asks for you as it always has (user,
            // 2026-11-15 and 2026-11-20).
            SessionEvent::TurnEnded {
                last_message,
                background,
            } => {
                s.pending_tool = None;
                // A compaction in the turn whose PostCompact never came ends with the turn, even
                // when background agents keep the tile working.
                s.compacting = false;
                s.message = last_message
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty());
                s.summary = s.message.as_deref().map(first_line);
                s.background = background;
                let ping = s.ping.take();
                let mut wait = s.wait.take();
                // A turn that calls `wait` keeps it past its end while a background command it
                // left running still runs, so a later turn that ends without calling `wait`
                // again, as after Claude asked you something and you answered, still waits on it
                // (user, 2026-11-27; record/areas/tasks/02-waits-and-pings.md).
                let commands: Vec<String> = s
                    .background
                    .iter()
                    .filter(|t| t.kind == "shell" && !t.id.is_empty())
                    .map(|t| t.id.clone())
                    .collect();
                match &wait {
                    Some(on) => {
                        s.carried_wait = (!commands.is_empty()).then(|| CarriedWait {
                            on: on.clone(),
                            commands,
                        });
                    }
                    None => match &s.carried_wait {
                        Some(carried) if carried.commands.iter().any(|c| commands.contains(c)) => {
                            wait = Some(carried.on.clone());
                        }
                        _ => s.carried_wait = None,
                    },
                }
                let to = match (&ping, s.asks, &wait) {
                    (Some(_), _, _) | (None, Some(true), _) => SessionState::YourTurn,
                    _ if s.waiting_on() > 0 => SessionState::Working,
                    (None, _, Some(_)) => SessionState::Waiting,
                    (None, Some(false), None) => SessionState::Unread,
                    (None, None, None) => SessionState::YourTurn,
                };
                if let Some(message) = ping {
                    s.summary = Some(message);
                    s.pinged_at = Some(now);
                } else if let Some(on) = wait.filter(|_| to == SessionState::Waiting) {
                    s.summary = Some(on);
                }
                // A turn's end starts its state over, even the state the tile is in, with a new
                // timer and a new need: a turn that begins with no UserPromptSubmit, such as one a
                // background command's exit starts, finds the tile where the turn before it
                // ended, and its own end asks for you again. A tile kept working stays as it is.
                if to == SessionState::Working {
                    set_state(s, &mut tile.timer, to);
                } else {
                    enter(s, &mut tile.timer, to);
                    tile.need = None;
                    turned = true;
                }
            }
            // Only a look after the turn ended reads it: one from before is the look the app
            // last recorded, while the turn ran. Either way, the look is the latest visit.
            SessionEvent::Seen { at } if at >= s.state_since => {
                s.seen_at = Some(s.seen_at.map_or(at, |old| old.max(at)));
                match s.state {
                    SessionState::Unread => set_state(s, &mut tile.timer, SessionState::Done),
                    SessionState::YourTurn => {
                        look(s, &mut tile.timer);
                    }
                    _ => {}
                }
            }
            SessionEvent::Seen { at } => {
                s.seen_at = Some(s.seen_at.map_or(at, |old| old.max(at)));
            }
            // hooks::apply handles these with `follow` and `set_app_session`, and never passes
            // them on.
            SessionEvent::Continued { .. } | SessionEvent::AppChanged => {}
            SessionEvent::TurnFailed { error, text } => {
                s.wait = None;
                s.ping = None;
                s.pending_tool = None;
                s.background.clear();
                s.message = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
                s.summary = Some(s.message.as_deref().map(first_line).unwrap_or(error));
                set_state(s, &mut tile.timer, SessionState::YourTurn);
            }
            SessionEvent::Ended => {
                s.wait = None;
                s.ping = None;
                s.pending_tool = None;
                s.background.clear();
                s.summary = None;
                s.ended_at = Some(now);
                set_state(s, &mut tile.timer, SessionState::Ended);
            }
            // A /compact you typed is work you wait on, so it moves the tile to working. Claude
            // Code also fired a PreCompact in a session that sat idle, with no compaction and no
            // PostCompact after it, and we take it to have been one Claude Code started on its
            // own (unverified: Plates keeps no hook payloads, and keeping one from an idle session
            // would settle it; record/areas/hooks/log.md, 2026-11-20). So a compaction Claude
            // Code starts marks only a turn that's running, and a tile that waits on you, or one
            // kept working only for background agents, keeps saying so.
            SessionEvent::CompactStarted { manual } => {
                if s.state != SessionState::Working && manual {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
                if s.state == SessionState::Working && (manual || s.waiting_on() == 0) {
                    s.compacting = true;
                }
            }
            // A /compact you typed hands the turn back once it's done, and the Claude desktop
            // app then shows "Compacted session" (user, 2026-11-15). One Claude Code started on
            // its own runs mid-turn, and the turn goes on.
            SessionEvent::CompactFinished { manual } => {
                s.compacting = false;
                if manual {
                    s.summary = Some(COMPACTED.to_string());
                    set_state(s, &mut tile.timer, SessionState::YourTurn);
                }
            }
            SessionEvent::InputAsked { summary } => {
                s.summary = Some(summary);
                s.pending_tool = None;
                s.question = true;
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            SessionEvent::InputGiven => {
                if s.state == SessionState::NeedsApproval {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
            }
            SessionEvent::Interrupted { denied } => {
                s.wait = None;
                s.ping = None;
                s.background.clear();
                s.summary = Some(match (denied, s.pending_tool.take()) {
                    (true, Some(tool)) => format!("Denied: {}", tool.subject()),
                    (true, None) => "Denied".to_string(),
                    (false, _) => "Interrupted".to_string(),
                });
                set_state(s, &mut tile.timer, SessionState::YourTurn);
            }
        }
        // A tile that stops working, as when the session ends, ends a compaction whose
        // PostCompact never came, so it doesn't say "Compacting session…" the next time it works.
        if s.state != SessionState::Working {
            s.compacting = false;
        }
        s.refresh_line(now);
        let id = tile.id;
        let state = s.state;
        let needs_you = (state != was || turned)
            && matches!(
                state,
                SessionState::NeedsApproval | SessionState::YourTurn | SessionState::Unread
            );
        match place {
            Place::Shown(_) => tile.refresh_need(now),
            // A dismissed session comes back where a new session's tile would go, once it needs
            // you or you send it a new prompt, and a session that ends while dismissed goes for
            // good (user, 2026-11-15). A snoozed one waits for its time whatever it does, since
            // you snoozed it as one you can't act on yet (user, 2026-11-16).
            Place::Hidden(i) if (new_prompt || needs_you) && !snoozed(&self.hidden[i]) => {
                let mut tile = self.hidden.remove(i);
                returned(&mut tile);
                tile.refresh_need(now);
                let parent = tile.session().and_then(|s| s.app_parent.clone());
                let index = self.session_slot(parent.as_deref());
                self.tiles.insert(index, tile);
            }
            Place::Hidden(i) if state == SessionState::Ended => {
                let tile = self.hidden.remove(i);
                self.gone
                    .extend(tile.session().map(|s| s.list_key().to_string()));
            }
            Place::Hidden(_) => {}
        }
        id
    }

    // A new session's tile goes after the last session tile, so no session tile above it moves,
    // in the state the event moves a tile to, or working for an event that moves none.
    fn add_session(
        &mut self,
        session_id: &str,
        folder: &Path,
        event: &SessionEvent,
        now: SystemTime,
    ) -> usize {
        let mut session = Session::new(session_id, folder, now);
        if !matches!(event, SessionEvent::Started { .. }) {
            session.state = SessionState::Working;
        }
        let tile = Tile {
            id: self.new_id(),
            body: Body::Session(session),
            timer: Timer::Idle,
            need: None,
        };
        let index = self
            .tiles
            .iter()
            .rposition(|t| t.session().is_some())
            .map_or(0, |i| i + 1);
        self.tiles.insert(index, tile);
        index
    }

    /// Whether the app rings its tone for something after `after` and by `by`: a timer that ran
    /// out, or a ping.
    pub fn rings_between(&self, after: SystemTime, by: SystemTime) -> bool {
        self.ran_out_between(after, by) || self.pinged_between(after, by)
    }

    /// Whether a session on the rack pinged after `after` and by `by`, and still shows the
    /// ping, at your turn with the turn unread, which the app rings its tone for at once. A tone
    /// held while you were busy rings only for a ping still showing, as one for a timer rings
    /// only for a tile still time-up (record/areas/tasks/02-waits-and-pings.md). The ping's turn
    /// is the one the tile shows while the tile has been at your turn since the ping.
    pub fn pinged_between(&self, after: SystemTime, by: SystemTime) -> bool {
        self.tiles.iter().any(|t| {
            t.session()
                .filter(|s| {
                    s.state == SessionState::YourTurn
                        && !s.read
                        && s.pinged_at == Some(s.state_since)
                })
                .and_then(|s| s.pinged_at)
                .is_some_and(|at| after < at && at <= by)
        })
    }

    /// Whether a timer on the rack ran out after `after` and by `by`, which the app rings a tone
    /// for. A snoozed tile that came back counts, since its timer ran out as it came back.
    pub fn ran_out_between(&self, after: SystemTime, by: SystemTime) -> bool {
        self.tiles.iter().any(|t| match t.timer {
            Timer::TimeUp { since } => after < since && since <= by,
            _ => false,
        })
    }

    /// Turns timers that have run out into time-up, and clears ended sessions' tiles 30 minutes
    /// after they ended. It returns whether anything changed.
    ///
    /// It refreshes every tile's need, so a time-up timer loaded from `rack.json`, which no tick
    /// changes, needs you from the first tick.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let before = self.tiles.len();
        // An ended tile with a reminder for tomorrow, or one that's run out and you haven't
        // clicked, stays until you have (user, 2026-11-16).
        let stays = |t: &Tile| {
            t.timer.is_tomorrow()
                || t.timer.is_time_up()
                || t.session()
                    .and_then(|s| s.ended_at)
                    .is_none_or(|ended| now < ended + ENDED_CLEARS_AFTER)
        };
        let cleared = self.tiles.iter().filter(|t| !stays(t));
        self.gone.extend(
            cleared
                .filter_map(|t| t.session())
                .map(|s| s.list_key().to_string()),
        );
        self.tiles.retain(stays);
        let mut changed = self.tiles.len() != before;
        // A snoozed tile, or a dismissed session with a reminder for tomorrow, comes back when
        // its time runs out, time-up, as one that needs you does.
        let mut i = 0;
        while i < self.hidden.len() {
            let timer = self.hidden[i].timer.at(now);
            if timer.is_time_up() {
                let mut tile = self.hidden.remove(i);
                returned(&mut tile);
                tile.timer = timer;
                tile.refresh_need(now);
                self.show(tile);
                changed = true;
            } else {
                i += 1;
            }
        }
        for tile in &mut self.tiles {
            let (timer, need) = (tile.timer, tile.need);
            tile.timer = tile.timer.at(now);
            tile.refresh_need(now);
            changed |= tile.timer != timer || tile.need != need;
            if let Body::Session(s) = &mut tile.body {
                changed |= s.refresh_line(now);
            }
        }
        changed
    }

    pub fn add_own(&mut self, title: &str, now: SystemTime) -> TileId {
        let id = self.new_id();
        self.tiles.push(Tile {
            id,
            body: Body::Own(Own {
                title: title.trim().to_string(),
            }),
            timer: Timer::Idle,
            need: None,
        });
        if let Some(t) = self.tile_mut(id) {
            t.refresh_need(now);
        }
        id
    }

    /// Names a session from its first prompt, read from its transcript, when Plates first saw the
    /// session after that prompt.
    /// Sets a session's first prompt over any prompt its first event carried, since the event
    /// that brought a session's tile can come long after the session's first prompt.
    pub fn set_first_prompt(&mut self, session_id: &str, prompt: &str) {
        if prompt_title(prompt).is_empty() {
            return;
        }
        if let Some(s) = self.session_mut(session_id) {
            s.first_prompt = Some(prompt.to_string());
        }
    }

    /// Sets a session's status line, the first line of the newest text Claude wrote in the turn,
    /// which a working tile shows.
    pub fn set_status(&mut self, session_id: &str, text: &str, now: SystemTime) {
        let line = first_line(text);
        if line.is_empty() {
            return;
        }
        if let Some(s) = self.session_mut(session_id)
            && s.status.as_deref() != Some(line.as_str())
        {
            s.status = Some(line);
            s.status_at = now;
            s.refresh_line(now);
        }
    }

    /// Sets where a session's transcript is.
    /// Gives a session the app's plan read back from its transcript.
    pub fn set_plan(&mut self, session_id: &str, plan: Vec<Step>) {
        if let Some(s) = self.session_mut(session_id) {
            s.plan = plan;
        }
    }

    pub fn set_transcript(&mut self, session_id: &str, path: &Path) {
        if let Some(s) = self.session_mut(session_id) {
            s.transcript = Some(path.to_path_buf());
        }
    }

    /// A session's tile, shown or dismissed, and the session it holds.
    pub fn session(&self, session_id: &str) -> Option<(TileId, &Session)> {
        self.tiles
            .iter()
            .chain(&self.hidden)
            .find_map(|t| match &t.body {
                Body::Session(s) if s.id == session_id => Some((t.id, s)),
                _ => None,
            })
    }

    fn session_mut(&mut self, session_id: &str) -> Option<&mut Session> {
        self.tiles
            .iter_mut()
            .chain(self.hidden.iter_mut())
            .find_map(|t| match &mut t.body {
                Body::Session(s) if s.id == session_id => Some(s),
                _ => None,
            })
    }

    fn place_of(&self, session_id: &str) -> Option<Place> {
        let shown = self.tiles.iter().position(|t| holds(t, session_id));
        let hidden = || self.hidden.iter().position(|t| holds(t, session_id));
        shown
            .map(Place::Shown)
            .or_else(|| hidden().map(Place::Hidden))
    }

    /// Has a session's tile follow the Claude desktop app's session from the Claude Code session
    /// `from` to `to`, the one the app moved it to, so the tile stops showing where `from` was
    /// left. It returns whether `from` had a tile.
    ///
    /// When `to` already has a tile, from events that reached Plates before the app's file named
    /// `to`, the tile of `from` takes that tile's session and that tile goes. So what you set on
    /// the tile of `from` stays: its place, whether it's dismissed or snoozed, a reminder for
    /// tomorrow, and the app's title, ID, and parent while the newer tile has none yet. A dismissed
    /// tile that isn't snoozed goes instead, since the newer tile's events have brought the session
    /// back to the rack, as they would have brought the dismissed tile back.
    pub fn follow(
        &mut self,
        from: &str,
        to: &str,
        transcript: Option<&Path>,
        now: SystemTime,
    ) -> bool {
        if from == to {
            return false;
        }
        let Some(old) = self.place_of(from) else {
            return false;
        };
        let Some(newer) = self.place_of(to) else {
            let Some(s) = self.session_mut(from) else {
                return false;
            };
            s.id = to.to_string();
            if let Some(path) = transcript {
                s.transcript = Some(path.to_path_buf());
            }
            return true;
        };
        if let (Place::Hidden(i), Place::Shown(_)) = (&old, &newer)
            && !snoozed(&self.hidden[*i])
        {
            self.hidden.remove(*i);
            return true;
        }
        let newer = match newer {
            Place::Shown(j) => self.tiles.remove(j),
            Place::Hidden(j) => self.hidden.remove(j),
        };
        let Some(old) = self.place_of(from) else {
            return false;
        };
        let (tile, shown) = match old {
            Place::Shown(i) => (&mut self.tiles[i], true),
            Place::Hidden(i) => (&mut self.hidden[i], false),
        };
        let (Body::Session(mut s), Body::Session(was)) = (newer.body, &tile.body) else {
            unreachable!("both tiles were found by their session IDs");
        };
        s.app_title = s.app_title.or_else(|| was.app_title.clone());
        s.app_id = s.app_id.or_else(|| was.app_id.clone());
        s.app_parent = s.app_parent.or_else(|| was.app_parent.clone());
        s.custom_title = s.custom_title.or_else(|| was.custom_title.clone());
        s.first_prompt = was.first_prompt.clone().or(s.first_prompt);
        s.dismissed_at = was.dismissed_at;
        if s.transcript.is_none() {
            s.transcript = transcript.map(Path::to_path_buf);
        }
        tile.body = Body::Session(s);
        if !tile.timer.is_tomorrow() {
            tile.timer = newer.timer;
        }
        if shown {
            tile.refresh_need(now);
        }
        true
    }

    /// Every session tile, shown or dismissed, with whether it's dismissed, for saving.
    pub fn session_tiles(&self) -> impl Iterator<Item = (&Tile, bool)> {
        let shown = self.tiles.iter().filter(|t| !t.is_own());
        shown
            .map(|t| (t, false))
            .chain(self.hidden.iter().map(|t| (t, true)))
    }

    /// Puts back a session tile saved before Plates last closed, after the other session tiles,
    /// or among the dismissed.
    pub fn restore_session(
        &mut self,
        session: Session,
        timer: Timer,
        dismissed: bool,
        now: SystemTime,
    ) -> TileId {
        let id = self.new_id();
        let mut tile = Tile {
            id,
            body: Body::Session(session),
            timer,
            need: None,
        };
        if dismissed {
            self.hidden.push(tile);
        } else {
            tile.refresh_need(now);
            let index = self
                .tiles
                .iter()
                .rposition(|t| t.session().is_some())
                .map_or(0, |i| i + 1);
            self.tiles.insert(index, tile);
        }
        id
    }

    // Where a session tile goes: after its parent's family, the parent's tile and the
    // sub-sessions under it, when the parent's tile is shown, and otherwise after the last
    // session tile, so no session tile above it moves (user, 2026-11-15).
    fn session_slot(&self, parent: Option<&str>) -> usize {
        parent
            .and_then(|p| self.after_family(p))
            .unwrap_or_else(|| {
                self.tiles
                    .iter()
                    .rposition(|t| t.session().is_some())
                    .map_or(0, |i| i + 1)
            })
    }

    // The index just past the family of the shown session whose app ID is `parent`.
    fn after_family(&self, parent: &str) -> Option<usize> {
        let start = self
            .tiles
            .iter()
            .position(|t| t.session().and_then(|s| s.app_id.as_deref()) == Some(parent))?;
        Some(self.family_end(start))
    }

    // The index just past the family of the tile at `start`: its tile, and the tiles right after
    // it whose parent is in the family, such as a sub-session's own sub-sessions. These are the
    // tiles the rack draws nested under it.
    fn family_end(&self, start: usize) -> usize {
        let Some(head) = self.tiles[start]
            .session()
            .and_then(|s| s.app_id.as_deref())
        else {
            return start + 1;
        };
        let mut family = vec![head];
        let mut end = start + 1;
        while let Some(s) = self.tiles.get(end).and_then(|t| t.session()) {
            match s.app_parent.as_deref() {
                Some(p) if family.contains(&p) => {
                    family.extend(s.app_id.as_deref());
                    end += 1;
                }
                _ => break,
            }
        }
        end
    }

    /// The sub-sessions nested under a shown session's tile, which dismissing it takes along.
    pub fn family(&self, id: TileId) -> Vec<TileId> {
        let Some(start) = self.tiles.iter().position(|t| t.id == id) else {
            return Vec::new();
        };
        let end = self.family_end(start);
        self.tiles[start + 1..end].iter().map(|t| t.id).collect()
    }

    /// Moves a shown sub-session's tile to just after its parent's family. The app calls it for
    /// a tile the event it's handling added, which nobody has seen yet, and once more when the
    /// app's file first names the tile's parent, a few seconds into a new sub-session, which moves
    /// the tile once.
    pub fn place_under_parent(&mut self, session_id: &str) {
        let Some(i) = self
            .tiles
            .iter()
            .position(|t| t.session().is_some_and(|s| s.id == session_id))
        else {
            return;
        };
        let Some(parent) = self.tiles[i].session().and_then(|s| s.app_parent.clone()) else {
            return;
        };
        let tile = self.tiles.remove(i);
        let index = self.after_family(&parent).unwrap_or(i);
        self.tiles.insert(index, tile);
    }

    // Moves the shown tile of a session the app detached from `parent`, with its own sub-sessions,
    // from the parent's family to just past it, so the parent's other sub-sessions stay under the
    // parent, as the app's sidebar shows a detached session on its own (user, 2026-11-16). A tile
    // that sat anywhere else stays there. The tile's parent is already cleared, so the family's
    // run stops at the tile only if the tile sat in it.
    fn leave_family(&mut self, session_id: &str, parent: &str) {
        let Some(start) = self.tiles.iter().position(|t| holds(t, session_id)) else {
            return;
        };
        if self.after_family(parent) != Some(start) {
            return;
        }
        let end = self.family_end(start);
        let block: Vec<Tile> = self.tiles.drain(start..end).collect();
        let at = self.after_family(parent).unwrap_or(start);
        self.tiles.splice(at..at, block);
    }

    /// Moves each restored sub-session's tile under its parent's, in the order they were saved,
    /// before the rack first shows them.
    pub fn gather_families(&mut self) {
        let children: Vec<String> = self
            .tiles
            .iter()
            .filter_map(|t| t.session())
            .filter(|s| s.app_parent.is_some())
            .map(|s| s.id.clone())
            .collect();
        for id in children {
            self.place_under_parent(&id);
        }
    }

    /// Sets the title and ID the Claude desktop app keeps for a session, from its file for it,
    /// and the app ID of the session it was started from, or none for a session the app shows on
    /// its own, such as one detached from its parent, whose tile then leaves the parent's family.
    /// It returns whether the session has a new parent, which moves the tile under the parent's.
    pub fn set_app_session(&mut self, session_id: &str, app: &AppSession) -> bool {
        let Some(s) = self.session_mut(session_id) else {
            return false;
        };
        if app.title.is_some() {
            s.app_title = app.title.clone();
        }
        if app.id.is_some() {
            s.app_id = app.id.clone();
        }
        s.pulls = app.pulls.clone();
        if s.app_parent == app.parent {
            return false;
        }
        let was = std::mem::replace(&mut s.app_parent, app.parent.clone());
        if let (None, Some(was)) = (&app.parent, was) {
            self.leave_family(session_id, &was);
        }
        app.parent.is_some()
    }

    /// Takes the tile of a session the Claude desktop app archived off the rack for good, from
    /// among the shown, the dismissed, and the snoozed, and returns it. Its sub-sessions stay, each until the app
    /// archives it too.
    pub fn archived_in_app(&mut self, session_id: &str) -> Option<TileId> {
        let of = |t: &Tile| t.session().is_some_and(|s| s.id == session_id);
        let tile = match self.tiles.iter().position(of) {
            Some(i) => self.tiles.remove(i),
            None => self.hidden.remove(self.hidden.iter().position(of)?),
        };
        self.gone
            .extend(tile.session().map(|s| s.list_key().to_string()));
        Some(tile.id)
    }

    /// Whether the rack shows the session, or holds it dismissed.
    pub fn knows(&self, session_id: &str) -> bool {
        self.session(session_id).is_some()
    }

    /// Reads an unread session's tile, as clicking it does. It returns whether the tile changed.
    pub fn mark_read(&mut self, id: TileId, now: SystemTime) -> bool {
        let Some(tile) = self.tile_mut(id) else {
            return false;
        };
        let Body::Session(s) = &mut tile.body else {
            return false;
        };
        match s.state {
            SessionState::Unread => {
                s.state = SessionState::Done;
                s.state_since = now;
            }
            // Your turn stays your turn, since it still waits for your reply.
            SessionState::YourTurn => {
                if !look(s, &mut tile.timer) {
                    return false;
                }
            }
            _ => return false,
        }
        tile.refresh_need(now);
        true
    }

    /// Marks a finished session's turn unread again, so it needs you as it did before you read
    /// it, without its timer (user, 2026-11-16). A done tile goes back to unread, and a your-turn
    /// tile you'd read goes back to unread. It keeps its `state_since`, so the tile's age still
    /// counts from when the turn ended, or from when you read it. It returns whether the tile
    /// changed.
    pub fn mark_unread(&mut self, id: TileId, now: SystemTime) -> bool {
        let Some(tile) = self.tile_mut(id) else {
            return false;
        };
        let Body::Session(s) = &mut tile.body else {
            return false;
        };
        match s.state {
            SessionState::Done => s.state = SessionState::Unread,
            SessionState::YourTurn if s.read => s.read = false,
            _ => return false,
        }
        tile.refresh_need(now);
        true
    }

    /// Puts back your own tiles, in order, as `store` loaded them, with where each was.
    pub fn with_own_tiles(tiles: impl IntoIterator<Item = (String, Timer, OwnPlace)>) -> Self {
        let mut rack = Rack::new();
        for (title, timer, place) in tiles {
            let id = rack.new_id();
            let tile = Tile {
                id,
                body: Body::Own(Own { title }),
                timer,
                need: None,
            };
            match place {
                OwnPlace::Shown => rack.tiles.push(tile),
                OwnPlace::Snoozed => rack.hidden.push(tile),
                OwnPlace::Archived(at) => rack.archive.push(Archived { tile, at }),
            }
        }
        rack.archive.sort_by_key(|a| std::cmp::Reverse(a.at));
        rack
    }

    pub fn rename_own(&mut self, id: TileId, title: &str) -> bool {
        let title = title.trim();
        match self.tile_mut(id).map(|t| &mut t.body) {
            Some(Body::Own(own)) if !title.is_empty() => {
                own.title = title.to_string();
                true
            }
            _ => false,
        }
    }

    /// Takes a tile off the rack, for an Undo to put back. Your own tile and an ended session's
    /// go, and a session that hasn't ended is dismissed: it's read, its timer stops, and it waits
    /// off the rack until it needs you or you send it a new prompt. A session's sub-sessions,
    /// nested under its tile, go with it (user, 2026-11-17).
    pub fn remove(&mut self, id: TileId, now: SystemTime) -> Option<Removed> {
        let index = self.tiles.iter().position(|t| t.id == id)?;
        let own = self.tiles[index].is_own();
        let index_in_group = self.tiles[..index]
            .iter()
            .filter(|t| t.is_own() == own)
            .count();
        let end = self.family_end(index);
        let family: Vec<Tile> = self.tiles.drain(index + 1..end).collect();
        let mut tile = self.tiles.remove(index);
        self.put_away(&mut tile, now);
        let family = family
            .into_iter()
            .map(|mut t| {
                self.put_away(&mut t, now);
                t
            })
            .collect();
        Some(Removed {
            tile,
            index_in_group,
            family,
        })
    }

    // Puts a tile taken off the rack where it waits: a session that hasn't ended among the
    // dismissed, and your own tile in the archive.
    fn put_away(&mut self, tile: &mut Tile, now: SystemTime) {
        match &mut tile.body {
            Body::Session(s) if s.state != SessionState::Ended => {
                if s.state == SessionState::Unread {
                    s.state = SessionState::Done;
                }
                s.dismissed_at = Some(now);
                tile.timer.stop();
                tile.need = None;
                self.hidden.push(tile.clone());
            }
            // An ended session's tile has nothing left to bring back, so it goes for good.
            Body::Session(_) => {}
            // Your own tile waits in the archive without its timer, while an Undo puts it back
            // with the timer as it was.
            Body::Own(_) => {
                let mut kept = tile.clone();
                kept.timer = Timer::Idle;
                kept.need = None;
                self.archive.insert(
                    0,
                    Archived {
                        tile: kept,
                        at: now,
                    },
                );
                self.archive.truncate(ARCHIVE_KEEPS);
            }
        }
    }

    // Session tiles always come before your own, so a group's tiles sit together, and a
    // session's sub-sessions come back under it.
    pub fn restore(&mut self, removed: Removed) {
        let Removed {
            tile,
            index_in_group,
            family,
        } = removed;
        let own = tile.is_own();
        let sessions = self.tiles.iter().filter(|t| !t.is_own()).count();
        let (start, len) = if own {
            (sessions, self.tiles.len() - sessions)
        } else {
            (0, sessions)
        };
        let mut index = Some(start + index_in_group.min(len));
        for (k, tile) in std::iter::once(tile).chain(family).enumerate() {
            match (self.take_back(tile), index) {
                (Some(tile), Some(i)) => {
                    self.tiles.insert(i, tile);
                    index = Some(i + 1);
                }
                // A sub-session whose parent didn't come back goes where it would come back on
                // its own.
                (Some(tile), None) => self.show(tile),
                (None, _) if k == 0 => index = None,
                (None, _) => {}
            }
        }
    }

    // Takes a removed tile back for an Undo. A dismissed session comes back as it stands now,
    // since its events kept it up to date, and not at all if it came back or ended meanwhile.
    fn take_back(&mut self, removed: Tile) -> Option<Tile> {
        let mut tile = match removed.session() {
            Some(s) if s.state != SessionState::Ended => {
                let i = self.hidden.iter().position(|t| t.id == removed.id)?;
                self.hidden.remove(i)
            }
            _ => removed,
        };
        // An Undo takes the tile back out of the archive it went into.
        self.archive.retain(|a| a.tile.id != tile.id);
        returned(&mut tile);
        Some(tile)
    }

    /// Takes a tile off the rack until `until`, a reminder for later, runs out, or until you wake
    /// it from the list of snoozed tiles. It returns whether a tile was snoozed.
    pub fn snooze(&mut self, id: TileId, until: Timer, now: SystemTime) -> bool {
        let Some(index) = self.tiles.iter().position(|t| t.id == id) else {
            return false;
        };
        let mut tile = self.tiles.remove(index);
        tile.timer = until.at(now);
        tile.need = None;
        take_in_hand(&mut tile);
        self.hidden.push(tile);
        true
    }

    /// Brings a snoozed tile back before its time, as it stands now, with no timer. It returns
    /// whether a tile came back.
    pub fn wake(&mut self, id: TileId, now: SystemTime) -> bool {
        let Some(index) = self.hidden.iter().position(|t| t.id == id && snoozed(t)) else {
            return false;
        };
        let mut tile = self.hidden.remove(index);
        tile.timer = Timer::Idle;
        tile.refresh_need(now);
        self.show(tile);
        true
    }

    /// What you removed, newest first, with when: your own tiles, and the sessions you dismissed
    /// that haven't come back on their own. A dismissed sub-session whose parent is dismissed
    /// too isn't listed, since it comes back with its parent.
    pub fn archived(&self) -> Vec<(&Tile, SystemTime)> {
        let parents: Vec<&str> = self
            .hidden
            .iter()
            .filter_map(Tile::session)
            .filter(|s| s.dismissed_at.is_some())
            .filter_map(|s| s.app_id.as_deref())
            .collect();
        let dismissed = self.hidden.iter().filter_map(move |t| {
            let s = t.session()?;
            let at = s.dismissed_at?;
            let under = s
                .app_parent
                .as_deref()
                .is_some_and(|p| parents.contains(&p));
            (!under).then_some((t, at))
        });
        let mut all: Vec<_> = self
            .archive
            .iter()
            .map(|a| (&a.tile, a.at))
            .chain(dismissed)
            .collect();
        all.sort_by_key(|a| std::cmp::Reverse(a.1));
        all
    }

    /// Brings a tile in the archive back to the rack, as it stands now, after the others of its
    /// kind, and a session with its dismissed sub-sessions under it. It returns whether a tile
    /// came back.
    pub fn unarchive(&mut self, id: TileId, now: SystemTime) -> bool {
        let (mut tile, family) = if let Some(i) = self.archive.iter().position(|a| a.tile.id == id)
        {
            (self.archive.remove(i).tile, Vec::new())
        } else if let Some(i) = self.hidden.iter().position(|t| t.id == id && dismissed(t)) {
            let family = self.dismissed_family(&self.hidden[i]);
            (self.hidden.remove(i), family)
        } else {
            return false;
        };
        returned(&mut tile);
        tile.refresh_need(now);
        self.show(tile);
        // Each sub-session comes back after its parent, which came back before it.
        for sub in family {
            if let Some(i) = self.hidden.iter().position(|t| t.id == sub) {
                let mut sub = self.hidden.remove(i);
                returned(&mut sub);
                sub.refresh_need(now);
                self.show(sub);
            }
        }
        true
    }

    /// Forgets one tile in the archive, as Clear all forgets every one: your own tile goes for
    /// good, and a dismissed session leaves the list, with the sub-sessions listed under it, though
    /// each still comes back when it needs you (user, 2026-11-17). It returns whether a tile left
    /// the archive.
    pub fn forget(&mut self, id: TileId) -> bool {
        if let Some(i) = self.archive.iter().position(|a| a.tile.id == id) {
            self.archive.remove(i);
            return true;
        }
        let Some(tile) = self.hidden.iter().find(|t| t.id == id && dismissed(t)) else {
            return false;
        };
        let family = self.dismissed_family(tile);
        for tile in &mut self.hidden {
            if tile.id == id || family.contains(&tile.id) {
                returned(tile);
            }
        }
        true
    }

    // The dismissed sub-sessions under a dismissed session, at any depth, each after its parent,
    // which the archive lists under the session's row.
    fn dismissed_family(&self, head: &Tile) -> Vec<TileId> {
        let mut family: Vec<&str> = head
            .session()
            .and_then(|s| s.app_id.as_deref())
            .into_iter()
            .collect();
        let mut ids = Vec::new();
        while let Some(t) = self.hidden.iter().find(|t| {
            !ids.contains(&t.id)
                && dismissed(t)
                && t.session()
                    .and_then(|s| s.app_parent.as_deref())
                    .is_some_and(|p| family.contains(&p))
        }) {
            ids.push(t.id);
            family.extend(t.session().and_then(|s| s.app_id.as_deref()));
        }
        ids
    }

    /// Empties the archive: your own tiles in it go for good, and the sessions you dismissed
    /// leave it, though each still comes back when it needs you.
    pub fn clear_archive(&mut self) {
        self.archive.clear();
        for tile in &mut self.hidden {
            returned(tile);
        }
    }

    /// The snoozed tiles, in the order you snoozed them.
    pub fn snoozed(&self) -> impl Iterator<Item = &Tile> {
        self.hidden.iter().filter(|t| snoozed(t))
    }

    /// Whether a snoozed session needs you, which the rack shows on its count of snoozed tiles
    /// without bringing the session back.
    pub fn snoozed_need_you(&self) -> bool {
        self.snoozed()
            .filter_map(Tile::session)
            .any(|s| match s.state {
                SessionState::NeedsApproval | SessionState::Unread => true,
                SessionState::YourTurn => !s.read,
                _ => false,
            })
    }

    // A tile coming back from off the rack goes where a new tile of its kind would: a session's
    // after its parent's family or the last session tile, and your own after your others.
    fn show(&mut self, tile: Tile) {
        if tile.is_own() {
            self.tiles.push(tile);
            return;
        }
        let parent = tile.session().and_then(|s| s.app_parent.clone());
        let index = self.session_slot(parent.as_deref());
        self.tiles.insert(index, tile);
    }

    /// Starts a session's 5-minute timer, as a turn that ends does, when one of its pull requests
    /// broke while nothing works on it: its checks failed, or it came to conflict with its base.
    /// A session that's working leaves it to Claude, and a timer already set, or a snooze, stays
    /// as it is (user, 2026-11-27). An ended session's tile has no timer slot to show or cancel a
    /// timer in, so it starts none. It returns whether the timer started.
    pub fn pull_broke(&mut self, session_id: &str, now: SystemTime) -> bool {
        let Some(tile) = self.tiles.iter_mut().find(|t| {
            t.session().is_some_and(|s| {
                s.id == session_id
                    && !matches!(s.state, SessionState::Working | SessionState::Ended)
            })
        }) else {
            return false;
        };
        if tile.timer != Timer::Idle {
            return false;
        }
        tile.timer = Timer::start(now, DEFAULT_MINUTES);
        tile.refresh_need(now);
        true
    }

    /// Starts a timer of `minutes`, or starts a running one over.
    pub fn start_timer(&mut self, id: TileId, minutes: u32, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id) {
            tile.timer = Timer::start(now, minutes);
            tile.refresh_need(now);
            take_in_hand(tile);
        }
    }

    /// Brightens every time-up tile again, as it did when its time ran out, for when you're back
    /// at the PC after you were away and might have missed it, such as a reminder for tomorrow
    /// that ran out overnight (user, 2026-11-16). It returns whether any tile brightens.
    pub fn welcome_back(&mut self, now: SystemTime) -> bool {
        let mut any = false;
        for tile in &mut self.tiles {
            if let Some((Need::TimeUp, _)) = tile.need {
                tile.need = Some((Need::TimeUp, now));
                any = true;
            }
        }
        any
    }

    /// Brightens again every tile on the rack that started needing you at `since` or later, whose
    /// brighten waited while you were busy (record/areas/rack/03-holding-pings.md). It returns
    /// whether any tile brightens.
    pub fn brighten_since(&mut self, since: SystemTime, now: SystemTime) -> bool {
        let mut any = false;
        for tile in &mut self.tiles {
            if let Some((need, at)) = tile.need
                && at >= since
            {
                tile.need = Some((need, now));
                any = true;
            }
        }
        any
    }

    /// Sets a reminder for tomorrow, over any timer running, where the local time is
    /// `utc_offset` seconds ahead of UTC.
    pub fn remind_tomorrow(&mut self, id: TileId, now: SystemTime, utc_offset: i64) {
        if let Some(tile) = self.tile_mut(id) {
            tile.timer = Timer::tomorrow(now, utc_offset);
            tile.refresh_need(now);
            take_in_hand(tile);
        }
    }

    pub fn cancel_timer(&mut self, id: TileId, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id).filter(|t| t.timer.is_set()) {
            tile.timer = Timer::Idle;
            tile.refresh_need(now);
            take_in_hand(tile);
        }
    }

    /// A click on a time-up tile clears time-up.
    pub fn clear_time_up(&mut self, id: TileId, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id).filter(|t| t.timer.is_time_up()) {
            tile.timer = Timer::Idle;
            tile.refresh_need(now);
        }
    }

    /// Your own tiles, those on the rack in order, then those snoozed, then those in the archive,
    /// with their titles, timers, and where each is, which is what `store` saves.
    pub fn own_tiles(&self) -> impl Iterator<Item = (&str, Timer, OwnPlace)> {
        fn own(t: &Tile) -> Option<&str> {
            match &t.body {
                Body::Own(o) => Some(o.title.as_str()),
                Body::Session(_) => None,
            }
        }
        let shown = self
            .tiles
            .iter()
            .filter_map(move |t| own(t).map(|title| (title, t.timer, OwnPlace::Shown)));
        let snoozed = self
            .hidden
            .iter()
            .filter_map(move |t| own(t).map(|title| (title, t.timer, OwnPlace::Snoozed)));
        let archived = self.archive.iter().filter_map(move |a| {
            own(&a.tile).map(|title| (title, a.tile.timer, OwnPlace::Archived(a.at)))
        });
        shown.chain(snoozed).chain(archived)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{BackgroundTask, Tool, WrapUp};
    use std::time::UNIX_EPOCH;

    fn at(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000 + s)
    }

    const FOLDER: &str = "/work/plates";

    fn apply(rack: &mut Rack, id: &str, event: SessionEvent, s: u64) -> TileId {
        rack.apply(id, Path::new(FOLDER), event, at(s))
    }

    fn session(rack: &Rack, id: TileId) -> &Session {
        rack.tile(id).and_then(Tile::session).unwrap()
    }

    fn started() -> SessionEvent {
        SessionEvent::Started {
            title: None,
            resumed: false,
        }
    }

    fn prompt(text: &str) -> SessionEvent {
        SessionEvent::PromptSubmitted {
            title: None,
            prompt: Some(text.into()),
        }
    }

    fn bash(command: &str) -> SessionEvent {
        SessionEvent::ToolPending {
            tool: Tool {
                name: "Bash".into(),
                command: Some(command.into()),
                file: None,
                target: None,
            },
        }
    }

    fn stop(message: &str) -> SessionEvent {
        SessionEvent::TurnEnded {
            last_message: Some(message.into()),
            background: Vec::new(),
        }
    }

    fn plan(statuses: &[crate::session::StepStatus]) -> SessionEvent {
        use crate::session::{PlanChange, Step};
        SessionEvent::PlanChanged(PlanChange::Replace(
            statuses
                .iter()
                .map(|&status| Step {
                    text: "Step".into(),
                    status,
                })
                .collect(),
        ))
    }

    #[test]
    fn a_plan_with_a_step_pending_lasts_into_the_next_turn_and_a_finished_one_goes() {
        use crate::session::StepStatus::{Completed as C, InProgress as I, Pending as P};
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Build it"), 0);
        apply(&mut rack, "a", plan(&[C, I, P]), 1);
        assert_eq!(session(&rack, id).plan.len(), 3);

        apply(&mut rack, "a", stop("Which way?"), 2);
        apply(&mut rack, "a", prompt("The first"), 3);
        assert_eq!(
            session(&rack, id).plan.len(),
            3,
            "a step was pending, so the answer carries the plan on"
        );

        apply(&mut rack, "a", plan(&[C, C, I]), 4);
        apply(&mut rack, "a", stop("Done."), 5);
        apply(&mut rack, "a", prompt("Thanks"), 6);
        assert!(session(&rack, id).plan.is_empty());
    }

    #[test]
    fn a_plans_change_ends_a_wait() {
        use crate::session::StepStatus::InProgress;
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Watch the checks"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            1,
        );
        apply(&mut rack, "a", stop("Waiting on the checks."), 2);
        assert_eq!(session(&rack, id).state, SessionState::Waiting);
        apply(&mut rack, "a", plan(&[InProgress]), 3);
        assert_eq!(session(&rack, id).state, SessionState::Working);
    }

    #[test]
    fn a_tile_follows_its_session_to_the_claude_code_session_the_app_moved_it_to() {
        let mut rack = Rack::new();
        let first = apply(&mut rack, "first", prompt("Go"), 0);
        let old = apply(&mut rack, "old", prompt("Go"), 0);
        let last = apply(&mut rack, "last", prompt("Go"), 0);
        apply(
            &mut rack,
            "old",
            SessionEvent::Interrupted { denied: false },
            1,
        );
        let order = |rack: &Rack| -> Vec<TileId> { rack.tiles().iter().map(|t| t.id).collect() };

        let now = at(1);
        assert!(rack.follow("old", "new", Some(Path::new("/t/new.jsonl")), now));
        assert!(!rack.knows("old"));
        let s = session(&rack, old);
        assert_eq!(s.id, "new");
        assert_eq!(s.transcript.as_deref(), Some(Path::new("/t/new.jsonl")));
        assert_eq!(s.summary.as_deref(), Some("Interrupted"), "the state stays");
        assert_eq!(order(&rack), [first, old, last]);
        assert!(
            !rack.follow("gone", "elsewhere", None, now),
            "no tile, no move"
        );

        // Events from the newer session came before the app's file named it, so it has a tile of
        // its own at the end, whose session the old tile takes, keeping its place and the app's
        // fields the newer tile hasn't learned yet.
        let app = AppSession {
            id: Some("local_s".into()),
            parent: Some("local_p".into()),
            ..AppSession::default()
        };
        rack.set_app_session("new", &app);
        apply(&mut rack, "newer", prompt("Go on"), 2);
        assert!(rack.follow("new", "newer", None, at(2)));
        assert_eq!(order(&rack), [first, old, last]);
        let s = session(&rack, old);
        assert_eq!(s.id, "newer");
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.app_id.as_deref(), Some("local_s"));
        assert_eq!(s.app_parent.as_deref(), Some("local_p"));

        // A snoozed tile stays snoozed, and the newer tile goes.
        rack.snooze(old, Timer::later(at(3), 2), at(3));
        apply(&mut rack, "newest", prompt("Again"), 4);
        assert!(rack.follow("newer", "newest", None, at(4)));
        assert_eq!(order(&rack), [first, last]);
        assert!(rack.snoozed().any(|t| t.id == old));
        assert_eq!(rack.session("newest").map(|(id, _)| id), Some(old));
    }

    #[test]
    fn a_sub_session_goes_under_its_parent_after_the_ones_already_there() {
        let app = |id: &str, parent: Option<&str>| AppSession {
            id: Some(id.into()),
            parent: parent.map(Into::into),
            ..AppSession::default()
        };
        let mut rack = Rack::new();
        let add = |rack: &mut Rack, id: &str, app_id: &str, parent: Option<&str>| {
            let tile = apply(rack, id, started(), 0);
            rack.set_app_session(id, &app(app_id, parent));
            rack.place_under_parent(id);
            tile
        };
        let plates = add(&mut rack, "plates", "local_p", None);
        let other = add(&mut rack, "other", "local_o", None);
        let probe = add(&mut rack, "probe", "local_c1", Some("local_p"));
        let deeper = add(&mut rack, "deeper", "local_c2", Some("local_c1"));
        let second = add(&mut rack, "second", "local_c3", Some("local_p"));
        let orphan = add(&mut rack, "orphan", "local_x", Some("local_gone"));
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, [plates, probe, deeper, second, other, orphan]);

        // A sub-session the app detaches leaves its parent's family with its own sub-session,
        // so its sibling stays under the parent, and one that sat elsewhere stays there.
        rack.set_app_session("probe", &app("local_c1", None));
        rack.set_app_session("orphan", &app("local_x", None));
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, [plates, second, probe, deeper, other, orphan]);
    }

    fn ids(rack: &Rack) -> Vec<TileId> {
        rack.tiles().iter().map(|t| t.id).collect()
    }

    // A rack of "plates", its sub-session "probe", and probe's own sub-session "deeper", then
    // "other".
    fn family_rack() -> (Rack, [TileId; 4]) {
        let mut rack = Rack::new();
        let add = |rack: &mut Rack, id: &str, parent: Option<&str>| {
            let tile = apply(rack, id, stop("Done."), 0);
            let app = AppSession {
                id: Some(format!("local_{id}")),
                parent: parent.map(|p| format!("local_{p}")),
                ..AppSession::default()
            };
            rack.set_app_session(id, &app);
            rack.place_under_parent(id);
            tile
        };
        let plates = add(&mut rack, "plates", None);
        let other = add(&mut rack, "other", None);
        let probe = add(&mut rack, "probe", Some("plates"));
        let deeper = add(&mut rack, "deeper", Some("probe"));
        (rack, [plates, probe, deeper, other])
    }

    #[test]
    fn dismissing_a_session_takes_its_sub_sessions_and_an_undo_brings_them_back() {
        let (mut rack, [plates, probe, deeper, other]) = family_rack();
        assert_eq!(rack.family(plates), [probe, deeper]);
        apply(&mut rack, "deeper", SessionEvent::Ended, 1);
        let removed = rack.remove(plates, at(5)).unwrap();
        assert_eq!(ids(&rack), [other]);
        assert!(!rack.knows("deeper"), "an ended sub-session goes for good");
        let listed: Vec<TileId> = rack.archived().iter().map(|(t, _)| t.id).collect();
        assert_eq!(listed, [plates], "the archive lists the family once");

        rack.restore(removed);
        assert_eq!(ids(&rack), [plates, probe, deeper, other]);
        assert!(rack.archived().is_empty());
    }

    #[test]
    fn bringing_a_session_back_from_the_archive_brings_its_sub_sessions() {
        let (mut rack, [plates, probe, deeper, other]) = family_rack();
        rack.remove(plates, at(5));
        assert!(rack.unarchive(plates, at(6)));
        assert_eq!(ids(&rack), [other, plates, probe, deeper]);
        assert!(rack.archived().is_empty());
    }

    #[test]
    fn an_undo_after_the_parent_came_back_puts_its_sub_sessions_under_it() {
        let (mut rack, [plates, probe, deeper, other]) = family_rack();
        let removed = rack.remove(plates, at(5)).unwrap();
        apply(&mut rack, "plates", prompt("More"), 6);
        rack.restore(removed);
        assert_eq!(ids(&rack), [other, plates, probe, deeper]);
    }

    #[test]
    fn a_dismissed_sub_session_comes_back_without_the_ones_that_went_with_it() {
        let (mut rack, [plates, probe, deeper, other]) = family_rack();
        rack.remove(probe, at(5));
        assert_eq!(ids(&rack), [plates, other]);
        apply(&mut rack, "probe", prompt("Go on"), 6);
        assert_eq!(ids(&rack), [plates, probe, other]);
        let listed: Vec<TileId> = rack.archived().iter().map(|(t, _)| t.id).collect();
        assert_eq!(listed, [deeper], "its sub-session waits until it needs you");
    }

    #[test]
    fn a_turn_that_leaves_agents_running_stays_working_until_they_report_back() {
        let task = |kind: &str| BackgroundTask {
            id: format!("{kind}-1"),
            kind: kind.into(),
            description: "Search the app".into(),
        };
        let ended = |background: Vec<BackgroundTask>| SessionEvent::TurnEnded {
            last_message: Some("Two agents are on it.".into()),
            background,
        };
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            ended(vec![task("subagent"), task("subagent")]),
            5,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.line.as_deref(), Some("2 agents running"));
        assert_eq!(rack.tile(id).unwrap().need, None);

        // An agent's result starts a turn, which ends with nothing left running.
        apply(&mut rack, "a", prompt("<task-notification>"), 60);
        apply(&mut rack, "a", stop("Both found it."), 70);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);

        // A dev server in the background doesn't hold the turn.
        apply(&mut rack, "a", prompt("Serve it"), 80);
        apply(&mut rack, "a", ended(vec![task("shell")]), 90);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);

        // And a wrap-up that asks you something makes it your turn, agents or not.
        apply(&mut rack, "a", prompt("Go"), 100);
        apply(&mut rack, "a", wrapped(true), 104);
        apply(&mut rack, "a", ended(vec![task("subagent")]), 105);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);
    }

    #[test]
    fn a_turn_that_calls_wait_waits_without_needing_you_until_the_next_turn_starts() {
        let mut rack = Rack::new();
        let id = apply(
            &mut rack,
            "a",
            prompt("Open the PR and watch its checks"),
            0,
        );
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            5,
        );
        apply(&mut rack, "a", wrapped(false), 6);
        apply(&mut rack, "a", stop("Opened #12; watching its checks."), 7);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Waiting);
        assert_eq!(s.summary.as_deref(), Some("CI checks"));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
        assert_eq!(rack.tile(id).unwrap().need, None);

        // Whether a turn the watching command's exit starts fires UserPromptSubmit on Windows is
        // unmeasured, so its first tool ends the wait too.
        apply(&mut rack, "a", bash("gh pr view 12"), 300);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.summary, None);
        assert_eq!(s.wait, None);
    }

    #[test]
    fn a_wait_carries_over_while_its_command_runs_past_a_turn_that_asked_you_something() {
        let ended = |shell: &[&str]| SessionEvent::TurnEnded {
            last_message: Some("Done.".into()),
            background: shell
                .iter()
                .map(|id| BackgroundTask {
                    id: (*id).into(),
                    kind: "shell".into(),
                    description: "cargo test".into(),
                })
                .collect(),
        };
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Run the tests"), 0);
        let on = SessionEvent::WaitCalled {
            on: "test run".into(),
        };
        apply(&mut rack, "a", on, 1);
        apply(&mut rack, "a", wrapped(false), 2);
        apply(&mut rack, "a", ended(&["s1"]), 3);
        assert_eq!(session(&rack, id).state, SessionState::Waiting);

        // A turn that asks you something is your turn, however long the tests still run.
        apply(&mut rack, "a", prompt("Which name?"), 10);
        apply(&mut rack, "a", wrapped(true), 11);
        apply(&mut rack, "a", ended(&["s1"]), 12);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);

        // Your answer's turn ends without a `wait`, and still waits on the tests.
        apply(&mut rack, "a", prompt("The first"), 20);
        apply(&mut rack, "a", wrapped(false), 21);
        apply(&mut rack, "a", ended(&["s1", "s2"]), 22);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Waiting);
        assert_eq!(s.summary.as_deref(), Some("test run"));

        // Once the tests have finished, a turn's end waits on nothing.
        apply(&mut rack, "a", prompt("Thanks"), 30);
        apply(&mut rack, "a", wrapped(false), 31);
        apply(&mut rack, "a", ended(&["s2"]), 32);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Unread);
        assert_eq!(s.carried_wait, None);
    }

    #[test]
    fn a_turn_that_pings_is_your_turn_with_the_message_and_rings_at_once() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Ping me once checks pass"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            5,
        );
        apply(&mut rack, "a", wrapped(false), 6);
        apply(&mut rack, "a", stop("Watching."), 7);
        apply(
            &mut rack,
            "a",
            SessionEvent::PingCalled {
                message: "Checks passed; ready to merge #12".into(),
            },
            300,
        );
        apply(&mut rack, "a", wrapped(false), 301);
        apply(&mut rack, "a", stop("The checks passed."), 302);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(
            s.summary.as_deref(),
            Some("Checks passed; ready to merge #12")
        );
        assert_eq!(s.pinged_at, Some(at(302)));
        assert!(rack.tile(id).unwrap().timer.is_set());
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::YourTurn, at(302))));
        assert!(rack.pinged_between(at(301), at(302)));
        assert!(!rack.pinged_between(at(302), at(400)));
    }

    #[test]
    fn a_turn_that_leaves_background_agents_running_stays_working_even_if_it_calls_wait() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            1,
        );
        apply(&mut rack, "a", wrapped(false), 2);
        apply(
            &mut rack,
            "a",
            SessionEvent::TurnEnded {
                last_message: Some("Two agents are on it.".into()),
                background: vec![BackgroundTask {
                    id: "t1".into(),
                    kind: "subagent".into(),
                    description: "Review the diff".into(),
                }],
            },
            3,
        );
        assert_eq!(session(&rack, id).state, SessionState::Working);
    }

    #[test]
    fn a_ping_you_have_read_rings_nothing_when_its_held_tone_comes() {
        let mut rack = Rack::new();
        apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::PingCalled {
                message: "Checks passed".into(),
            },
            1,
        );
        apply(&mut rack, "a", stop("Done."), 2);
        assert!(rack.pinged_between(at(1), at(2)));
        apply(&mut rack, "a", SessionEvent::Seen { at: at(3) }, 3);
        assert!(!rack.pinged_between(at(1), at(4)));
    }

    fn notification() -> SessionEvent {
        SessionEvent::PromptSubmitted {
            title: None,
            prompt: Some("<task-notification>done</task-notification>".into()),
        }
    }

    fn shown(rack: &Rack, id: TileId) -> bool {
        rack.tiles().iter().any(|t| t.id == id)
    }

    #[test]
    fn a_dismissed_waiting_session_stays_off_the_rack_when_its_turn_waits_again() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Watch the checks"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            1,
        );
        apply(&mut rack, "a", stop("Watching."), 2);
        rack.remove(id, at(3));
        apply(&mut rack, "a", notification(), 60);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            61,
        );
        apply(&mut rack, "a", stop("Still running."), 62);
        assert!(!shown(&rack, id));
        // A prompt you type brings it back.
        apply(&mut rack, "a", prompt("Status?"), 70);
        assert!(shown(&rack, id));
    }

    #[test]
    fn a_ping_brings_back_a_dismissed_session_already_at_your_turn_and_rings() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", wrapped(true), 1);
        apply(&mut rack, "a", stop("Merge it?"), 2);
        rack.remove(id, at(3));
        // The turn a background command's exit starts fires no prompt, here.
        apply(
            &mut rack,
            "a",
            SessionEvent::PingCalled {
                message: "Checks passed".into(),
            },
            60,
        );
        apply(&mut rack, "a", stop("The checks passed."), 61);
        assert!(shown(&rack, id));
        assert!(rack.pinged_between(at(60), at(61)));
    }

    #[test]
    fn a_turn_that_starts_with_no_prompt_still_starts_your_turn_over() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", wrapped(true), 1);
        apply(&mut rack, "a", stop("Merge it?"), 2);
        apply(&mut rack, "a", SessionEvent::Seen { at: at(3) }, 3);
        assert!(session(&rack, id).read);
        apply(&mut rack, "a", bash("gh pr checks 12"), 60);
        apply(&mut rack, "a", wrapped(true), 61);
        apply(&mut rack, "a", stop("Checks failed. Retry?"), 62);
        let s = session(&rack, id);
        assert!(!s.read);
        assert_eq!(s.state_since, at(62));
        assert!(rack.tile(id).unwrap().timer.is_set());
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::YourTurn, at(62))));
    }

    #[test]
    fn a_ping_you_answered_rings_nothing_when_a_later_turn_ends_at_your_turn() {
        let mut rack = Rack::new();
        apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::PingCalled {
                message: "Checks passed".into(),
            },
            1,
        );
        apply(&mut rack, "a", stop("Done."), 2);
        apply(&mut rack, "a", prompt("Merge it"), 20);
        apply(&mut rack, "a", wrapped(true), 59);
        apply(&mut rack, "a", stop("Merged. Delete the branch?"), 60);
        assert!(!rack.pinged_between(at(1), at(120)));
    }

    #[test]
    fn a_question_from_a_waiting_session_starts_a_new_turn() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("gh pr create"), 1);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            2,
        );
        apply(&mut rack, "a", stop("Watching."), 3);
        apply(
            &mut rack,
            "a",
            SessionEvent::InputAsked {
                summary: "Asks: Merge now?".into(),
            },
            60,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::NeedsApproval);
        assert!(s.recent.is_empty());
        assert_eq!(s.message, None);
    }

    #[test]
    fn a_session_whose_tile_goes_for_good_is_reported_once_for_its_task_list() {
        let mut rack = Rack::new();
        // An ended tile the rack clears after its time.
        apply(&mut rack, "cleared", prompt("Go"), 0);
        apply(&mut rack, "cleared", SessionEvent::Ended, 1);
        rack.tick(at(1) + ENDED_CLEARS_AFTER - Duration::from_secs(1));
        assert!(rack.take_gone().is_empty());
        rack.tick(at(1) + ENDED_CLEARS_AFTER);
        assert_eq!(rack.take_gone(), vec!["cleared".to_string()]);
        assert!(rack.take_gone().is_empty());

        // A dismissed session that ends.
        let dismissed = apply(&mut rack, "dismissed", prompt("Go"), 0);
        rack.remove(dismissed, at(1));
        assert!(rack.take_gone().is_empty());
        apply(&mut rack, "dismissed", SessionEvent::Ended, 2);
        assert_eq!(rack.take_gone(), vec!["dismissed".to_string()]);

        // An ended tile you remove, which an Undo could bring back, goes through `Removed`.
        let ended = apply(&mut rack, "ended", prompt("Go"), 0);
        apply(&mut rack, "ended", SessionEvent::Ended, 1);
        let removed = rack.remove(ended, at(2)).unwrap();
        assert!(rack.take_gone().is_empty());
        assert_eq!(removed.ended_lists(), vec!["ended".to_string()]);
    }

    #[test]
    fn a_session_in_the_app_keeps_its_list_under_the_apps_id() {
        let mut s = Session::new("cli-id", Path::new(FOLDER), at(0));
        assert_eq!(s.list_key(), "cli-id");
        s.app_id = Some("local_abc".into());
        assert_eq!(s.list_key(), "local_abc");
    }

    #[test]
    fn a_wrap_up_that_asks_you_something_wins_over_a_wait_and_a_ping_wins_over_both() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            1,
        );
        apply(&mut rack, "a", wrapped(true), 2);
        apply(&mut rack, "a", stop("Merge it?"), 3);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);
        assert_eq!(session(&rack, id).summary.as_deref(), Some("Merge it?"));

        // A ping rings a tile already at your turn again, as a new need.
        apply(
            &mut rack,
            "a",
            SessionEvent::PingCalled {
                message: "Checks passed".into(),
            },
            10,
        );
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            11,
        );
        apply(&mut rack, "a", stop("Done."), 12);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Checks passed"));
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::YourTurn, at(12))));
    }

    fn wrapped(asks: bool) -> SessionEvent {
        SessionEvent::WrappedUp(WrapUp {
            recap: Some("Built it.".into()),
            ask: asks.then(|| "Commit it?".into()),
            ..WrapUp::default()
        })
    }

    #[test]
    fn a_turn_keeps_its_whole_message_its_wrap_up_and_its_newest_tools_until_the_next_prompt() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        for (i, command) in ["a", "b", "b", "c", "d", "e"].iter().enumerate() {
            apply(&mut rack, "a", bash(command), 1 + i as u64);
        }
        let texts: Vec<&str> = session(&rack, id)
            .recent
            .iter()
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(
            texts,
            ["Running b", "Running c", "Running d", "Running e"],
            "the newest four, with a repeat kept once"
        );
        apply(&mut rack, "a", wrapped(true), 8);
        apply(&mut rack, "a", stop("Built it.\n\nCommit it?"), 9);
        let s = session(&rack, id);
        assert_eq!(s.message.as_deref(), Some("Built it.\n\nCommit it?"));
        assert_eq!(s.summary.as_deref(), Some("Built it."));
        assert_eq!(
            s.wrap_up.as_ref().and_then(|w| w.ask.as_deref()),
            Some("Commit it?")
        );

        apply(&mut rack, "a", prompt("Yes"), 10);
        let s = session(&rack, id);
        assert_eq!((&s.message, &s.wrap_up), (&None, &None));
        assert!(s.recent.is_empty());
    }

    #[test]
    fn a_session_start_adds_a_new_tile() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", started(), 0);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::New);
        assert_eq!(s.title(), "plates");
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
    }

    #[test]
    fn a_prompt_moves_the_tile_to_working_and_stops_its_timer() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", started(), 0);
        rack.start_timer(id, 10, at(1));
        apply(&mut rack, "a", prompt("Fix the dock hold\nThen test it"), 2);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.title(), "Fix the dock hold");
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
    }

    #[test]
    fn only_the_first_prompt_names_the_session() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("First"), 0);
        apply(&mut rack, "a", prompt("Second"), 1);
        assert_eq!(session(&rack, id).title(), "First");
    }

    #[test]
    fn a_custom_title_from_any_prompt_wins() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("First"), 0);
        let renamed = SessionEvent::PromptSubmitted {
            title: Some("Dock tuning".into()),
            prompt: Some("Second".into()),
        };
        apply(&mut rack, "a", renamed, 1);
        assert_eq!(session(&rack, id).title(), "Dock tuning");
    }

    #[test]
    fn an_approval_names_the_tool_and_starts_a_five_minute_timer() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("cargo test -p collector"), 1);
        apply(
            &mut rack,
            "a",
            SessionEvent::ApprovalAsked { tool: None },
            2,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::NeedsApproval);
        assert_eq!(
            s.summary.as_deref(),
            Some("Wants to run: cargo test -p collector")
        );
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(2), 5));
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::Approval, at(2))));
    }

    #[test]
    fn a_late_permission_prompt_keeps_the_approval_and_its_timer() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("cargo test"), 1);
        apply(
            &mut rack,
            "a",
            SessionEvent::ApprovalAsked { tool: None },
            2,
        );
        let notified = SessionEvent::ApprovalNotified {
            message: "Claude needs your permission".into(),
        };
        apply(&mut rack, "a", notified, 8);
        assert_eq!(
            session(&rack, id).summary.as_deref(),
            Some("Wants to run: cargo test")
        );
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(2), 5));
    }

    #[test]
    fn a_permission_prompt_without_a_tool_uses_its_message() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        let notified = SessionEvent::ApprovalNotified {
            message: "Claude needs your permission to fetch example.com".into(),
        };
        apply(&mut rack, "a", notified, 1);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::NeedsApproval);
        assert_eq!(
            s.summary.as_deref(),
            Some("Claude needs your permission to fetch example.com")
        );
    }

    #[test]
    fn a_finished_tool_moves_an_approval_back_to_working_and_stops_its_timer() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("cargo test"), 1);
        apply(
            &mut rack,
            "a",
            SessionEvent::ApprovalAsked { tool: None },
            2,
        );
        apply(
            &mut rack,
            "a",
            SessionEvent::ToolFinished {
                activity: None,
                failed: false,
            },
            30,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.summary, None);
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
        assert_eq!(rack.tile(id).unwrap().need, None);
    }

    #[test]
    fn a_finished_tool_leaves_a_working_tile_working() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::ToolFinished {
                activity: None,
                failed: false,
            },
            1,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.state_since, at(0));
    }

    #[test]
    fn a_stop_moves_the_tile_to_your_turn_with_the_first_line_of_claudes_message() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            stop("Drafted three headlines.\nWhich tone do you want?"),
            60,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Drafted three headlines."));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(60), 5));
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::YourTurn, at(60))));
    }

    #[test]
    fn an_api_error_ends_the_turn_with_its_text_or_its_error() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        let failed = SessionEvent::TurnFailed {
            error: "rate_limit".into(),
            text: Some("API Error: Rate limit reached".into()),
        };
        apply(&mut rack, "a", failed, 5);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("API Error: Rate limit reached"));

        let id = apply(&mut rack, "b", prompt("Go"), 0);
        let failed = SessionEvent::TurnFailed {
            error: "overloaded".into(),
            text: None,
        };
        apply(&mut rack, "b", failed, 5);
        assert_eq!(session(&rack, id).summary.as_deref(), Some("overloaded"));
    }

    #[test]
    fn an_ended_session_clears_itself_thirty_minutes_later() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done"), 0);
        apply(&mut rack, "a", SessionEvent::Ended, 10);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Ended);
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
        assert_eq!(rack.tile(id).unwrap().need, None);
        assert!(!rack.tick(at(10 + 30 * 60 - 1)));
        assert!(rack.tick(at(10 + 30 * 60)));
        assert!(rack.tile(id).is_none());
    }

    #[test]
    fn a_clear_ends_the_tile_and_brings_it_back_as_new() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done"), 0);
        apply(&mut rack, "a", SessionEvent::Ended, 10);
        apply(&mut rack, "a", started(), 10);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::New);
        assert_eq!(s.ended_at, None);
        assert!(!rack.tick(at(10 + 31 * 60)));
        assert!(rack.tile(id).is_some());
    }

    #[test]
    fn a_session_start_leaves_a_live_tile_as_it_is() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done"), 0);
        apply(&mut rack, "a", started(), 5);
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(0), 5));
    }

    #[test]
    fn an_event_from_an_unknown_session_adds_its_tile_in_that_events_state() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Done"), 0);
        let b = apply(&mut rack, "b", bash("ls"), 0);
        let c = apply(
            &mut rack,
            "c",
            SessionEvent::ApprovalAsked { tool: None },
            0,
        );
        assert_eq!(session(&rack, a).state, SessionState::YourTurn);
        assert_eq!(session(&rack, b).state, SessionState::Working);
        assert_eq!(session(&rack, c).state, SessionState::NeedsApproval);
        assert_eq!(rack.tile(a).unwrap().timer, Timer::start(at(0), 5));
    }

    #[test]
    fn a_new_session_goes_after_the_sessions_and_before_your_own_tiles() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", started(), 0);
        let own = rack.add_own("Check release CI", at(0));
        let b = apply(&mut rack, "b", started(), 1);
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![a, b, own]);
    }

    #[test]
    fn a_timer_runs_out_into_time_up_at_the_next_tick() {
        let mut rack = Rack::new();
        let id = rack.add_own("Check release CI", at(0));
        rack.start_timer(id, 5, at(0));
        assert!(!rack.tick(at(299)));
        assert!(rack.tick(at(400)));
        let tile = rack.tile(id).unwrap();
        assert_eq!(tile.timer, Timer::TimeUp { since: at(300) });
        assert_eq!(tile.need, Some((Need::TimeUp, at(400))));
        assert_eq!(rack.needs_you().count(), 1);
    }

    #[test]
    fn starting_a_running_timer_starts_it_over() {
        let mut rack = Rack::new();
        let id = rack.add_own("Check release CI", at(0));
        rack.start_timer(id, 5, at(0));
        rack.start_timer(id, 20, at(100));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(100), 20));
    }

    #[test]
    fn a_running_timer_cancels_and_a_time_up_one_clears_on_a_click() {
        let mut rack = Rack::new();
        let id = rack.add_own("Check release CI", at(0));
        rack.start_timer(id, 5, at(0));
        rack.cancel_timer(id, at(10));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);

        rack.start_timer(id, 5, at(20));
        rack.tick(at(400));
        rack.cancel_timer(id, at(401));
        assert!(rack.tile(id).unwrap().timer.is_time_up());
        rack.clear_time_up(id, at(402));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
        assert_eq!(rack.tile(id).unwrap().need, None);
    }

    #[test]
    fn a_time_up_tile_starts_another_timer_from_the_hourglass() {
        let mut rack = Rack::new();
        let id = rack.add_own("Check release CI", at(0));
        rack.start_timer(id, 5, at(0));
        rack.tick(at(300));
        rack.start_timer(id, 5, at(310));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::start(at(310), 5));
    }

    #[test]
    fn a_session_going_back_to_working_clears_its_time_up() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done"), 0);
        rack.tick(at(300));
        assert_eq!(rack.tile(id).unwrap().need, Some((Need::TimeUp, at(300))));
        apply(&mut rack, "a", prompt("Next"), 310);
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);
        assert_eq!(rack.tile(id).unwrap().need, None);
    }

    #[test]
    fn removing_your_own_tile_can_be_undone_in_its_place() {
        let mut rack = Rack::new();
        let a = rack.add_own("A", at(0));
        let b = rack.add_own("B", at(0));
        let c = rack.add_own("C", at(0));
        let removed = rack.remove(b, at(0)).unwrap();
        assert_eq!(removed.index_in_group, 1);
        rack.restore(removed);
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![a, b, c]);
    }

    #[test]
    fn an_undo_puts_your_tile_back_below_a_session_that_arrived_meanwhile() {
        let mut rack = Rack::new();
        let s1 = apply(&mut rack, "s1", started(), 0);
        let o1 = rack.add_own("O1", at(0));
        let o2 = rack.add_own("O2", at(0));
        let removed = rack.remove(o1, at(0)).unwrap();
        let s2 = apply(&mut rack, "s2", started(), 1);
        rack.restore(removed);
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![s1, s2, o1, o2]);
    }

    #[test]
    fn a_status_line_lasts_until_the_next_prompt() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        rack.set_status("a", "Reading the receiver first.\nThen the rest.", at(0));
        assert_eq!(
            session(&rack, id).status.as_deref(),
            Some("Reading the receiver first.")
        );
        apply(&mut rack, "a", prompt("Next"), 5);
        assert_eq!(session(&rack, id).status, None);
    }

    #[test]
    fn a_working_tile_shows_new_text_and_then_the_tool_after_it() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("cargo test"), 10);
        rack.set_status("a", "Running the tests.", at(10));
        assert_eq!(
            session(&rack, id).line.as_deref(),
            Some("Running the tests.")
        );
        assert!(rack.tick(at(14)));
        assert_eq!(
            session(&rack, id).line.as_deref(),
            Some("Running cargo test")
        );
        // The same text read again at the next tool doesn't hold the tile a second time.
        apply(&mut rack, "a", bash("cargo build"), 20);
        rack.set_status("a", "Running the tests.", at(20));
        assert_eq!(
            session(&rack, id).line.as_deref(),
            Some("Running cargo build")
        );
        apply(
            &mut rack,
            "a",
            SessionEvent::ToolFinished {
                activity: None,
                failed: false,
            },
            30,
        );
        assert_eq!(
            session(&rack, id).line.as_deref(),
            Some(crate::session::THINKING)
        );
    }

    #[test]
    fn a_compact_you_type_works_then_hands_you_the_turn_and_an_auto_one_keeps_working() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done."), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: true },
            1,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.line.as_deref(), Some(crate::session::COMPACTING));
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactFinished { manual: true },
            2,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some(COMPACTED));

        apply(&mut rack, "a", prompt("Go on"), 3);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: false },
            4,
        );
        let s = session(&rack, id);
        assert_eq!(s.line.as_deref(), Some(crate::session::COMPACTING));
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactFinished { manual: false },
            5,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert!(!s.compacting);
    }

    #[test]
    fn a_compaction_claude_code_starts_leaves_an_idle_tile_as_it_is() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Merge it?"), 0);
        let timer = rack.tile(id).unwrap().timer;
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: false },
            60,
        );
        let t = rack.tile(id).unwrap();
        let s = t.session().unwrap();
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Merge it?"));
        assert!(!s.compacting);
        assert_eq!(t.timer, timer, "the turn's timer runs on");

        // A tile that waits on something outside the session keeps waiting.
        apply(
            &mut rack,
            "b",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            0,
        );
        let b = apply(&mut rack, "b", stop("Watching the checks."), 1);
        apply(
            &mut rack,
            "b",
            SessionEvent::CompactStarted { manual: false },
            2,
        );
        let s = session(&rack, b);
        assert_eq!(s.state, SessionState::Waiting);
        assert_eq!(s.summary.as_deref(), Some("CI checks"));
        assert!(!s.compacting);

        // So does a tile kept working only for a background agent, and the turn's end ends a
        // compaction in the turn whose PostCompact never came.
        let c = apply(&mut rack, "c", prompt("Review it"), 0);
        apply(
            &mut rack,
            "c",
            SessionEvent::CompactStarted { manual: false },
            1,
        );
        apply(
            &mut rack,
            "c",
            SessionEvent::TurnEnded {
                last_message: Some("An agent is reviewing.".into()),
                background: vec![BackgroundTask {
                    id: "t1".into(),
                    kind: "subagent".into(),
                    description: "Review the diff".into(),
                }],
            },
            2,
        );
        let s = session(&rack, c);
        assert_eq!(s.state, SessionState::Working);
        assert!(!s.compacting);
        assert_eq!(s.line.as_deref(), Some("Agent: Review the diff"));
        apply(
            &mut rack,
            "c",
            SessionEvent::CompactStarted { manual: false },
            3,
        );
        let s = session(&rack, c);
        assert!(!s.compacting);
        assert_eq!(s.line.as_deref(), Some("Agent: Review the diff"));
    }

    #[test]
    fn a_compact_you_type_ends_a_wait() {
        let mut rack = Rack::new();
        apply(
            &mut rack,
            "a",
            SessionEvent::WaitCalled {
                on: "CI checks".into(),
            },
            0,
        );
        let id = apply(&mut rack, "a", stop("Watching the checks."), 1);
        assert_eq!(session(&rack, id).state, SessionState::Waiting);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: true },
            2,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.summary, None);
        assert_eq!(s.wait, None);
        assert_eq!(s.line.as_deref(), Some(crate::session::COMPACTING));
    }

    #[test]
    fn a_new_turn_or_a_tool_ends_a_compaction_whose_postcompact_never_came() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: false },
            1,
        );
        apply(&mut rack, "a", bash("cargo test"), 2);
        let s = session(&rack, id);
        assert!(!s.compacting);
        assert_ne!(s.line.as_deref(), Some(crate::session::COMPACTING));

        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: false },
            3,
        );
        apply(&mut rack, "a", prompt("And then?"), 4);
        let s = session(&rack, id);
        assert!(!s.compacting);
        assert_eq!(s.line.as_deref(), Some(crate::session::THINKING));
    }

    #[test]
    fn an_mcp_servers_request_needs_you_until_you_answer() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(
            &mut rack,
            "a",
            SessionEvent::InputAsked {
                summary: "github asks: Pick a repository".into(),
            },
            1,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::NeedsApproval);
        assert_eq!(s.summary.as_deref(), Some("github asks: Pick a repository"));
        apply(&mut rack, "a", SessionEvent::InputGiven, 2);
        assert_eq!(session(&rack, id).state, SessionState::Working);
    }

    #[test]
    fn every_look_records_the_latest_visit_whatever_the_turn() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        assert_eq!(session(&rack, id).seen_at, None);
        apply(&mut rack, "a", SessionEvent::Seen { at: at(5) }, 5);
        assert_eq!(
            session(&rack, id).seen_at,
            Some(at(5)),
            "a look while working"
        );
        apply(&mut rack, "a", SessionEvent::Seen { at: at(3) }, 6);
        assert_eq!(
            session(&rack, id).seen_at,
            Some(at(5)),
            "an older look doesn't move the visit back"
        );
    }

    #[test]
    fn a_turn_that_asks_nothing_is_unread_until_you_look_and_then_done() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", wrapped(false), 1);
        apply(&mut rack, "a", stop("All pushed."), 2);
        let t = rack.tile(id).unwrap();
        assert_eq!(t.session().unwrap().state, SessionState::Unread);
        assert_eq!(t.timer, Timer::Idle);
        assert_eq!(t.need, Some((Need::Unread, at(2))));

        apply(&mut rack, "a", SessionEvent::Seen { at: at(1) }, 3);
        assert_eq!(
            session(&rack, id).state,
            SessionState::Unread,
            "a look from before the turn ended doesn't read it"
        );
        apply(&mut rack, "a", SessionEvent::Seen { at: at(4) }, 4);
        let t = rack.tile(id).unwrap();
        assert_eq!(t.session().unwrap().state, SessionState::Done);
        assert_eq!(t.need, None);

        apply(&mut rack, "a", prompt("Next"), 3);
        apply(&mut rack, "a", wrapped(true), 4);
        apply(&mut rack, "a", stop("Which fix first?"), 5);
        assert_eq!(session(&rack, id).state, SessionState::YourTurn);

        apply(&mut rack, "a", prompt("And again"), 6);
        apply(&mut rack, "a", stop("No wrap-up this time."), 7);
        assert_eq!(
            session(&rack, id).state,
            SessionState::YourTurn,
            "a turn with no wrap-up is your turn"
        );

        apply(&mut rack, "a", prompt("Once more"), 8);
        apply(&mut rack, "a", wrapped(false), 9);
        apply(&mut rack, "a", stop("Done."), 10);
        assert!(rack.mark_read(id, at(11)), "clicking the tile reads it");
        assert_eq!(session(&rack, id).state, SessionState::Done);
    }

    #[test]
    fn an_interrupt_or_a_denial_hands_you_the_turn() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("rm -rf target"), 1);
        apply(
            &mut rack,
            "a",
            SessionEvent::ApprovalAsked { tool: None },
            2,
        );
        apply(
            &mut rack,
            "a",
            SessionEvent::Interrupted { denied: true },
            3,
        );
        let s = session(&rack, id);
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Denied: rm -rf target"));

        apply(&mut rack, "a", prompt("Try again"), 4);
        apply(
            &mut rack,
            "a",
            SessionEvent::Interrupted { denied: false },
            5,
        );
        assert_eq!(session(&rack, id).summary.as_deref(), Some("Interrupted"));
    }

    #[test]
    fn the_transcripts_first_prompt_names_a_session_seen_late() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("The fifth prompt"), 0);
        rack.set_first_prompt("a", "The first prompt");
        assert_eq!(session(&rack, id).title(), "The first prompt");
        rack.set_first_prompt("a", "<system-reminder>only context</system-reminder>");
        assert_eq!(session(&rack, id).title(), "The first prompt");
    }

    #[test]
    fn a_dismissed_session_stays_off_the_rack_until_it_needs_you() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", wrapped(false), 1);
        apply(&mut rack, "a", stop("Pushed."), 2);
        assert!(rack.remove(a, at(0)).is_some());
        assert!(rack.tile(a).is_none());
        assert!(rack.knows("a"));

        // Its tool events and a look keep it off the rack, but up to date.
        apply(&mut rack, "a", SessionEvent::Seen { at: at(3) }, 3);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactStarted { manual: true },
            4,
        );
        assert!(rack.tile(a).is_none());
        apply(&mut rack, "a", bash("cargo test"), 5);
        assert!(rack.tile(a).is_none());

        // A turn that asks you something brings it back, needing you.
        apply(&mut rack, "a", wrapped(true), 6);
        apply(&mut rack, "a", stop("Commit it?"), 7);
        let t = rack.tile(a).unwrap();
        assert_eq!(t.session().unwrap().state, SessionState::YourTurn);
        assert_eq!(t.need, Some((Need::YourTurn, at(7))));
    }

    #[test]
    fn a_reminder_for_tomorrow_outlasts_turns_dismissal_and_the_ended_tile_clearing() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", stop("Done for today."), 5);
        rack.remind_tomorrow(id, at(10), 0);
        let Timer::Tomorrow { ends } = rack.tile(id).unwrap().timer else {
            panic!("the tile should have a reminder for tomorrow");
        };

        // A turn and a read leave it.
        apply(&mut rack, "a", prompt("One more thing"), 20);
        apply(&mut rack, "a", stop("Done."), 30);
        rack.mark_read(id, at(31));
        assert!(rack.tile(id).unwrap().timer.is_tomorrow());

        // Dismissed, the session comes back when the reminder runs out.
        rack.remove(id, at(0));
        assert!(rack.tile(id).is_none());
        assert!(rack.tick(ends + Duration::from_secs(1)));
        let tile = rack.tile(id).unwrap();
        assert_eq!(tile.timer, Timer::TimeUp { since: ends });
        assert_eq!(tile.need.map(|(n, _)| n), Some(Need::TimeUp));

        // Back at the PC, the time-up tile brightens again, from now.
        assert!(rack.welcome_back(at(9_999_999)));
        assert_eq!(
            rack.tile(id).unwrap().need,
            Some((Need::TimeUp, at(9_999_999)))
        );

        // An ended tile stays while it's time-up, and clears once you've clicked it.
        apply(&mut rack, "a", SessionEvent::Ended, 40);
        rack.remind_tomorrow(id, at(41), 0);
        let later = ends + ENDED_CLEARS_AFTER + Duration::from_secs(3 * 86_400);
        rack.tick(later);
        assert!(rack.tile(id).unwrap().timer.is_time_up());
        rack.clear_time_up(id, later);
        rack.tick(later);
        assert!(rack.tile(id).is_none());
    }

    #[test]
    fn tiles_that_started_needing_you_while_pings_waited_brighten_again_together() {
        let mut rack = Rack::new();
        let before = apply(&mut rack, "a", stop("Push it?"), 0);
        let during = apply(&mut rack, "b", stop("Commit it?"), 20);
        assert!(rack.brighten_since(at(10), at(60)));
        assert_eq!(
            rack.tile(during).unwrap().need,
            Some((Need::YourTurn, at(60)))
        );
        assert_eq!(
            rack.tile(before).unwrap().need,
            Some((Need::YourTurn, at(0))),
            "a tile that needed you before the pings waited brightened then"
        );
        assert!(!rack.brighten_since(at(61), at(70)));
    }

    #[test]
    fn a_read_turn_can_be_marked_unread_and_needs_you_again() {
        let mut rack = Rack::new();
        let done = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", wrapped(false), 4);
        apply(&mut rack, "a", stop("Pushed."), 5);
        rack.mark_read(done, at(6));
        assert_eq!(session(&rack, done).state, SessionState::Done);
        assert!(rack.mark_unread(done, at(7)));
        assert_eq!(session(&rack, done).state, SessionState::Unread);
        assert_eq!(
            rack.tile(done).unwrap().need.map(|(n, _)| n),
            Some(Need::Unread)
        );

        let yours = apply(&mut rack, "b", prompt("Go"), 0);
        apply(&mut rack, "b", stop("Commit it?"), 5);
        rack.mark_read(yours, at(6));
        assert!(rack.mark_unread(yours, at(7)));
        let tile = rack.tile(yours).unwrap();
        assert!(!tile.session().unwrap().read);
        assert_eq!(tile.need.map(|(n, _)| n), Some(Need::YourTurn));
        assert_eq!(tile.timer, Timer::Idle, "marking it unread starts no timer");

        assert!(!rack.mark_unread(yours, at(8)), "it's unread already");
        let working = apply(&mut rack, "c", prompt("Go"), 0);
        assert!(!rack.mark_unread(working, at(8)));
    }

    #[test]
    fn reading_your_turn_stops_its_reminder_but_keeps_it_your_turn() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Commit it?"), 0);
        assert!(rack.tile(a).unwrap().timer.is_running());
        assert!(rack.mark_read(a, at(1)));
        let t = rack.tile(a).unwrap();
        assert_eq!(t.session().unwrap().state, SessionState::YourTurn);
        assert!(t.session().unwrap().read);
        assert_eq!(t.timer, Timer::Idle);
        assert_eq!(t.need, None);

        // A look from the app reads the next one, and a new turn is unread again.
        apply(&mut rack, "a", prompt("Go on"), 2);
        apply(&mut rack, "a", stop("Push it?"), 3);
        assert!(!session(&rack, a).read);
        apply(&mut rack, "a", SessionEvent::Seen { at: at(4) }, 4);
        assert!(session(&rack, a).read);
    }

    #[test]
    fn a_turn_you_looked_at_stays_read_without_a_timer_however_long_it_waits() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Commit it?"), 0);
        assert!(rack.mark_read(a, at(10)));
        rack.tick(at(3600));
        let t = rack.tile(a).unwrap();
        assert_eq!(t.timer, Timer::Idle);
        assert_eq!(t.need, None);
        assert!(
            !rack.mark_read(a, at(3601)),
            "a second look changes nothing"
        );
    }

    #[test]
    fn a_timer_you_set_yourself_outlasts_a_look() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Commit it?"), 0);
        rack.start_timer(a, 30, at(5));
        assert!(rack.mark_read(a, at(10)));
        rack.tick(at(400));
        let timer = rack.tile(a).unwrap().timer;
        assert_eq!(
            timer.remaining(at(400)),
            Some(Duration::from_secs(5 + 1800 - 400))
        );
    }

    #[test]
    fn a_failed_tool_is_marked_among_the_turns_tools() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", bash("a"), 1);
        apply(&mut rack, "a", bash("b"), 2);
        let failed = SessionEvent::ToolFinished {
            activity: Some("Running a".into()),
            failed: true,
        };
        apply(&mut rack, "a", failed, 3);
        let marks: Vec<bool> = session(&rack, id).recent.iter().map(|r| r.failed).collect();
        assert_eq!(marks, [true, false]);
    }

    #[test]
    fn a_snoozed_session_waits_for_its_time_even_when_it_needs_you() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", prompt("Deploy it"), 0);
        assert!(rack.snooze(a, Timer::later(at(10), 2), at(10)));
        assert!(rack.tile(a).is_none());
        assert_eq!(rack.snoozed().count(), 1);

        // It asks you something meanwhile, which shows on the moon and leaves it snoozed.
        apply(&mut rack, "a", stop("Is staging free?"), 60);
        assert!(rack.tile(a).is_none());
        assert!(rack.snoozed_need_you());

        // Two hours on, it comes back time-up.
        rack.tick(at(10 + 7_200));
        assert!(rack.tile(a).unwrap().timer.is_time_up());
        assert_eq!(rack.snoozed().count(), 0);
    }

    #[test]
    fn waking_a_snoozed_tile_brings_it_back_now_among_its_kind() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        let mine = rack.add_own("Call the bank", at(0));
        let other = rack.add_own("Renew the domain", at(0));
        rack.snooze(a, Timer::later(at(1), 4), at(1));
        rack.snooze(mine, Timer::tomorrow(at(1), 0), at(1));
        assert_eq!(rack.snoozed().count(), 2);

        assert!(rack.wake(mine, at(5)));
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, [other, mine]);
        assert_eq!(rack.tile(mine).unwrap().timer, Timer::Idle);

        assert!(rack.wake(a, at(6)));
        assert_eq!(
            rack.tiles()[0].id,
            a,
            "a session goes before your own tiles"
        );
        assert!(!rack.wake(a, at(7)), "only a snoozed tile wakes");
    }

    #[test]
    fn a_new_prompt_brings_a_dismissed_session_back_and_an_end_drops_it() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Done."), 0);
        let b = apply(&mut rack, "b", stop("Done."), 0);
        rack.remove(a, at(0));
        rack.remove(b, at(0));
        apply(&mut rack, "a", prompt("More"), 1);
        assert_eq!(session(&rack, a).state, SessionState::Working);
        apply(&mut rack, "b", SessionEvent::Ended, 2);
        assert!(!rack.knows("b"), "a dismissed session that ends is gone");
    }

    fn archived(rack: &Rack) -> Vec<(String, SystemTime)> {
        rack.archived()
            .into_iter()
            .map(|(t, at)| (t.title(), at))
            .collect()
    }

    #[test]
    fn removing_your_own_tile_keeps_it_in_the_archive_without_its_timer() {
        let mut rack = Rack::new();
        let a = rack.add_own("A", at(0));
        let b = rack.add_own("B", at(0));
        rack.start_timer(a, 5, at(0));
        rack.remove(a, at(10));
        rack.remove(b, at(20));
        assert_eq!(
            archived(&rack),
            vec![("B".to_string(), at(20)), ("A".to_string(), at(10))],
            "newest first"
        );
        assert!(rack.unarchive(a, at(30)));
        let back = rack.tile(a).unwrap();
        assert_eq!(back.timer, Timer::Idle);
        assert_eq!(archived(&rack), vec![("B".to_string(), at(20))]);
    }

    #[test]
    fn an_undo_takes_your_tile_back_out_of_the_archive() {
        let mut rack = Rack::new();
        let a = rack.add_own("A", at(0));
        rack.start_timer(a, 5, at(0));
        let removed = rack.remove(a, at(10)).unwrap();
        rack.restore(removed);
        assert!(rack.archived().is_empty());
        assert!(
            rack.tile(a).unwrap().timer.is_running(),
            "the Undo keeps its timer"
        );
    }

    #[test]
    fn a_dismissed_session_leaves_the_archive_once_it_comes_back() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Done."), 0);
        rack.remove(a, at(5));
        assert_eq!(archived(&rack).len(), 1);
        apply(&mut rack, "a", prompt("More"), 6);
        assert!(rack.tile(a).is_some());
        assert!(rack.archived().is_empty());
    }

    #[test]
    fn an_ended_session_cleared_from_the_rack_skips_the_archive() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", SessionEvent::Ended, 0);
        rack.remove(a, at(5));
        assert!(rack.archived().is_empty());
    }

    #[test]
    fn clearing_the_archive_keeps_a_dismissed_session_coming_back_when_it_needs_you() {
        let mut rack = Rack::new();
        let own = rack.add_own("A", at(0));
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        rack.remove(own, at(1));
        rack.remove(a, at(2));
        rack.clear_archive();
        assert!(rack.archived().is_empty());
        assert!(
            !rack.unarchive(own, at(3)),
            "your own tile is gone for good"
        );
        apply(&mut rack, "a", wrapped(true), 4);
        apply(&mut rack, "a", stop("Commit it?"), 5);
        assert!(rack.tile(a).is_some());
    }

    #[test]
    fn a_timer_that_ran_out_counts_once_between_ticks() {
        let mut rack = Rack::new();
        let a = rack.add_own("A", at(0));
        let b = rack.add_own("B", at(0));
        rack.start_timer(a, 1, at(0));
        rack.snooze(b, Timer::later(at(0), 1), at(0));
        rack.tick(at(59));
        assert!(!rack.ran_out_between(at(58), at(59)));
        rack.tick(at(60));
        assert!(rack.ran_out_between(at(59), at(60)));
        rack.tick(at(61));
        assert!(
            !rack.ran_out_between(at(60), at(61)),
            "it ran out before this tick"
        );
        rack.tick(at(3_600));
        assert!(
            rack.ran_out_between(at(3_599), at(3_600)),
            "a snoozed tile that came back counts"
        );
        assert!(rack.tile(b).is_some());
    }

    #[test]
    fn forgetting_one_archived_tile_leaves_the_others() {
        let mut rack = Rack::new();
        let own = rack.add_own("A", at(0));
        let kept = rack.add_own("B", at(0));
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        rack.remove(own, at(1));
        rack.remove(kept, at(2));
        rack.remove(a, at(3));
        assert!(rack.forget(own));
        assert!(rack.forget(a));
        let listed: Vec<TileId> = rack.archived().iter().map(|(t, _)| t.id).collect();
        assert_eq!(listed, [kept]);
        assert!(
            !rack.unarchive(own, at(4)),
            "your own tile is gone for good"
        );
        assert!(!rack.forget(own));
        apply(&mut rack, "a", wrapped(true), 5);
        apply(&mut rack, "a", stop("Commit it?"), 6);
        assert!(
            rack.tile(a).is_some(),
            "a forgotten session still comes back"
        );
    }

    #[test]
    fn forgetting_a_session_forgets_the_sub_sessions_listed_under_it() {
        let (mut rack, [plates, probe, deeper, _]) = family_rack();
        rack.remove(plates, at(5));
        assert!(rack.forget(plates));
        assert!(rack.archived().is_empty());
        assert!(!rack.unarchive(probe, at(6)));
        assert!(!rack.unarchive(deeper, at(6)));
    }

    #[test]
    fn the_archive_keeps_your_newest_removed_tiles() {
        let mut rack = Rack::new();
        for i in 0..ARCHIVE_KEEPS as u64 + 5 {
            let id = rack.add_own(&format!("T{i}"), at(0));
            rack.remove(id, at(i));
        }
        let kept = archived(&rack);
        assert_eq!(kept.len(), ARCHIVE_KEEPS);
        assert_eq!(kept[0].0, format!("T{}", ARCHIVE_KEEPS + 4));
    }

    #[test]
    fn an_undo_brings_a_dismissed_session_back_as_it_stands_now() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        let removed = rack.remove(a, at(0)).unwrap();
        rack.set_status("a", "Reading the receiver.", at(1));
        rack.restore(removed);
        assert_eq!(
            session(&rack, a).status.as_deref(),
            Some("Reading the receiver.")
        );
        let ended = apply(&mut rack, "b", SessionEvent::Ended, 2);
        let removed = rack.remove(ended, at(0)).unwrap();
        assert!(!rack.knows("b"), "an ended session's tile goes for good");
        rack.restore(removed);
        assert!(rack.tile(ended).is_some());
    }

    #[test]
    fn renaming_keeps_a_title_and_refuses_an_empty_one() {
        let mut rack = Rack::new();
        let id = rack.add_own("  Check release CI ", at(0));
        assert_eq!(rack.tile(id).unwrap().title(), "Check release CI");
        assert!(!rack.rename_own(id, "   "));
        assert!(rack.rename_own(id, "Check CI"));
        assert_eq!(rack.tile(id).unwrap().title(), "Check CI");
    }

    #[test]
    fn a_broken_pull_request_starts_the_timer_only_while_nothing_works_on_the_session() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Open the PR"), 0);
        assert!(!rack.pull_broke("a", at(1)));
        assert_eq!(rack.tile(id).unwrap().timer, Timer::Idle);

        // The turn's end starts the timer of your turn, which you cancel.
        apply(&mut rack, "a", stop("Opened it."), 2);
        rack.cancel_timer(id, at(2));
        assert!(rack.pull_broke("a", at(3)));
        assert!(rack.tile(id).unwrap().timer.is_running());
        // A timer that's already set stays as it is.
        assert!(!rack.pull_broke("a", at(4)));
        assert!(!rack.pull_broke("unknown", at(4)));
    }

    #[test]
    fn an_archived_session_leaves_for_good_whether_shown_or_dismissed() {
        let mut rack = Rack::new();
        let shown = apply(&mut rack, "shown", prompt("Go"), 0);
        let dismissed = apply(&mut rack, "dismissed", prompt("Go"), 0);
        rack.remove(dismissed, at(1));
        assert_eq!(rack.archived_in_app("shown"), Some(shown));
        assert_eq!(rack.archived_in_app("dismissed"), Some(dismissed));
        assert!(!rack.knows("shown") && !rack.knows("dismissed"));
        assert_eq!(rack.take_gone().len(), 2);
        assert_eq!(rack.archived_in_app("shown"), None);
    }
}
