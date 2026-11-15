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
    session::{BackgroundTask, SessionEvent, SessionState, Tool, WrapUp, first_line},
    transcript::{self, Turn},
};

/// Applies a hook message to the rack, and names a session Plates first sees after its first
/// prompt from that prompt, read from the session's transcript. It returns the tile the message
/// changed, or `None` for a message Plates ignores.
pub fn apply(rack: &mut Rack, message: HookMessage, now: SystemTime) -> Option<TileId> {
    let event = message.event?;
    let known = rack.knows(&message.session_id);
    // A look at a session Plates doesn't show, such as one whose tile you cleared, brings no
    // tile back.
    if !known && matches!(event, SessionEvent::Seen { .. }) {
        return None;
    }
    let id = rack.apply(&message.session_id, &message.cwd, event, now);
    if let Some(status) = &message.status {
        rack.set_status(&message.session_id, status, now);
    }
    if let Some(app) = &message.app {
        rack.set_app_session(&message.session_id, app);
        // A sub-session's tile goes under its parent's, which only the app's file says, so the
        // tile this message just added moves there before the rack draws it.
        if !known {
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

/// Brings session tiles restored from `sessions.json` up to date with what each session did while
/// Plates was closed, which sent its events to no receiver. A turn counts as new when the
/// transcript's record for it is later than the tile's last change, so the records Claude Code
/// adds after a turn ends, such as its `system` record, don't replay the turn.
pub fn catch_up(rack: &mut Rack) {
    let sessions: Vec<_> = rack
        .session_tiles()
        .filter_map(|(tile, _)| tile.session())
        .filter(|s| s.state != SessionState::Ended)
        .filter_map(|s| {
            let path = s.transcript.clone()?;
            Some((s.id.clone(), s.folder.clone(), path, s.state, s.state_since))
        })
        .collect();
    for (id, folder, path, state, since) in sessions {
        let Ok(Some((turn, at))) = transcript::newest_turn(&path) else {
            continue;
        };
        if at <= since {
            continue;
        }
        let in_turn = matches!(state, SessionState::Working | SessionState::NeedsApproval);
        let started = SessionEvent::PromptSubmitted {
            title: None,
            prompt: None,
        };
        match turn {
            Turn::Running => {
                if !in_turn {
                    rack.apply(&id, &folder, started, at);
                }
            }
            Turn::Finished { last_text, wrap_up } => {
                if !in_turn {
                    rack.apply(&id, &folder, started, at);
                }
                if let Some(wrap_up) = wrap_up {
                    rack.apply(&id, &folder, SessionEvent::WrappedUp(wrap_up), at);
                }
                // The transcript doesn't say what was left running.
                let ended = SessionEvent::TurnEnded {
                    last_message: last_text,
                    background: Vec::new(),
                };
                rack.apply(&id, &folder, ended, at);
            }
            Turn::Interrupted { denied } => {
                rack.apply(&id, &folder, SessionEvent::Interrupted { denied }, at);
            }
        }
    }
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
    let event = match hook.as_str() {
        "SessionStart" => Some(SessionEvent::Started {
            title: text("session_title"),
        }),
        "UserPromptSubmit" => Some(SessionEvent::PromptSubmitted {
            title: text("session_title"),
            prompt: text("prompt"),
        }),
        // The Claude desktop app has Claude end each turn with its wrap-up tool.
        "PreToolUse" if text("tool_name").as_deref() == Some("mcp__ccd_turn__wrap_up") => Some(
            SessionEvent::WrappedUp(WrapUp::read(v.get("tool_input").unwrap_or(&Value::Null))),
        ),
        "PreToolUse" => tool(&v, &cwd).map(|tool| SessionEvent::ToolPending { tool }),
        "PermissionRequest" => Some(SessionEvent::ApprovalAsked {
            tool: tool(&v, &cwd),
        }),
        "Notification" if text("notification_type").as_deref() == Some("permission_prompt") => {
            Some(SessionEvent::ApprovalNotified {
                message: text("message").unwrap_or_default(),
            })
        }
        "PostToolUse" | "PostToolUseFailure" => Some(SessionEvent::ToolFinished),
        "Stop" => Some(SessionEvent::TurnEnded {
            last_message: text("last_assistant_message"),
            background: background_tasks(&v),
        }),
        "StopFailure" => Some(SessionEvent::TurnFailed {
            error: text("error").unwrap_or_else(|| "API error".to_string()),
            text: text("last_assistant_message"),
        }),
        "SessionEnd" => Some(SessionEvent::Ended),
        "PreCompact" => Some(SessionEvent::CompactStarted),
        "PostCompact" => Some(SessionEvent::CompactFinished {
            manual: text("trigger").as_deref() == Some("manual"),
        }),
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

// Each of Stop's `background_tasks` has a `type`, such as `subagent` or `shell`, and a
// `description` of up to 1,000 characters, of which the card shows the first line.
fn background_tasks(v: &Value) -> Vec<BackgroundTask> {
    v.get("background_tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|t| {
            Some(BackgroundTask {
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
    fn the_common_fields_pick_the_session() {
        let m = parse(base("SessionStart").to_string().as_bytes()).unwrap();
        assert_eq!(m.hook, "SessionStart");
        assert_eq!(m.session_id, "abc123");
        assert_eq!(m.cwd, PathBuf::from("/work/plates"));
        assert_eq!(
            m.transcript_path,
            Some(PathBuf::from("/home/u/.claude/projects/p/abc123.jsonl"))
        );
        assert_eq!(m.event, Some(SessionEvent::Started { title: None }));
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
        assert_eq!(event(base("PostToolUse")), Some(SessionEvent::ToolFinished));
        assert_eq!(
            event(base("PostToolUseFailure")),
            Some(SessionEvent::ToolFinished)
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
                        kind: "subagent".into(),
                        description: "Search the app".into()
                    },
                    BackgroundTask {
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
