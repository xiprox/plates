//! The rack's tiles in their order, and the rules from record/areas/core/01-core.md that change
//! them.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use crate::{
    session::{Session, SessionEvent, SessionState, first_line, prompt_title},
    timer::{DEFAULT_MINUTES, Timer},
};

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
            Some(SessionState::YourTurn) => Some(Need::YourTurn),
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rack {
    tiles: Vec<Tile>,
    next_id: u64,
}

impl Rack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
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
        let index = match self
            .tiles
            .iter()
            .position(|t| t.session().is_some_and(|s| s.id == session_id))
        {
            Some(i) => i,
            None => self.add_session(session_id, folder, &event, now),
        };
        let tile = &mut self.tiles[index];
        let Body::Session(s) = &mut tile.body else {
            unreachable!("the tile found by its session id holds that session");
        };
        s.folder = folder.to_path_buf();

        let set_state = |s: &mut Session, timer: &mut Timer, to: SessionState| {
            if s.state == to {
                return;
            }
            s.state = to;
            s.state_since = now;
            *timer = match to {
                SessionState::YourTurn | SessionState::NeedsApproval => {
                    Timer::start(now, DEFAULT_MINUTES)
                }
                SessionState::Working | SessionState::Ended => Timer::Idle,
                SessionState::New => *timer,
            };
        };

        match event {
            SessionEvent::Started { title } => {
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
                s.summary = None;
                s.status = None;
                s.pending_tool = None;
                set_state(s, &mut tile.timer, SessionState::Working);
            }
            SessionEvent::ToolPending { tool } => {
                s.pending_tool = Some(tool);
            }
            SessionEvent::ApprovalAsked { tool } => {
                if tool.is_some() {
                    s.pending_tool = tool;
                }
                if s.state != SessionState::NeedsApproval {
                    s.summary = s.pending_tool.as_ref().map(|t| t.summary());
                }
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            SessionEvent::ApprovalNotified { message } => {
                if s.state != SessionState::NeedsApproval {
                    s.summary = s.pending_tool.as_ref().map(|t| t.summary());
                }
                if s.summary.is_none() {
                    s.summary = Some(first_line(&message));
                }
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            SessionEvent::ToolFinished => {
                s.pending_tool = None;
                if s.state == SessionState::NeedsApproval {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
            }
            SessionEvent::TurnEnded { last_message } => {
                s.pending_tool = None;
                s.summary = last_message
                    .map(|m| first_line(&m))
                    .filter(|m| !m.is_empty());
                set_state(s, &mut tile.timer, SessionState::YourTurn);
            }
            SessionEvent::TurnFailed { error, text } => {
                s.pending_tool = None;
                s.summary = Some(
                    text.map(|t| first_line(&t))
                        .filter(|t| !t.is_empty())
                        .unwrap_or(error),
                );
                set_state(s, &mut tile.timer, SessionState::YourTurn);
            }
            SessionEvent::Ended => {
                s.pending_tool = None;
                s.summary = None;
                s.ended_at = Some(now);
                set_state(s, &mut tile.timer, SessionState::Ended);
            }
        }
        tile.refresh_need(now);
        tile.id
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

    /// Turns timers that have run out into time-up, and clears ended sessions' tiles 30 minutes
    /// after they ended. It returns whether anything changed.
    ///
    /// It refreshes every tile's need, so a time-up timer loaded from `rack.json`, which no tick
    /// changes, needs you from the first tick.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let before = self.tiles.len();
        self.tiles.retain(|t| {
            t.session()
                .and_then(|s| s.ended_at)
                .is_none_or(|ended| now < ended + ENDED_CLEARS_AFTER)
        });
        let mut changed = self.tiles.len() != before;
        for tile in &mut self.tiles {
            let (timer, need) = (tile.timer, tile.need);
            tile.timer = tile.timer.at(now);
            tile.refresh_need(now);
            changed |= tile.timer != timer || tile.need != need;
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
        let session = self.tiles.iter_mut().find_map(|t| match &mut t.body {
            Body::Session(s) if s.id == session_id => Some(s),
            _ => None,
        });
        if let Some(s) = session {
            s.first_prompt = Some(prompt.to_string());
        }
    }

    /// Sets a session's status line, the first line of the newest text Claude wrote in the turn,
    /// which a working tile shows.
    pub fn set_status(&mut self, session_id: &str, text: &str) {
        let line = first_line(text);
        if line.is_empty() {
            return;
        }
        let session = self.tiles.iter_mut().find_map(|t| match &mut t.body {
            Body::Session(s) if s.id == session_id => Some(s),
            _ => None,
        });
        if let Some(s) = session {
            s.status = Some(line);
        }
    }

    /// Sets the title the Claude desktop app shows for a session.
    pub fn set_app_title(&mut self, session_id: &str, title: &str) {
        let session = self.tiles.iter_mut().find_map(|t| match &mut t.body {
            Body::Session(s) if s.id == session_id => Some(s),
            _ => None,
        });
        if let Some(s) = session {
            s.app_title = Some(title.to_string());
        }
    }

    pub fn knows(&self, session_id: &str) -> bool {
        self.tiles
            .iter()
            .any(|t| t.session().is_some_and(|s| s.id == session_id))
    }

    /// Puts back your own tiles, in order, as `store` loaded them.
    pub fn with_own_tiles(tiles: impl IntoIterator<Item = (String, Timer)>) -> Self {
        let mut rack = Rack::new();
        for (title, timer) in tiles {
            let id = rack.new_id();
            rack.tiles.push(Tile {
                id,
                body: Body::Own(Own { title }),
                timer,
                need: None,
            });
        }
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

    /// Takes your own tile, or an ended session's tile, off the rack, for an Undo to put back.
    /// A session that hasn't ended stays, since its next event would bring it back anyway.
    pub fn remove(&mut self, id: TileId) -> Option<Removed> {
        let index = self.tiles.iter().position(|t| t.id == id)?;
        let removable = match &self.tiles[index].body {
            Body::Own(_) => true,
            Body::Session(s) => s.state == SessionState::Ended,
        };
        if !removable {
            return None;
        }
        let own = self.tiles[index].is_own();
        let index_in_group = self.tiles[..index]
            .iter()
            .filter(|t| t.is_own() == own)
            .count();
        Some(Removed {
            tile: self.tiles.remove(index),
            index_in_group,
        })
    }

    // Session tiles always come before your own, so a group's tiles sit together.
    pub fn restore(&mut self, removed: Removed) {
        let own = removed.tile.is_own();
        let sessions = self.tiles.iter().filter(|t| !t.is_own()).count();
        let (start, len) = if own {
            (sessions, self.tiles.len() - sessions)
        } else {
            (0, sessions)
        };
        let index = start + removed.index_in_group.min(len);
        self.tiles.insert(index, removed.tile);
    }

    /// Starts a timer of `minutes`, or starts a running one over.
    pub fn start_timer(&mut self, id: TileId, minutes: u32, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id) {
            tile.timer = Timer::start(now, minutes);
            tile.refresh_need(now);
        }
    }

    pub fn cancel_timer(&mut self, id: TileId, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id).filter(|t| t.timer.is_running()) {
            tile.timer = Timer::Idle;
            tile.refresh_need(now);
        }
    }

    /// A click on a time-up tile clears time-up.
    pub fn clear_time_up(&mut self, id: TileId, now: SystemTime) {
        if let Some(tile) = self.tile_mut(id).filter(|t| t.timer.is_time_up()) {
            tile.timer = Timer::Idle;
            tile.refresh_need(now);
        }
    }

    /// Your own tiles, in order, with their titles and timers, which is what `store` saves.
    pub fn own_tiles(&self) -> impl Iterator<Item = (&str, Timer)> {
        self.tiles.iter().filter_map(|t| match &t.body {
            Body::Own(o) => Some((o.title.as_str(), t.timer)),
            Body::Session(_) => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Tool;
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
        SessionEvent::Started { title: None }
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
            },
        }
    }

    fn stop(message: &str) -> SessionEvent {
        SessionEvent::TurnEnded {
            last_message: Some(message.into()),
        }
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
        apply(&mut rack, "a", SessionEvent::ToolFinished, 30);
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
        apply(&mut rack, "a", SessionEvent::ToolFinished, 1);
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
        let removed = rack.remove(b).unwrap();
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
        let removed = rack.remove(o1).unwrap();
        let s2 = apply(&mut rack, "s2", started(), 1);
        rack.restore(removed);
        let order: Vec<TileId> = rack.tiles().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![s1, s2, o1, o2]);
    }

    #[test]
    fn a_status_line_lasts_until_the_next_prompt() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        rack.set_status("a", "Reading the receiver first.\nThen the rest.");
        assert_eq!(
            session(&rack, id).status.as_deref(),
            Some("Reading the receiver first.")
        );
        apply(&mut rack, "a", prompt("Next"), 5);
        assert_eq!(session(&rack, id).status, None);
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
    fn only_your_own_tiles_and_ended_sessions_can_be_removed() {
        let mut rack = Rack::new();
        let live = apply(&mut rack, "a", prompt("Go"), 0);
        let ended = apply(&mut rack, "b", SessionEvent::Ended, 0);
        assert!(rack.remove(live).is_none());
        assert!(rack.remove(ended).is_some());
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
}
