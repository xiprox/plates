//! Reading a Claude Code hook event's JSON into a `SessionEvent`, as
//! record/areas/hooks/01-claude-code-hooks.md maps them.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::session::{SessionEvent, Tool};

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
        }),
        "StopFailure" => Some(SessionEvent::TurnFailed {
            error: text("error").unwrap_or_else(|| "API error".to_string()),
            text: text("last_assistant_message"),
        }),
        "SessionEnd" => Some(SessionEvent::Ended),
        _ => None,
    };
    Ok(HookMessage {
        hook,
        session_id,
        cwd,
        transcript_path,
        event,
    })
}

// The tool an approval is for: the command of a shell tool, the file of a tool that edits one,
// with the session's folder taken off the front, or else the tool's name.
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
    Some(Tool {
        name,
        command,
        file,
    })
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
            json!({ "tool_name": "Read", "tool_input": { "file_path": "/work/plates/x" } }),
        );
        let Some(SessionEvent::ToolPending { tool }) = event(read) else {
            panic!("PreToolUse should be ToolPending");
        };
        assert_eq!(tool.summary(), "Wants to use: Read");
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
            json!({ "stop_hook_active": false, "last_assistant_message": "Done." }),
        );
        assert_eq!(
            event(stop),
            Some(SessionEvent::TurnEnded {
                last_message: Some("Done.".into())
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
