//! Reading a Claude Code hook event's JSON into a `SessionEvent`, as
//! record/areas/hooks/01-claude-code-hooks.md maps them.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::Value;

use crate::{
    desktop::AppSession,
    rack::{Rack, TileId},
    session::{
        BackgroundTask, PlanChange, SessionEvent, SessionState, Tool, UPDATE_PLAN, WrapUp,
        first_line,
    },
    tasks::{self, Field},
    transcript::{self, Turn, TurnCalls},
};

/// Applies a hook message to the rack, and names a session Plates first sees after its first
/// prompt from that prompt, read from the session's transcript. It returns the tile the message
/// changed, or `None` for a message Plates ignores.
pub fn apply(rack: &mut Rack, message: HookMessage, now: SystemTime) -> Option<TileId> {
    let event = message.event?;
    let session_id = message.session_id.as_str();
    let app = message.app.as_ref();
    let transcript = message.transcript_path.as_deref();
    // A session the Claude desktop app archived leaves the rack for good, whether its tile is
    // shown, dismissed, or snoozed, since the app still keeps the session (user, 2026-11-27).
    // While the app keeps it archived, none of its events bring a tile back, as an event it sent
    // after it was archived, such as a `SessionEnd`, would otherwise add one (unverified whether
    // archiving sends any). A session the app moved before archiving it still has its tile under
    // the session it moved from.
    if app.is_some_and(|a| a.archived) {
        if let SessionEvent::Continued { from } = &event {
            rack.archived_in_app(from);
        }
        return rack.archived_in_app(session_id);
    }
    match &event {
        SessionEvent::Continued { from } => {
            return follow(rack, from, session_id, transcript, app, now);
        }
        SessionEvent::AppChanged => return refresh_app(rack, session_id, app),
        _ => {}
    }
    // Once the Claude desktop app moves its session to a new Claude Code session, the session's
    // tile follows it. When the app's file already names the new session as its first event
    // arrives, the tile follows it then, rather than the event adding a second tile for the same
    // session of the app's. An event from the session the app left, such as its `SessionEnd`,
    // would add a tile for a session that's over (unverified whether the app's old session sends
    // any, record/areas/hooks/01-claude-code-hooks.md, Left open).
    if !rack.knows(session_id)
        && let Some(app) = app
    {
        if let Some(from) = moved_from(rack, session_id, app) {
            follow(rack, &from, session_id, transcript, Some(app), now);
        } else if left(rack, session_id, app) {
            return None;
        }
    }
    let known = rack.knows(session_id);
    // A look at a session Plates doesn't show, such as one whose tile you cleared, brings no
    // tile back. Nor does opening such a session in the Claude desktop app, which resumes it
    // without anything happening in it, so its tile comes with its next prompt or tool
    // (record/areas/hooks/log.md, 2026-11-16). Nor does a compaction Claude Code starts on its
    // own, since it fired one in a session that sat idle, with no PostCompact after it to clear
    // the tile's line (unverified, as `Rack::apply` says; record/areas/hooks/log.md, 2026-11-20).
    if !known
        && matches!(
            event,
            SessionEvent::Seen { .. }
                | SessionEvent::Started { resumed: true, .. }
                | SessionEvent::CompactStarted { manual: false }
        )
    {
        return None;
    }
    let id = rack.apply(&message.session_id, &message.cwd, event, now);
    if let Some(status) = &message.status {
        rack.set_status(&message.session_id, status, now);
    }
    if let Some(app) = &message.app {
        let new_parent = rack.set_app_session(&message.session_id, app);
        // A sub-session's tile goes under its parent's, which only the app's file says, so the
        // tile this message just added moves there before the rack draws it. A new sub-session's
        // file names its parent only after its first events, so its tile moves there then, once
        // (record/areas/hooks/log.md, 2026-11-16).
        if !known || new_parent {
            rack.place_under_parent(&message.session_id);
        }
    }
    if let Some(path) = &message.transcript_path {
        rack.set_transcript(&message.session_id, path);
    }
    if !known
        && let Some(Ok(Some(prompt))) = message
            .transcript_path
            .as_deref()
            .map(transcript::first_prompt)
    {
        rack.set_first_prompt(&message.session_id, &prompt);
    }
    Some(id)
}

/// Whether a message shows that Claude just heard from you, which ends your typing for the pings
/// that wait (record/areas/rack/03-holding-pings.md): a prompt with text you typed, rather than a
/// subagent's report or a task notification, which are only the blocks of context the Claude
/// desktop app puts before a prompt; a question you answered, whose tool finishes as you answer;
/// an MCP server's form or page you answered; or an interrupt or a denial. A tool you approved
/// doesn't count, since Claude Code sends nothing until the tool finishes, which can be minutes
/// after you approved it. It reads the session's tile as it was before the message.
pub fn answers_claude(rack: &Rack, message: &HookMessage) -> bool {
    let asked_you = || {
        rack.session(&message.session_id)
            .is_some_and(|(_, s)| s.state == SessionState::NeedsApproval && s.question)
    };
    match &message.event {
        Some(event @ SessionEvent::PromptSubmitted { .. }) => event.is_typed_prompt(),
        Some(SessionEvent::ToolFinished { .. }) => asked_you(),
        Some(SessionEvent::InputGiven | SessionEvent::Interrupted { .. }) => true,
        _ => false,
    }
}

/// Brings session tiles restored from `sessions.json` up to date with what each session did while
/// Plates was closed, which sent its events to no receiver, and gives each its plan back. A turn
/// counts as new when the
/// transcript's record for it is later than the tile's last change, so the records Claude Code
/// adds after a turn ends, such as its `system` record, don't replay the turn.
pub fn catch_up(rack: &mut Rack) {
    let ids: Vec<String> = rack
        .session_tiles()
        .filter_map(|(tile, _)| tile.session())
        .map(|s| s.id.clone())
        .collect();
    for id in ids {
        catch_up_session(rack, &id);
        restore_plan(rack, &id);
    }
}

// Plates keeps no plan in sessions.json, so a restored session's plan comes back from its
// transcript, which also holds the calls Claude made while Plates was closed. It comes after the
// catch-up's events, since the transcript's calls already reflect the turns those stand for.
fn restore_plan(rack: &mut Rack, id: &str) {
    let Some(path) = rack
        .session(id)
        .map(|(_, s)| s)
        .filter(|s| s.state != SessionState::Ended)
        .and_then(|s| s.transcript.clone())
    else {
        return;
    };
    if let Ok(plan) = transcript::plan(&path) {
        rack.set_plan(id, plan);
    }
}

