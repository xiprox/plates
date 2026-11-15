//! A Claude Code session as its tile sees it, and the events that change it.
//! record/areas/hooks/01-claude-code-hooks.md maps each hook event to one of these events.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    New,
    Working,
    YourTurn,
    NeedsApproval,
    Ended,
}

/// One hook event that changes a session's tile, with the JSON already read out of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// `SessionStart`, which also fires on a resume, a `/clear`, and a compaction.
    Started { title: Option<String> },
    /// `UserPromptSubmit`.
    PromptSubmitted {
        title: Option<String>,
        prompt: Option<String>,
    },
    /// `PreToolUse`, kept for the summary of an approval that follows.
    ToolPending { tool: Tool },
    /// `PermissionRequest`, which names the tool it asks about.
    ApprovalAsked { tool: Option<Tool> },
    /// `Notification` with `notification_type` `permission_prompt`.
    ApprovalNotified { message: String },
    /// `PostToolUse` or `PostToolUseFailure`.
    ToolFinished,
    /// `Stop`.
    TurnEnded { last_message: Option<String> },
    /// `StopFailure`.
    TurnFailed { error: String, text: Option<String> },
    /// `SessionEnd`.
    Ended,
}

impl SessionEvent {
    /// The state this event moves a tile to, if it moves one.
    pub fn moves_to(&self) -> Option<SessionState> {
        match self {
            SessionEvent::Started { .. } => Some(SessionState::New),
            SessionEvent::PromptSubmitted { .. } => Some(SessionState::Working),
            SessionEvent::ToolPending { .. } | SessionEvent::ToolFinished => None,
            SessionEvent::ApprovalAsked { .. } | SessionEvent::ApprovalNotified { .. } => {
                Some(SessionState::NeedsApproval)
            }
            SessionEvent::TurnEnded { .. } | SessionEvent::TurnFailed { .. } => {
                Some(SessionState::YourTurn)
            }
            SessionEvent::Ended => Some(SessionState::Ended),
        }
    }
}

/// A tool Claude is about to use, which names what an approval is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    /// The command, for a tool that runs one.
    pub command: Option<String>,
    /// The file, for a tool that edits one.
    pub file: Option<String>,
}

impl Tool {
    /// The approval's summary line, as record/areas/hooks/01-claude-code-hooks.md words it.
    pub fn summary(&self) -> String {
        if let Some(command) = &self.command {
            format!("Wants to run: {}", first_line(command))
        } else if let Some(file) = &self.file {
            format!("Wants to edit: {file}")
        } else {
            format!("Wants to use: {}", self.name)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub folder: PathBuf,
    pub custom_title: Option<String>,
    pub first_prompt: Option<String>,
    pub state: SessionState,
    /// When the session last changed state, for the hover text, such as "waiting 12m".
    pub state_since: SystemTime,
    pub summary: Option<String>,
    pub pending_tool: Option<Tool>,
    pub ended_at: Option<SystemTime>,
}

impl Session {
    pub fn new(id: &str, folder: &Path, now: SystemTime) -> Self {
        Session {
            id: id.to_string(),
            folder: folder.to_path_buf(),
            custom_title: None,
            first_prompt: None,
            state: SessionState::New,
            state_since: now,
            summary: None,
            pending_tool: None,
            ended_at: None,
        }
    }

    /// The session's custom title, or else the first line of its first prompt, or else, until
    /// either arrives, the name of its folder.
    pub fn title(&self) -> String {
        self.custom_title
            .clone()
            .or_else(|| self.first_prompt.as_deref().map(first_line))
            .or_else(|| {
                self.folder
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Session".to_string())
    }
}

/// The first non-empty line of `text`, trimmed.
pub fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn a_tool_names_its_command_its_file_or_itself() {
        let tool = |command: Option<&str>, file: Option<&str>| Tool {
            name: "Bash".into(),
            command: command.map(Into::into),
            file: file.map(Into::into),
        };
        assert_eq!(
            tool(Some("cargo test -p collector\necho done"), None).summary(),
            "Wants to run: cargo test -p collector"
        );
        assert_eq!(
            tool(None, Some("src/hooks.rs")).summary(),
            "Wants to edit: src/hooks.rs"
        );
        assert_eq!(tool(None, None).summary(), "Wants to use: Bash");
    }

    #[test]
    fn a_title_falls_back_from_custom_to_first_prompt_to_folder() {
        let mut s = Session::new("a", Path::new("/work/plates"), UNIX_EPOCH);
        assert_eq!(s.title(), "plates");
        s.first_prompt = Some("\n  Fix the dock hold\nand more".into());
        assert_eq!(s.title(), "Fix the dock hold");
        s.custom_title = Some("Dock tuning".into());
        assert_eq!(s.title(), "Dock tuning");
    }
}
