//! The rack's tiles in their order, and the rules from record/areas/core/01-core.md that change
//! them.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use crate::{
    desktop::AppSession,
    session::{COMPACTED, Session, SessionEvent, SessionState, first_line, prompt_title},
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rack {
    tiles: Vec<Tile>,
    /// The tiles of sessions you dismissed, kept up to date off the rack until one needs you
    /// again or you send it a new prompt.
    hidden: Vec<Tile>,
    next_id: u64,
}

/// Where `apply` found a session's tile.
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
        let holds = |t: &Tile| t.session().is_some_and(|s| s.id == session_id);
        let place = match (
            self.tiles.iter().position(holds),
            self.hidden.iter().position(holds),
        ) {
            (Some(i), _) => Place::Shown(i),
            (None, Some(i)) => Place::Hidden(i),
            (None, None) => Place::Shown(self.add_session(session_id, folder, &event, now)),
        };
        let new_prompt = matches!(event, SessionEvent::PromptSubmitted { .. });
        let tile = match place {
            Place::Shown(i) => &mut self.tiles[i],
            Place::Hidden(i) => &mut self.hidden[i],
        };
        let Body::Session(s) = &mut tile.body else {
            unreachable!("the tile found by its session id holds that session");
        };
        let was = s.state;
        s.folder = folder.to_path_buf();

        let set_state = |s: &mut Session, timer: &mut Timer, to: SessionState| {
            if s.state == to {
                return;
            }
            s.state = to;
            s.state_since = now;
            s.read = false;
            *timer = match to {
                SessionState::YourTurn | SessionState::NeedsApproval => {
                    Timer::start(now, DEFAULT_MINUTES)
                }
                SessionState::Working
                | SessionState::Unread
                | SessionState::Done
                | SessionState::Ended => Timer::Idle,
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
                s.activity = None;
                s.tool_running = false;
                s.pending_tool = None;
                s.asks = None;
                set_state(s, &mut tile.timer, SessionState::Working);
            }
            SessionEvent::WrappedUp { asks } => {
                s.asks = Some(asks);
            }
            SessionEvent::ToolPending { tool } => {
                s.activity = Some(tool.activity());
                s.activity_at = now;
                s.tool_running = true;
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
                s.tool_running = false;
                if s.state == SessionState::NeedsApproval {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
            }
            // A turn whose wrap-up asked you nothing is unread until you look at it, and any
            // other turn's end is your turn, so a session with no wrap-up still asks for you as
            // it always has (user, 2026-11-15).
            SessionEvent::TurnEnded { last_message } => {
                s.pending_tool = None;
                s.summary = last_message
                    .map(|m| first_line(&m))
                    .filter(|m| !m.is_empty());
                let to = if s.asks == Some(false) {
                    SessionState::Unread
                } else {
                    SessionState::YourTurn
                };
                set_state(s, &mut tile.timer, to);
            }
            // Only a look after the turn ended reads it: one from before is the look the app
            // last recorded, while the turn ran.
            SessionEvent::Seen { at } if at >= s.state_since => match s.state {
                SessionState::Unread => set_state(s, &mut tile.timer, SessionState::Done),
                SessionState::YourTurn if !s.read => {
                    s.read = true;
                    tile.timer = Timer::Idle;
                }
                _ => {}
            },
            SessionEvent::Seen { .. } => {}
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
            SessionEvent::CompactStarted => {
                s.compacting = true;
                if s.state != SessionState::Working {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
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
                set_state(s, &mut tile.timer, SessionState::NeedsApproval);
            }
            SessionEvent::InputGiven => {
                if s.state == SessionState::NeedsApproval {
                    s.summary = None;
                    set_state(s, &mut tile.timer, SessionState::Working);
                }
            }
            SessionEvent::Interrupted { denied } => {
                s.summary = Some(match (denied, s.pending_tool.take()) {
                    (true, Some(tool)) => format!("Denied: {}", tool.subject()),
                    (true, None) => "Denied".to_string(),
                    (false, _) => "Interrupted".to_string(),
                });
                set_state(s, &mut tile.timer, SessionState::YourTurn);
            }
        }
        // A turn that ends, or a session that ends, ends a compaction whose PostCompact never
        // came, so its tile doesn't say "Compacting session…" the next time it works.
        if s.state != SessionState::Working {
            s.compacting = false;
        }
        s.refresh_line(now);
        let id = tile.id;
        let state = s.state;
        let needs_you = state != was
            && matches!(
                state,
                SessionState::NeedsApproval | SessionState::YourTurn | SessionState::Unread
            );
        match place {
            Place::Shown(_) => tile.refresh_need(now),
            // A dismissed session comes back, after the last session tile, once it needs you or
            // you send it a new prompt, and a session that ends while dismissed goes for good
            // (user, 2026-11-15).
            Place::Hidden(i) if new_prompt || needs_you => {
                let mut tile = self.hidden.remove(i);
                tile.refresh_need(now);
                let index = self
                    .tiles
                    .iter()
                    .rposition(|t| t.session().is_some())
                    .map_or(0, |i| i + 1);
                self.tiles.insert(index, tile);
            }
            Place::Hidden(i) if state == SessionState::Ended => {
                self.hidden.remove(i);
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
        let session = self
            .tiles
            .iter_mut()
            .chain(self.hidden.iter_mut())
            .find_map(|t| match &mut t.body {
                Body::Session(s) if s.id == session_id => Some(s),
                _ => None,
            });
        if let Some(s) = session {
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
        let session = self
            .tiles
            .iter_mut()
            .chain(self.hidden.iter_mut())
            .find_map(|t| match &mut t.body {
                Body::Session(s) if s.id == session_id => Some(s),
                _ => None,
            });
        if let Some(s) = session
            && s.status.as_deref() != Some(line.as_str())
        {
            s.status = Some(line);
            s.status_at = now;
            s.refresh_line(now);
        }
    }

    /// Sets the title and ID the Claude desktop app keeps for a session, from its file for it.
    pub fn set_app_session(&mut self, session_id: &str, app: &AppSession) {
        let session = self
            .tiles
            .iter_mut()
            .chain(self.hidden.iter_mut())
            .find_map(|t| match &mut t.body {
                Body::Session(s) if s.id == session_id => Some(s),
                _ => None,
            });
        if let Some(s) = session {
            if app.title.is_some() {
                s.app_title = app.title.clone();
            }
            if app.id.is_some() {
                s.app_id = app.id.clone();
            }
        }
    }

    /// Whether the rack shows the session, or holds it dismissed.
    pub fn knows(&self, session_id: &str) -> bool {
        self.tiles
            .iter()
            .chain(&self.hidden)
            .any(|t| t.session().is_some_and(|s| s.id == session_id))
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
            // Your turn stays your turn, since it still waits for your reply, but its reminder
            // stops.
            SessionState::YourTurn if !s.read => {
                s.read = true;
                tile.timer = Timer::Idle;
            }
            _ => return false,
        }
        tile.refresh_need(now);
        true
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

    /// Takes a tile off the rack, for an Undo to put back. Your own tile and an ended session's
    /// go, and a session that hasn't ended is dismissed: it's read, its timer stops, and it waits
    /// off the rack until it needs you or you send it a new prompt.
    pub fn remove(&mut self, id: TileId) -> Option<Removed> {
        let index = self.tiles.iter().position(|t| t.id == id)?;
        let own = self.tiles[index].is_own();
        let index_in_group = self.tiles[..index]
            .iter()
            .filter(|t| t.is_own() == own)
            .count();
        let mut tile = self.tiles.remove(index);
        if let Body::Session(s) = &mut tile.body
            && s.state != SessionState::Ended
        {
            if s.state == SessionState::Unread {
                s.state = SessionState::Done;
            }
            tile.timer = Timer::Idle;
            tile.need = None;
            self.hidden.push(tile.clone());
        }
        Some(Removed {
            tile,
            index_in_group,
        })
    }

    // Session tiles always come before your own, so a group's tiles sit together. A dismissed
    // session comes back as it stands now, since its events kept it up to date, and not at all
    // if it came back or ended meanwhile.
    pub fn restore(&mut self, removed: Removed) {
        let tile = match removed.tile.session() {
            Some(s) if s.state != SessionState::Ended => {
                match self.hidden.iter().position(|t| t.id == removed.tile.id) {
                    Some(i) => self.hidden.remove(i),
                    None => return,
                }
            }
            _ => removed.tile,
        };
        let removed = Removed { tile, ..removed };
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
                target: None,
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
        apply(&mut rack, "a", SessionEvent::ToolFinished, 30);
        assert_eq!(
            session(&rack, id).line.as_deref(),
            Some(crate::session::THINKING)
        );
    }

    #[test]
    fn a_compact_you_type_works_then_hands_you_the_turn_and_an_auto_one_keeps_working() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", stop("Done."), 0);
        apply(&mut rack, "a", SessionEvent::CompactStarted, 1);
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
        apply(&mut rack, "a", SessionEvent::CompactStarted, 4);
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
    fn a_turn_that_asks_nothing_is_unread_until_you_look_and_then_done() {
        let mut rack = Rack::new();
        let id = apply(&mut rack, "a", prompt("Go"), 0);
        apply(&mut rack, "a", SessionEvent::WrappedUp { asks: false }, 1);
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
        apply(&mut rack, "a", SessionEvent::WrappedUp { asks: true }, 4);
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
        apply(&mut rack, "a", SessionEvent::WrappedUp { asks: false }, 9);
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
        apply(&mut rack, "a", SessionEvent::WrappedUp { asks: false }, 1);
        apply(&mut rack, "a", stop("Pushed."), 2);
        assert!(rack.remove(a).is_some());
        assert!(rack.tile(a).is_none());
        assert!(rack.knows("a"));

        // Its tool events and a look keep it off the rack, but up to date.
        apply(&mut rack, "a", SessionEvent::Seen { at: at(3) }, 3);
        apply(&mut rack, "a", SessionEvent::CompactStarted, 4);
        apply(
            &mut rack,
            "a",
            SessionEvent::CompactFinished { manual: false },
            5,
        );
        assert!(rack.tile(a).is_none());

        // A turn that asks you something brings it back, needing you.
        apply(&mut rack, "a", SessionEvent::WrappedUp { asks: true }, 6);
        apply(&mut rack, "a", stop("Commit it?"), 7);
        let t = rack.tile(a).unwrap();
        assert_eq!(t.session().unwrap().state, SessionState::YourTurn);
        assert_eq!(t.need, Some((Need::YourTurn, at(7))));
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
    fn a_new_prompt_brings_a_dismissed_session_back_and_an_end_drops_it() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", stop("Done."), 0);
        let b = apply(&mut rack, "b", stop("Done."), 0);
        rack.remove(a);
        rack.remove(b);
        apply(&mut rack, "a", prompt("More"), 1);
        assert_eq!(session(&rack, a).state, SessionState::Working);
        apply(&mut rack, "b", SessionEvent::Ended, 2);
        assert!(!rack.knows("b"), "a dismissed session that ends is gone");
    }

    #[test]
    fn an_undo_brings_a_dismissed_session_back_as_it_stands_now() {
        let mut rack = Rack::new();
        let a = apply(&mut rack, "a", prompt("Go"), 0);
        let removed = rack.remove(a).unwrap();
        rack.set_status("a", "Reading the receiver.", at(1));
        rack.restore(removed);
        assert_eq!(
            session(&rack, a).status.as_deref(),
            Some("Reading the receiver.")
        );
        let ended = apply(&mut rack, "b", SessionEvent::Ended, 2);
        let removed = rack.remove(ended).unwrap();
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
}