fn catch_up_session(rack: &mut Rack, id: &str) {
    let Some((folder, path, state, since)) = rack
        .session(id)
        .map(|(_, s)| s)
        .filter(|s| s.state != SessionState::Ended)
        .and_then(|s| {
            Some((
                s.folder.clone(),
                s.transcript.clone()?,
                s.state,
                s.state_since,
            ))
        })
    else {
        return;
    };
    let Ok(Some((turn, at))) = transcript::newest_turn(&path) else {
        return;
    };
    if at <= since {
        return;
    }
    let in_turn = matches!(state, SessionState::Working | SessionState::NeedsApproval);
    let started = SessionEvent::PromptSubmitted {
        title: None,
        prompt: None,
    };
    match turn {
        Turn::Running => {
            if !in_turn {
                rack.apply(id, &folder, started, at);
            }
        }
        Turn::Finished { last_text, wrap_up } => {
            if !in_turn {
                rack.apply(id, &folder, started, at);
            }
            if let Some(wrap_up) = wrap_up {
                rack.apply(id, &folder, SessionEvent::WrappedUp(wrap_up), at);
            }
            // The transcript doesn't say what was left running.
            let ended = SessionEvent::TurnEnded {
                last_message: last_text,
                background: Vec::new(),
            };
            rack.apply(id, &folder, ended, at);
        }
        Turn::Interrupted { denied } => {
            rack.apply(id, &folder, SessionEvent::Interrupted { denied }, at);
        }
    }
}

/// Has the tile of the Claude Code session `from` follow the Claude desktop app's session to `to`,
/// the Claude Code session the app moved it to, and returns the tile. A tile that follows catches
/// up with the new session's transcript, as one restored from `sessions.json` does, since the new
/// session's turns may have run while Plates was closed. Claude Code keeps a project's transcripts
/// in one folder, each named for its session's ID (measured on Windows 11 25H2, 2026-11-16,
/// record/areas/hooks/log.md), so without an event from the new session to say where its
/// transcript is, Plates names the file beside the old one's. A file Claude Code hasn't written
/// yet fails to read, and the next catch-up reads it.
fn follow(
    rack: &mut Rack,
    from: &str,
    to: &str,
    transcript: Option<&Path>,
    app: Option<&AppSession>,
    now: SystemTime,
) -> Option<TileId> {
    let (_, old) = rack.session(from)?;
    // Only the file for the tile's own session of the app's moves it, rather than one that lists
    // its Claude Code session among another's earlier ones (unverified whether any file does).
    if let (Some(tile_app), Some(file_app)) = (&old.app_id, app.and_then(|a| a.id.as_ref()))
        && tile_app != file_app
    {
        return None;
    }
    let (was, since, folder) = (old.state, old.state_since, old.folder.clone());
    let transcript = transcript.map(Path::to_path_buf).or_else(|| {
        old.transcript
            .as_ref()
            .map(|path| path.with_file_name(format!("{to}.jsonl")))
    });
    let had_tile = rack.knows(to);
    if !rack.follow(from, to, transcript.as_deref(), now) {
        return None;
    }
    if !had_tile {
        // The app may end the session it left before its file names the new one, as a `/clear`
        // ends one (unverified for a move, record/areas/hooks/01-claude-code-hooks.md, Left
        // open), and the session the tile now follows is still going.
        if was == SessionState::Ended {
            let started = SessionEvent::Started {
                title: None,
                resumed: false,
            };
            rack.apply(to, &folder, started, since);
        }
        catch_up_session(rack, to);
    }
    if let Some(app) = app
        && rack.set_app_session(to, app)
    {
        rack.place_under_parent(to);
    }
    rack.session(to).map(|(tile, _)| tile)
}

// The Claude Code session of the tile that holds the app's session `app` is for, when the app's
// file names `session_id` as the session it runs in now and the tile holds another: the app has
// moved its session to `session_id`.
fn moved_from(rack: &Rack, session_id: &str, app: &AppSession) -> Option<String> {
    if app.cli_id.as_deref() != Some(session_id) {
        return None;
    }
    let app_id = app.id.as_deref()?;
    rack.session_tiles()
        .filter_map(|(tile, _)| tile.session())
        .find(|s| s.app_id.as_deref() == Some(app_id) && s.id != session_id)
        .map(|s| s.id.clone())
}

// Whether `session_id` is a Claude Code session the app has moved its session away from: the
// app's file names another as the session it runs in now, and a tile holds that session of the
// app's, which has followed the move. Without such a tile, the file may only list the ID among
// another session's earlier ones, and the event is the session's own.
fn left(rack: &Rack, session_id: &str, app: &AppSession) -> bool {
    let moved_on = app.cli_id.as_deref().is_some_and(|now| now != session_id);
    moved_on
        && app.id.as_deref().is_some_and(|app_id| {
            rack.session_tiles()
                .filter_map(|(tile, _)| tile.session())
                .any(|s| s.app_id.as_deref() == Some(app_id))
        })
}

/// Brings a tile's title and parent up to date with the Claude desktop app's file for its
/// session, which can change with no event to carry it, such as when you rename a session or
/// detach it from the session that started it. The events thread reports such a change within
/// about 1s.
fn refresh_app(rack: &mut Rack, session_id: &str, app: Option<&AppSession>) -> Option<TileId> {
    let app = app?;
    let (tile, _) = rack.session(session_id)?;
    if rack.set_app_session(session_id, app) {
        rack.place_under_parent(session_id);
    }
    Some(tile)
}

/// One hook event as the receiver got it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookMessage {
    /// The event's `hook_event_name`, such as `PreToolUse`.
    pub hook: String,
    pub session_id: String,
    pub cwd: PathBuf,
    pub transcript_path: Option<PathBuf>,
    /// The change to the session's tile, or `None` for an event Plates ignores, such as
    /// `Notification` with `idle_prompt`.
    pub event: Option<SessionEvent>,
    /// The newest text Claude wrote in the turn, which the app reads from the transcript for a
    /// tool event, since no hook event carries it mid-turn.
    pub status: Option<String>,
    /// The title and ID the Claude desktop app keeps for the session, which the app reads from
    /// the desktop app's own file for it.
    pub app: Option<AppSession>,
}

/// The hook events after which the app reads the transcript for the session's status line.
pub fn wants_status(hook: &str) -> bool {
    matches!(
        hook,
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "PermissionRequest"
    )
}

#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    Missing(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "the body isn't JSON: {e}"),
            ParseError::Missing(field) => write!(f, "the body has no `{field}`"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(body: &[u8]) -> Result<HookMessage, ParseError> {
    let v: Value = serde_json::from_slice(body).map_err(ParseError::Json)?;
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let hook = text("hook_event_name").ok_or(ParseError::Missing("hook_event_name"))?;
    let session_id = text("session_id").ok_or(ParseError::Missing("session_id"))?;
    let cwd = PathBuf::from(text("cwd").unwrap_or_default());
    let transcript_path = text("transcript_path").map(PathBuf::from);
    let plates_call = if hook == "PreToolUse" {
        plates_call(&v)
    } else {
        None
    };
    // A compaction's `trigger` is `manual` for a /compact you typed, and `auto` otherwise.
    let manual = text("trigger").as_deref() == Some("manual");
    let event = match hook.as_str() {
        "SessionStart" => Some(SessionEvent::Started {
            title: text("session_title"),
            resumed: text("source").as_deref() == Some("resume"),
        }),
        "UserPromptSubmit" => Some(SessionEvent::PromptSubmitted {
            title: text("session_title"),
            prompt: text("prompt"),
        }),
        // The Claude desktop app has Claude end each turn with its wrap-up tool.
        "PreToolUse" if text("tool_name").as_deref() == Some("mcp__ccd_turn__wrap_up") => Some(
            SessionEvent::WrappedUp(WrapUp::read(v.get("tool_input").unwrap_or(&Value::Null))),
        ),
        // The app's plan isn't one of the turn's tools. Claude sends it in the same message as the
        // next step's first tool call, so its PostToolUse can come while that tool still runs,
        // and counting it would mark that tool finished early (unverified,
        // record/areas/hooks/01-claude-code-hooks.md, "The app's plan").
        "PreToolUse" if text("tool_name").as_deref() == Some(UPDATE_PLAN) => {
            PlanChange::read(v.get("tool_input").unwrap_or(&Value::Null))
                .map(SessionEvent::PlanChanged)
        }
        "PostToolUse" | "PostToolUseFailure"
            if text("tool_name").as_deref() == Some(UPDATE_PLAN) =>
        {
            None
        }
        // Claude asking you a question is a tool too, and the Claude desktop app asks for it
        // through `PermissionRequest`, so without this the tile would ask you to approve it
        // (measured on macOS 26.5.2, 2026-11-16, record/areas/hooks/log.md). The question shows
        // from `PreToolUse`, and its `PostToolUse` moves the tile back to working once you answer.
        "PreToolUse" | "PermissionRequest"
            if text("tool_name").as_deref() == Some("AskUserQuestion") =>
        {
            Some(SessionEvent::InputAsked {
                summary: question(v.get("tool_input").unwrap_or(&Value::Null)),
            })
        }
        "PreToolUse" if plates_call.is_some() => plates_call,
        "PreToolUse" => tool(&v, &cwd).map(|tool| SessionEvent::ToolPending { tool }),
        "PermissionRequest" => Some(SessionEvent::ApprovalAsked {
            tool: tool(&v, &cwd),
        }),
        "Notification" if text("notification_type").as_deref() == Some("permission_prompt") => {
            Some(SessionEvent::ApprovalNotified {
                message: text("message").unwrap_or_default(),
            })
        }
        "PostToolUse" | "PostToolUseFailure" => Some(SessionEvent::ToolFinished {
            activity: tool(&v, &cwd).map(|t| t.activity()),
            failed: hook == "PostToolUseFailure",
        }),
        "Stop" => Some(SessionEvent::TurnEnded {
            last_message: text("last_assistant_message"),
            background: background_tasks(&v),
        }),
        "StopFailure" => Some(SessionEvent::TurnFailed {
            error: text("error").unwrap_or_else(|| "API error".to_string()),
            text: text("last_assistant_message"),
        }),
        "SessionEnd" => Some(SessionEvent::Ended),
        "PreCompact" => Some(SessionEvent::CompactStarted { manual }),
        "PostCompact" => Some(SessionEvent::CompactFinished { manual }),
        "Elicitation" => {
            let server = text("mcp_server_name").unwrap_or_else(|| "An MCP server".to_string());
            let message = text("message")
                .map(|m| first_line(&m))
                .filter(|m| !m.is_empty());
            Some(SessionEvent::InputAsked {
                summary: match message {
                    Some(message) => format!("{server} asks: {message}"),
                    None => format!("{server} asks for your input"),
                },
            })
        }
        "ElicitationResult" => Some(SessionEvent::InputGiven),
        _ => None,
    };
    Ok(HookMessage {
        hook,
        session_id,
        cwd,
        transcript_path,
        event,
        status: None,
        app: None,
    })
}

// The tool an approval is for, and what a working tile shows it doing. The input fields each
// tool's target comes from are the ones Claude Code's tool definitions name, such as Grep's
// `pattern` and Skill's `skill`.
fn tool(v: &Value, cwd: &Path) -> Option<Tool> {
    let name = v.get("tool_name")?.as_str()?.to_string();
    let input = |key: &str| {
        v.get("tool_input")
            .and_then(|i| i.get(key))
            .and_then(Value::as_str)
    };
    let command = matches!(name.as_str(), "Bash" | "PowerShell")
        .then(|| input("command"))
        .flatten()
        .map(str::to_string);
    let file = matches!(
        name.as_str(),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit"
    )
    .then(|| input("file_path").or_else(|| input("notebook_path")))
    .flatten()
    .map(|f| relative(Path::new(f), cwd));
    let target = match name.as_str() {
        "Read" | "Edit" | "MultiEdit" | "Write" => input("file_path").map(file_name),
        "NotebookEdit" => input("notebook_path").map(file_name),
        "Grep" | "Glob" => input("pattern").map(str::to_string),
        "WebFetch" => input("url").map(host),
        "WebSearch" => input("query").map(str::to_string),
        "Skill" => input("skill").map(str::to_string),
        "Bash" | "PowerShell" | "Agent" | "Task" => input("description").map(str::to_string),
        _ => None,
    }
    .map(|t| first_line(&t))
    .filter(|t| !t.is_empty());
    Some(Tool {
        name,
        command,
        file,
        target,
    })
}

// A call to the plates server's `wait` or `ping`, which Claude Code names after the server's
// registered name, `install::SERVER`, so change both together. `PreToolUse` comes before the
// server answers, so Plates checks the call's text as the server does and keeps only a call that
// passes (record/areas/tasks/02-waits-and-pings.md). The tools' names live in plates-tasks'
// `tools` and `call` too, so change them in all three together.
fn plates_call(v: &Value) -> Option<SessionEvent> {
    let input = v.get("tool_input")?;
    let field = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("");
    match v.get("tool_name")?.as_str()? {
        "mcp__plates__wait" => tasks::check_line(field("on"), Field::On)
            .ok()
            .map(|on| SessionEvent::WaitCalled { on }),
        "mcp__plates__ping" => tasks::check_line(field("message"), Field::Message)
            .ok()
            .map(|message| SessionEvent::PingCalled { message }),
        _ => None,
    }
}

/// The reason `plates-hook stop-check`, a `Stop` hook Claude Code waits for, gives Claude for
/// not ending its turn yet: the turn started a command in the background that still runs, and
/// called neither `wait` nor `ping`, so Plates would show the session as yours while Claude may
/// only be waiting on the command (user, 2026-11-27). Claude decides whether it waits, since a
/// command it left running for you, such as a dev server, waits on nothing. Claude Code sets
/// `stop_hook_active` on the retry, which this never blocks, so a turn is blocked once at most
/// (documented, https://code.claude.com/docs/en/hooks.md, "Stop input", as of 2026-11-27).
pub fn stop_nudge(stop: &Value, turn: TurnCalls) -> Option<String> {
    let retry = stop.get("stop_hook_active").and_then(Value::as_bool) == Some(true);
    if retry || !turn.backgrounded || turn.waited {
        return None;
    }
    let running: Vec<String> = background_tasks(stop)
        .into_iter()
        .filter(|t| t.kind == "shell")
        .map(|t| format!("\"{}\"", t.description))
        .collect();
    if running.is_empty() {
        return None;
    }
    Some(format!(
        "Plates: this turn left {} running in the background without calling wait. If you'll act \
         on its result when it finishes, call the plates server's wait tool with what you're \
         waiting on, then end your turn with no further message. If you won't, as for a dev \
         server left running for the user, or if this turn asks the user something, end your turn \
         again with no further message.",
        running.join(", ")
    ))
}

// Each of Stop's `background_tasks` has an `id`, a `type`, such as `subagent` or `shell`, and a
// `description` of up to 1,000 characters, of which the card shows the first line.
fn background_tasks(v: &Value) -> Vec<BackgroundTask> {
    v.get("background_tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| {
            Some(BackgroundTask {
                id: t
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                kind: t.get("type")?.as_str()?.to_string(),
                description: t
                    .get("description")
                    .and_then(Value::as_str)
                    .map(first_line)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

// "https://code.claude.com/docs/en/hooks.md" gives "code.claude.com".
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .to_string()
}

/// `AskUserQuestion`'s summary line: the first line of its first question, such as "Asks: Which
/// design should the icon use?", and how many more it asks.
fn question(input: &Value) -> String {
    let questions = input
        .get("questions")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let first = questions
        .first()
        .and_then(|q| q.get("question"))
        .and_then(Value::as_str)
        .map(first_line)
        .filter(|q| !q.is_empty());
    match (first, questions.len()) {
        (None, _) => "Asks you a question".to_string(),
        (Some(q), 0 | 1) => format!("Asks: {q}"),
        (Some(q), n) => format!("Asks: {q} (+{} more)", n - 1),
    }
}

fn relative(file: &Path, cwd: &Path) -> String {
    file.strip_prefix(cwd)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(body: Value) -> Option<SessionEvent> {
        parse(body.to_string().as_bytes()).unwrap().event
    }

    fn base(hook: &str) -> Value {
        json!({
            "session_id": "abc123",
            "transcript_path": "/home/u/.claude/projects/p/abc123.jsonl",
            "cwd": "/work/plates",
            "permission_mode": "default",
            "hook_event_name": hook,
        })
    }

    fn with(hook: &str, extra: Value) -> Value {
        let mut v = base(hook);
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v
    }

    #[test]
    fn the_plates_servers_wait_and_ping_are_read_from_pre_tool_use_once_their_text_checks_out() {
        let call = |name: &str, input: Value| {
            event(with(
                "PreToolUse",
                json!({ "tool_name": name, "tool_input": input }),
            ))
        };
        assert_eq!(
            call("mcp__plates__wait", json!({ "on": " CI checks " })),
            Some(SessionEvent::WaitCalled {
                on: "CI checks".into()
            })
        );
        assert_eq!(
            call(
                "mcp__plates__ping",
                json!({ "message": "Checks passed; ready to merge #12" })
            ),
            Some(SessionEvent::PingCalled {
                message: "Checks passed; ready to merge #12".into()
            })
        );
        // A call the server refuses is just a tool, as any other.
        let refused = call(
            "mcp__plates__wait",
            json!({ "on": "the long-running integration suite" }),
        );
        assert!(matches!(refused, Some(SessionEvent::ToolPending { .. })));
    }

    #[test]
    fn the_apps_plan_is_read_from_pre_tool_use_and_its_post_tool_use_is_ignored() {
        let input = json!({ "advance": true });
        let call = |hook: &str| {
            event(with(
                hook,
                json!({ "tool_name": "mcp__ccd_turn__update_plan", "tool_input": input }),
            ))
        };
        assert_eq!(
            call("PreToolUse"),
            Some(SessionEvent::PlanChanged(PlanChange::Update {
                done: Vec::new(),
                start: None,
                advance: true,
            }))
        );
        assert_eq!(call("PostToolUse"), None);
        assert_eq!(call("PostToolUseFailure"), None);
    }

    #[test]
    fn the_common_fields_pick_the_session() {
        let m = parse(base("SessionStart").to_string().as_bytes()).unwrap();
        assert_eq!(m.hook, "SessionStart");
        assert_eq!(m.session_id, "abc123");
        assert_eq!(m.cwd, PathBuf::from("/work/plates"));
        assert_eq!(
            m.transcript_path,
            Some(PathBuf::from("/home/u/.claude/projects/p/abc123.jsonl"))
        );
        assert_eq!(
            m.event,
            Some(SessionEvent::Started {
                title: None,
                resumed: false
            })
        );
    }

    fn message(hook: &str, extra: Value) -> HookMessage {
        parse(with(hook, extra).to_string().as_bytes()).unwrap()
    }

    fn some_time() -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000)
    }

    #[test]
    fn opening_a_session_the_rack_doesnt_show_adds_no_tile_until_it_does_something() {
        let now = some_time();
        let mut rack = Rack::new();
        let resumed = || message("SessionStart", json!({ "source": "resume" }));
        assert_eq!(apply(&mut rack, resumed(), now), None);
        assert!(!rack.knows("abc123"));
        let compacting = message("PreCompact", json!({ "trigger": "auto" }));
        assert_eq!(apply(&mut rack, compacting, now), None);
        assert!(!rack.knows("abc123"));

        let prompt = message("UserPromptSubmit", json!({ "prompt": "Go on" }));
        assert!(apply(&mut rack, prompt, now).is_some());
        // Once the rack shows the session, a resume reaches its tile.
        assert!(apply(&mut rack, resumed(), now).is_some());
    }

    #[test]
    fn a_question_from_claude_asks_for_input_rather_than_approval() {
        let input = json!({ "questions": [
            { "question": "Which design?\nRow 1 or 2", "options": [] },
            { "question": "Which background?", "options": [] },
        ] });
        let asks = Some(SessionEvent::InputAsked {
            summary: "Asks: Which design? (+1 more)".into(),
        });
        let pre = with(
            "PreToolUse",
            json!({ "tool_name": "AskUserQuestion", "tool_input": input }),
        );
        let permission = with(
            "PermissionRequest",
            json!({ "tool_name": "AskUserQuestion", "tool_input": input }),
        );
        assert_eq!(event(pre), asks);
        assert_eq!(event(permission), asks);

        let now = some_time();
        let mut rack = Rack::new();
        let id = apply(
            &mut rack,
            message(
                "PermissionRequest",
                json!({ "tool_name": "AskUserQuestion", "tool_input": input }),
            ),
            now,
        )
        .unwrap();
        let s = rack.tile(id).unwrap().session().unwrap();
        assert_eq!(s.state, SessionState::NeedsApproval);
        assert!(s.question);
        // The notification that comes a few seconds into the question leaves it a question.
        let notified = message(
            "Notification",
            json!({ "notification_type": "permission_prompt", "message": "Claude needs your permission" }),
        );
        apply(&mut rack, notified, now);
        let s = rack.tile(id).unwrap().session().unwrap();
        assert!(s.question);
        assert_eq!(s.summary.as_deref(), Some("Asks: Which design? (+1 more)"));

        let answered = message("PostToolUse", json!({ "tool_name": "AskUserQuestion" }));
        apply(&mut rack, answered, now);
        assert_eq!(
            rack.tile(id).unwrap().session().unwrap().state,
            SessionState::Working
        );
    }

    #[test]
    fn a_sub_session_moves_under_its_parent_once_the_app_names_it() {
        let now = some_time();
        let mut rack = Rack::new();
        let from = |session: &str, hook: &str, extra: Value, app: Option<AppSession>| {
            let mut m = message(hook, extra);
            m.session_id = session.into();
            m.app = app;
            m
        };
        let order = |rack: &Rack| -> Vec<String> {
            rack.tiles()
                .iter()
                .filter_map(|t| t.session())
                .map(|s| s.id.clone())
                .collect()
        };
        let prompt = || json!({ "prompt": "Go" });
        let parent = AppSession {
            id: Some("local_p".into()),
            ..AppSession::default()
        };
        apply(
            &mut rack,
            from("parent", "UserPromptSubmit", prompt(), Some(parent)),
            now,
        );
        apply(
            &mut rack,
            from("other", "UserPromptSubmit", prompt(), None),
            now,
        );
        // The sub-session's first event finds no file of the app's, so its tile goes last.
        apply(
            &mut rack,
            from("sub", "UserPromptSubmit", prompt(), None),
            now,
        );
        assert_eq!(order(&rack), ["parent", "other", "sub"]);

        let named = AppSession {
            id: Some("local_s".into()),
            parent: Some("local_p".into()),
            ..AppSession::default()
        };
        let finished = json!({ "tool_name": "Read" });
        apply(
            &mut rack,
            from("sub", "PostToolUse", finished.clone(), Some(named.clone())),
            now,
        );
        assert_eq!(order(&rack), ["parent", "sub", "other"]);
        // Once there, later events leave it in place.
        apply(
            &mut rack,
            from("sub", "PostToolUse", finished, Some(named)),
            now,
        );
        assert_eq!(order(&rack), ["parent", "sub", "other"]);
    }

    #[test]
    fn a_new_session_adds_its_tile_from_its_start() {
        let now = some_time();
        let mut rack = Rack::new();
        let started = message("SessionStart", json!({ "source": "startup" }));
        assert!(apply(&mut rack, started, now).is_some());
        assert!(rack.knows("abc123"));
    }

    #[test]
    fn a_prompt_carries_its_text_and_any_custom_title() {
        let body = with(
            "UserPromptSubmit",
            json!({ "prompt": "Fix the dock", "session_title": "Dock" }),
        );
        assert_eq!(
            event(body),
            Some(SessionEvent::PromptSubmitted {
                title: Some("Dock".into()),
                prompt: Some("Fix the dock".into())
            })
        );
    }

    #[test]
    fn a_shell_tool_names_its_command_and_an_edit_its_file() {
        let bash = with(
            "PreToolUse",
            json!({ "tool_name": "PowerShell", "tool_input": { "command": "cargo test" } }),
        );
        let Some(SessionEvent::ToolPending { tool }) = event(bash) else {
            panic!("PreToolUse should be ToolPending");
        };
        assert_eq!(tool.summary(), "Wants to run: cargo test");

        let edit = with(
            "PermissionRequest",
            json!({ "tool_name": "Edit", "tool_input": { "file_path": "/work/plates/src/lib.rs" } }),
        );
        let Some(SessionEvent::ApprovalAsked { tool: Some(tool) }) = event(edit) else {
            panic!("PermissionRequest should be ApprovalAsked with its tool");
        };
        // The file keeps the separators Claude Code sent.
        assert_eq!(tool.summary(), "Wants to edit: src/lib.rs");

        let read = with(
            "PreToolUse",
            json!({ "tool_name": "Read", "tool_input": { "file_path": "/work/plates/record/plan.md" } }),
        );
        let Some(SessionEvent::ToolPending { tool }) = event(read) else {
            panic!("PreToolUse should be ToolPending");
        };
        assert_eq!(tool.summary(), "Wants to use: Read");
        assert_eq!(tool.activity(), "Reading plan.md");
    }

    #[test]
    fn a_tools_target_comes_from_its_input() {
        let target = |name: &str, input: Value| {
            let body = with(
                "PreToolUse",
                json!({ "tool_name": name, "tool_input": input }),
            );
            let Some(SessionEvent::ToolPending { tool }) = event(body) else {
                panic!("PreToolUse should be ToolPending");
            };
            tool.activity()
        };
        assert_eq!(
            target(
                "WebFetch",
                json!({ "url": "https://code.claude.com/docs/en/hooks.md" })
            ),
            "Fetching code.claude.com"
        );
        assert_eq!(
            target(
                "PowerShell",
                json!({ "command": "cargo build", "description": "Build release\nnow" })
            ),
            "Build release"
        );
        assert_eq!(
            target("Grep", json!({ "pattern": "fn open_menu" })),
            "Searching for fn open_menu"
        );
    }

    #[test]
    fn only_a_permission_prompt_notification_counts() {
        let prompt = with(
            "Notification",
            json!({ "notification_type": "permission_prompt", "message": "Claude needs your permission" }),
        );
        assert_eq!(
            event(prompt),
            Some(SessionEvent::ApprovalNotified {
                message: "Claude needs your permission".into()
            })
        );
        let idle = with(
            "Notification",
            json!({ "notification_type": "idle_prompt", "message": "Waiting" }),
        );
        assert_eq!(event(idle), None);
    }

    #[test]
    fn a_tool_finishing_or_failing_both_finish_it() {
        let read =
            json!({ "tool_name": "Read", "tool_input": { "file_path": "/work/plates/plan.md" } });
        let finished = |hook: &str| event(with(hook, read.clone()));
        assert_eq!(
            finished("PostToolUse"),
            Some(SessionEvent::ToolFinished {
                activity: Some("Reading plan.md".into()),
                failed: false,
            })
        );
        assert_eq!(
            finished("PostToolUseFailure"),
            Some(SessionEvent::ToolFinished {
                activity: Some("Reading plan.md".into()),
                failed: true,
            })
        );
    }

    #[test]
    fn the_stop_nudge_blocks_once_a_turn_that_backgrounded_a_command_without_waiting() {
        let stop = |active: bool| {
            json!({ "stop_hook_active": active, "background_tasks": [
                { "id": "b2", "type": "shell", "status": "running", "description": "Run the tests" },
                { "id": "a1", "type": "subagent", "status": "running", "description": "Search" } ] })
        };
        let backgrounded = TurnCalls {
            backgrounded: true,
            waited: false,
        };
        let reason = stop_nudge(&stop(false), backgrounded).unwrap();
        assert!(reason.contains("\"Run the tests\""), "{reason}");
        assert!(!reason.contains("Search"), "{reason}");
        assert_eq!(stop_nudge(&stop(true), backgrounded), None);
        let waited = TurnCalls {
            backgrounded: true,
            waited: true,
        };
        assert_eq!(stop_nudge(&stop(false), waited), None);
        assert_eq!(stop_nudge(&stop(false), TurnCalls::default()), None);
        assert_eq!(
            stop_nudge(&json!({ "background_tasks": [] }), backgrounded),
            None
        );
    }

    #[test]
    fn a_turn_ends_with_claudes_message_or_an_api_error() {
        let stop = with(
            "Stop",
            json!({ "stop_hook_active": false, "last_assistant_message": "Done.",
                    "background_tasks": [
                        { "id": "a1", "type": "subagent", "status": "running",
                          "description": "Search the app\nin detail", "agent_type": "general-purpose" },
                        { "id": "b2", "type": "shell", "status": "running",
                          "description": "Dev server", "command": "npm run dev" } ] }),
        );
        assert_eq!(
            event(stop),
            Some(SessionEvent::TurnEnded {
                last_message: Some("Done.".into()),
                background: vec![
                    BackgroundTask {
                        id: "a1".into(),
                        kind: "subagent".into(),
                        description: "Search the app".into()
                    },
                    BackgroundTask {
                        id: "b2".into(),
                        kind: "shell".into(),
                        description: "Dev server".into()
                    },
                ],
            })
        );
        let failure = with(
            "StopFailure",
            json!({ "error": "rate_limit", "last_assistant_message": "API Error: Rate limit reached" }),
        );
        assert_eq!(
            event(failure),
            Some(SessionEvent::TurnFailed {
                error: "rate_limit".into(),
                text: Some("API Error: Rate limit reached".into())
            })
        );
    }

    #[test]
    fn catching_up_finishes_a_turn_that_ended_while_plates_was_closed_and_only_that() {
        use crate::rack::Rack;
        use std::time::Duration;
        let path =
            std::env::temp_dir().join(format!("plates-core-catch-up-{}.jsonl", std::process::id()));
        let lines = [
            json!({ "type": "user", "isSidechain": false, "timestamp": "2026-11-15T10:00:00Z",
                    "origin": { "kind": "human" }, "message": { "role": "user", "content": "Go" } }),
            json!({ "type": "assistant", "isSidechain": false, "timestamp": "2026-11-15T10:00:09Z",
                    "message": { "role": "assistant", "stop_reason": "end_turn",
                                 "content": [ { "type": "text", "text": "Pushed." } ] } }),
        ];
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        std::fs::write(&path, text.join("\n")).unwrap();
        let prompt_at = transcript::parse_timestamp("2026-11-15T10:00:00Z").unwrap();
        let end_at = transcript::parse_timestamp("2026-11-15T10:00:09Z").unwrap();

        let mut rack = Rack::new();
        let id = rack.apply(
            "a",
            Path::new("/w"),
            SessionEvent::PromptSubmitted {
                title: None,
                prompt: None,
            },
            prompt_at,
        );
        rack.set_transcript("a", &path);
        catch_up(&mut rack);
        let s = rack.tile(id).unwrap().session().unwrap();
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Pushed."));
        assert_eq!(s.state_since, end_at);

        // Read, and caught up again, the same turn stays read.
        rack.mark_read(id, end_at + Duration::from_secs(5));
        catch_up(&mut rack);
        assert!(rack.tile(id).unwrap().session().unwrap().read);
    }

    #[test]
    fn catching_up_gives_a_running_session_its_plan_back() {
        use crate::{rack::Rack, session::StepStatus};
        let path = std::env::temp_dir().join(format!(
            "plates-core-plan-back-{}.jsonl",
            std::process::id()
        ));
        let call = |at: &str, input: Value| {
            json!({ "type": "assistant", "isSidechain": false, "timestamp": at,
                    "message": { "role": "assistant", "content": [
                        { "type": "tool_use", "name": "mcp__ccd_turn__update_plan", "input": input } ] } })
        };
        let lines = [
            json!({ "type": "user", "isSidechain": false, "timestamp": "2026-11-21T10:00:00Z",
                    "origin": { "kind": "human" }, "message": { "role": "user", "content": "Go" } }),
            call(
                "2026-11-21T10:00:01Z",
                json!({ "scope": "first_task", "items": [
                    { "content": "Read", "status": "in_progress" },
                    { "content": "Build", "status": "pending" } ] }),
            ),
            call("2026-11-21T10:00:02Z", json!({ "advance": true })),
        ];
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        std::fs::write(&path, text.join("\n")).unwrap();

        let mut rack = Rack::new();
        let started = transcript::parse_timestamp("2026-11-21T10:00:00Z").unwrap();
        let prompt = SessionEvent::PromptSubmitted {
            title: None,
            prompt: None,
        };
        let id = rack.apply("a", Path::new("/w"), prompt, started);
        rack.set_transcript("a", &path);
        catch_up(&mut rack);
        let s = rack.tile(id).unwrap().session().unwrap();
        let statuses: Vec<StepStatus> = s.plan.iter().map(|step| step.status).collect();
        assert_eq!(statuses, [StepStatus::Completed, StepStatus::InProgress]);
        assert_eq!(s.state, SessionState::Working);
    }

    #[test]
    fn a_tile_follows_its_session_to_the_new_one_and_catches_up_with_it() {
        let dir = std::env::temp_dir().join(format!("plates-core-follow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lines = [
            json!({ "type": "user", "isSidechain": false, "timestamp": "2026-11-16T19:00:30Z",
                    "message": { "role": "user", "content": "Done, and the popup test was faulty" } }),
            json!({ "type": "assistant", "isSidechain": false, "timestamp": "2026-11-16T19:10:04Z",
                    "message": { "role": "assistant", "stop_reason": "end_turn",
                                 "content": [ { "type": "text", "text": "Stage 18 is closed." } ] } }),
        ];
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        std::fs::write(dir.join("new.jsonl"), text.join("\n")).unwrap();
        let interrupted_at = transcript::parse_timestamp("2026-11-16T18:58:46Z").unwrap();

        let mut rack = Rack::new();
        let folder = Path::new("/w");
        let prompt = SessionEvent::PromptSubmitted {
            title: None,
            prompt: None,
        };
        let id = rack.apply("old", folder, prompt, interrupted_at);
        rack.set_transcript("old", &dir.join("old.jsonl"));
        let cut = SessionEvent::Interrupted { denied: false };
        rack.apply("old", folder, cut, interrupted_at);

        let continued = |from: &str| HookMessage {
            hook: "AppFile".into(),
            session_id: "new".into(),
            cwd: folder.into(),
            transcript_path: None,
            event: Some(SessionEvent::Continued { from: from.into() }),
            status: None,
            app: None,
        };
        assert_eq!(apply(&mut rack, continued("old"), some_time()), Some(id));
        let s = rack.tile(id).unwrap().session().unwrap();
        assert_eq!(s.id, "new");
        assert_eq!(
            s.transcript.as_deref(),
            Some(dir.join("new.jsonl").as_path())
        );
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Stage 18 is closed."));

        // A move from a session the rack doesn't show brings no tile.
        assert_eq!(apply(&mut rack, continued("cleared"), some_time()), None);
        assert_eq!(rack.tiles().len(), 1);
    }

    #[test]
    fn a_tile_that_ended_before_it_followed_its_session_catches_up() {
        let dir =
            std::env::temp_dir().join(format!("plates-core-follow-ended-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let turn = json!({ "type": "assistant", "isSidechain": false,
                           "timestamp": "2026-11-16T19:10:04Z",
                           "message": { "role": "assistant", "stop_reason": "end_turn",
                                        "content": [ { "type": "text", "text": "Closed." } ] } });
        std::fs::write(dir.join("new.jsonl"), turn.to_string()).unwrap();
        let ended_at = transcript::parse_timestamp("2026-11-16T19:00:00Z").unwrap();

        let mut rack = Rack::new();
        let id = rack.apply("old", Path::new("/w"), SessionEvent::Ended, ended_at);
        rack.set_transcript("old", &dir.join("old.jsonl"));
        let mut continued = message("SessionStart", json!({}));
        continued.session_id = "new".into();
        continued.transcript_path = None;
        continued.event = Some(SessionEvent::Continued { from: "old".into() });
        assert_eq!(apply(&mut rack, continued, some_time()), Some(id));
        let s = rack.tile(id).unwrap().session().unwrap();
        assert_eq!(s.state, SessionState::YourTurn);
        assert_eq!(s.summary.as_deref(), Some("Closed."));
    }

    // A message from the Claude Code session `session_id`, with the app's file naming `cli_id` as
    // the session its session `local_s` runs in now.
    fn with_app(hook: &str, extra: Value, session_id: &str, cli_id: &str) -> HookMessage {
        let mut m = message(hook, extra);
        m.session_id = session_id.into();
        m.transcript_path = None;
        m.app = Some(AppSession {
            id: Some("local_s".into()),
            cli_id: Some(cli_id.into()),
            ..AppSession::default()
        });
        m
    }

    #[test]
    fn a_session_the_app_archived_leaves_and_its_events_bring_no_tile_back() {
        let now = some_time();
        let mut rack = Rack::new();
        let prompt = || json!({ "prompt": "Go" });
        apply(
            &mut rack,
            with_app("UserPromptSubmit", prompt(), "abc123", "abc123"),
            now,
        );
        assert!(rack.knows("abc123"));
        let archived = |hook: &str, extra: Value| {
            let mut m = with_app(hook, extra, "abc123", "abc123");
            m.app.as_mut().unwrap().archived = true;
            m
        };
        let mut changed = archived("AppFile", json!({}));
        changed.event = Some(SessionEvent::AppChanged);
        assert!(apply(&mut rack, changed, now).is_some());
        assert!(!rack.knows("abc123"));
        assert_eq!(
            apply(&mut rack, archived("SessionEnd", json!({})), now),
            None
        );
        assert!(!rack.knows("abc123"));
    }

    #[test]
    fn an_event_from_a_session_the_app_left_adds_no_tile() {
        let now = some_time();
        let mut rack = Rack::new();
        let prompt = || json!({ "prompt": "Go" });
        // Without a tile for the app's session, the file may only list the ID among another
        // session's earlier ones, so the event is the session's own.
        let stray = with_app("UserPromptSubmit", prompt(), "abc123", "newer");
        assert!(apply(&mut rack, stray, now).is_some());

        let mut rack = Rack::new();
        apply(
            &mut rack,
            with_app("UserPromptSubmit", prompt(), "newer", "newer"),
            now,
        );
        let late = with_app(
            "SessionEnd",
            json!({ "reason": "other" }),
            "abc123",
            "newer",
        );
        assert_eq!(apply(&mut rack, late, now), None);
        assert!(!rack.knows("abc123"));
    }

    #[test]
    fn a_new_sessions_first_event_moves_the_tile_when_the_app_already_names_it() {
        let now = some_time();
        let mut rack = Rack::new();
        let prompt = || json!({ "prompt": "Go" });
        let id = apply(
            &mut rack,
            with_app("UserPromptSubmit", prompt(), "old", "old"),
            now,
        )
        .unwrap();
        let cut = with_app("Stop", json!({}), "old", "old");
        apply(&mut rack, cut, now);

        let first = with_app("UserPromptSubmit", prompt(), "new", "new");
        assert_eq!(apply(&mut rack, first, now), Some(id));
        assert_eq!(rack.tiles().len(), 1, "no second tile");
        let s = rack.tile(id).unwrap().session().unwrap();
        assert_eq!(s.id, "new");
        assert_eq!(s.state, SessionState::Working);

        // A file for another session of the app's moves no tile.
        let mut other = with_app("AppFile", json!({}), "elsewhere", "elsewhere");
        other.event = Some(SessionEvent::Continued { from: "new".into() });
        other.app.as_mut().unwrap().id = Some("local_other".into());
        assert_eq!(apply(&mut rack, other, now), None);
        assert!(rack.knows("new"));
    }

    #[test]
    fn a_session_detached_from_its_parent_leaves_it_without_an_event() {
        let now = some_time();
        let mut rack = Rack::new();
        let app = |parent: Option<&str>| AppSession {
            id: Some("local_c".into()),
            parent: parent.map(Into::into),
            cli_id: Some("abc123".into()),
            ..AppSession::default()
        };
        let mut prompt = message("UserPromptSubmit", json!({ "prompt": "Go" }));
        prompt.app = Some(app(Some("local_p")));
        let id = apply(&mut rack, prompt, now).unwrap();
        let parent = |rack: &Rack| rack.tile(id).unwrap().session().unwrap().app_parent.clone();
        assert_eq!(parent(&rack).as_deref(), Some("local_p"));

        // The events thread reports the app's file changing with no event of the session's.
        let mut changed = message("UserPromptSubmit", json!({}));
        changed.event = Some(SessionEvent::AppChanged);
        changed.app = Some(app(None));
        assert_eq!(apply(&mut rack, changed, now), Some(id));
        assert_eq!(parent(&rack), None);
    }

    #[test]
    fn a_typed_prompt_or_an_answer_tells_that_claude_heard_from_you() {
        let now = some_time();
        let mut rack = Rack::new();
        let typed = message("UserPromptSubmit", json!({ "prompt": "Ship it" }));
        assert!(answers_claude(&rack, &typed));
        let report = message(
            "UserPromptSubmit",
            json!({ "prompt": "<task-notification>agent done</task-notification>" }),
        );
        assert!(!answers_claude(&rack, &report), "a subagent's report");

        // A question you answered counts, and a tool you approved doesn't.
        let question = message(
            "PermissionRequest",
            json!({ "tool_name": "AskUserQuestion", "tool_input": { "questions": [] } }),
        );
        apply(&mut rack, question, now);
        let answered = message("PostToolUse", json!({ "tool_name": "AskUserQuestion" }));
        assert!(answers_claude(&rack, &answered));
        apply(&mut rack, answered, now);
        let approval = message("PermissionRequest", json!({ "tool_name": "Bash" }));
        apply(&mut rack, approval, now);
        let ran = message("PostToolUse", json!({ "tool_name": "Bash" }));
        assert!(
            !answers_claude(&rack, &ran),
            "an approved tool, once it finishes"
        );

        let mut cut = message("Stop", json!({}));
        cut.event = Some(SessionEvent::Interrupted { denied: false });
        assert!(answers_claude(&rack, &cut));
        let form = message("ElicitationResult", json!({}));
        assert!(answers_claude(&rack, &form));
        let tool = message("PreToolUse", json!({ "tool_name": "Read" }));
        assert!(!answers_claude(&rack, &tool));
    }

    #[test]
    fn a_wrap_up_asks_when_it_has_a_question_or_blocks() {
        let wrap = |input: Value| {
            let body = with(
                "PreToolUse",
                json!({ "tool_name": "mcp__ccd_turn__wrap_up", "tool_input": input }),
            );
            let Some(SessionEvent::WrappedUp(wrap_up)) = event(body) else {
                panic!("the wrap-up's PreToolUse should be WrappedUp");
            };
            wrap_up
        };
        assert!(!wrap(json!({ "recap": "Pushed." })).asks());
        assert!(!wrap(json!({ "recap": "Pushed.", "ask": "  " })).asks());
        assert!(wrap(json!({ "recap": "Stuck.", "blocked": true })).asks());
        assert_eq!(
            wrap(json!({ "recap": "Built.", "ask": "Commit it?",
                         "actions": [ { "action": "commit", "label": "" }, { "label": "Not now" } ] })),
            WrapUp {
                recap: Some("Built.".into()),
                ask: Some("Commit it?".into()),
                blocked: false,
                choices: vec!["Commit".into(), "Not now".into()],
            }
        );
        assert_eq!(
            wrap(json!({ "ask": "Which?", "actions": "[{\"label\":\"Fix 1\"}]" })).choices,
            ["Fix 1"],
            "a list passed as a JSON string"
        );
    }

    #[test]
    fn compaction_and_an_mcp_servers_request_carry_what_the_tile_needs() {
        assert_eq!(
            event(with(
                "PostCompact",
                json!({ "trigger": "manual", "compact_summary": "..." })
            )),
            Some(SessionEvent::CompactFinished { manual: true })
        );
        assert_eq!(
            event(with("PostCompact", json!({ "trigger": "auto" }))),
            Some(SessionEvent::CompactFinished { manual: false })
        );
        assert_eq!(
            event(with(
                "PreCompact",
                json!({ "trigger": "manual", "custom_instructions": "" })
            )),
            Some(SessionEvent::CompactStarted { manual: true })
        );
        assert_eq!(
            event(with("PreCompact", json!({ "trigger": "auto" }))),
            Some(SessionEvent::CompactStarted { manual: false })
        );
        assert_eq!(
            event(with(
                "Elicitation",
                json!({ "mcp_server_name": "my-mcp-server", "message": "Please authenticate\nnow", "mode": "url" })
            )),
            Some(SessionEvent::InputAsked {
                summary: "my-mcp-server asks: Please authenticate".into()
            })
        );
    }

    #[test]
    fn a_session_end_ends_it_and_other_events_are_ignored() {
        assert_eq!(event(base("SessionEnd")), Some(SessionEvent::Ended));
        assert_eq!(event(base("SubagentStart")), None);
    }

    #[test]
    fn a_body_without_a_session_is_an_error() {
        assert!(matches!(
            parse(br#"{"hook_event_name":"Stop"}"#),
            Err(ParseError::Missing("session_id"))
        ));
        assert!(matches!(parse(b"not json"), Err(ParseError::Json(_))));
    }
}
