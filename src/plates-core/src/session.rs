//! A Claude Code session as its tile sees it, and the events that change it.
//! record/areas/hooks/01-claude-code-hooks.md maps each hook event to one of these events.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How many of the turn's newest tools a session keeps, for the hover card.
pub const RECENT_TOOLS: usize = 4;

/// How long a working tile shows Claude's newest text before what Claude does next, so the text
/// can be read even when a tool follows it at once.
pub const STATUS_HOLDS: Duration = Duration::from_secs(4);

/// How long a working tile shows a tool at least, so a tool that runs for a few milliseconds can
/// still be read.
pub const TOOL_HOLDS: Duration = Duration::from_secs(1);

/// What a working tile shows while no tool runs and Claude has written nothing new. The Claude
/// desktop app words a thinking step the same way (read in the Claude desktop app 5.11492.5.0,
/// ion-dist.asar). No hook event says when Claude thinks, so this stands for all of Claude's work
/// between tools.
pub const THINKING: &str = "Thinking…";

/// What a working tile shows from `PreCompact` until `PostCompact`, in the Claude desktop app's
/// words for it (read in the Claude desktop app 5.11492.5.0, ion-dist.asar, its status line).
pub const COMPACTING: &str = "Compacting session…";

/// Your turn's summary line after a `/compact` you typed, in the app's words for it.
pub const COMPACTED: &str = "Compacted session";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    New,
    Working,
    YourTurn,
    /// Claude finished a turn that asks you nothing, and you haven't looked at the session since.
    Unread,
    /// Claude finished a turn that asks you nothing, and you've looked at the session since, so
    /// it waits without needing you.
    Done,
    NeedsApproval,
    Ended,
}

/// One hook event that changes a session's tile, with the JSON already read out of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionEvent {
    /// `SessionStart`, which also fires on a resume, a `/clear`, and a compaction. `resumed` is
    /// whether its `source` is `resume`, which the Claude desktop app sends when you open a
    /// session that wasn't running.
    Started {
        title: Option<String>,
        resumed: bool,
    },
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
    /// `PostToolUse`, or `PostToolUseFailure` with `failed`, and the finished tool in words.
    ToolFinished {
        activity: Option<String>,
        failed: bool,
    },
    /// `Stop`, with the tasks still running as the turn ended.
    TurnEnded {
        last_message: Option<String>,
        background: Vec<BackgroundTask>,
    },
    /// `StopFailure`.
    TurnFailed { error: String, text: Option<String> },
    /// `SessionEnd`.
    Ended,
    /// You interrupted Claude, or denied the tool it asked about, which no hook event reports, so
    /// the app reads it from the transcript.
    Interrupted { denied: bool },
    /// `PreCompact`, for a `/compact` you typed or one Claude Code started on its own.
    CompactStarted,
    /// `PostCompact`, with whether you typed the `/compact`.
    CompactFinished { manual: bool },
    /// `Elicitation`, when an MCP server asks you to fill in a form or open a page, and Claude's
    /// `AskUserQuestion` tool, which asks you to pick among options.
    InputAsked { summary: String },
    /// `ElicitationResult`: you answered the MCP server.
    InputGiven,
    /// `PreToolUse` for the Claude desktop app's `mcp__ccd_turn__wrap_up`, which Claude calls at
    /// the end of a turn.
    WrappedUp(WrapUp),
    /// You opened the session in the Claude desktop app at `at`, which the app records in its
    /// file for the session as `lastFocusedAt`.
    Seen { at: SystemTime },
    /// The Claude desktop app moved its session from the Claude Code session `from` to this one,
    /// which the app records in its file for the session as `cliSessionId`, with `from` among
    /// its `priorCliSessionIds`.
    Continued { from: String },
    /// The Claude desktop app's file for the session changed the session's title or parent, which
    /// the message carries in its `app`.
    AppChanged,
}

impl SessionEvent {
    /// The state this event moves a tile to, if it moves one.
    pub fn moves_to(&self) -> Option<SessionState> {
        match self {
            SessionEvent::Started { .. } => Some(SessionState::New),
            SessionEvent::PromptSubmitted { .. } | SessionEvent::CompactStarted => {
                Some(SessionState::Working)
            }
            SessionEvent::ToolPending { .. }
            | SessionEvent::ToolFinished { .. }
            | SessionEvent::CompactFinished { .. }
            | SessionEvent::InputGiven
            | SessionEvent::WrappedUp(_)
            | SessionEvent::Seen { .. }
            | SessionEvent::Continued { .. }
            | SessionEvent::AppChanged => None,
            SessionEvent::ApprovalAsked { .. }
            | SessionEvent::ApprovalNotified { .. }
            | SessionEvent::InputAsked { .. } => Some(SessionState::NeedsApproval),
            SessionEvent::TurnEnded { .. }
            | SessionEvent::TurnFailed { .. }
            | SessionEvent::Interrupted { .. } => Some(SessionState::YourTurn),
            SessionEvent::Ended => Some(SessionState::Ended),
        }
    }
}

/// A task still running when Claude ended its turn, one of `Stop`'s `background_tasks`
/// (documented, https://code.claude.com/docs/en/hooks.md#stop-input, as of 2026-11-15).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundTask {
    /// The task's `type`, such as `subagent`, `workflow`, or `shell`.
    pub kind: String,
    pub description: String,
}

impl BackgroundTask {
    /// Whether Claude takes the session up again once the task finishes, so its tile keeps
    /// working. A background agent's result starts a new turn, while a background command, such
    /// as a dev server, can run until someone stops it, so it doesn't keep a tile working
    /// (record/areas/hooks/01-claude-code-hooks.md, "Background tasks").
    pub fn reports_back(&self) -> bool {
        matches!(self.kind.as_str(), "subagent" | "workflow" | "teammate")
    }
}

/// What Claude passed the Claude desktop app's wrap-up tool at the end of a turn: a recap, and a
/// question with buttons that answer it when a decision waits for you. Its `ask` and `blocked`
/// say a decision waits, and a `recap` alone says nothing does (read in the tool's own
/// description, Claude desktop app 5.11492.5.0).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapUp {
    pub recap: Option<String>,
    pub ask: Option<String>,
    pub blocked: bool,
    /// What the wrap-up's buttons say, such as "Yes, fix 1".
    pub choices: Vec<String>,
}

impl WrapUp {
    /// Reads the wrap-up tool's input, whose `actions` is a list of buttons, each with a `label`,
    /// an `action` the app draws its own button for, such as `commit`, or both.
    pub fn read(input: &Value) -> WrapUp {
        let text = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        // Claude can pass the list as a JSON string rather than a list.
        let actions = match input.get("actions") {
            Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
            Some(v) => v.clone(),
            None => Value::Null,
        };
        let choices = actions
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                let field = |key: &str| a.get(key).and_then(Value::as_str).map(str::trim);
                match (field("label"), field("action")) {
                    (Some(label), _) if !label.is_empty() => Some(label.to_string()),
                    (_, Some(action)) if !action.is_empty() => Some(app_action(action)),
                    _ => None,
                }
            })
            .collect();
        WrapUp {
            recap: text("recap"),
            ask: text("ask"),
            blocked: input.get("blocked").and_then(Value::as_bool) == Some(true),
            choices,
        }
    }

    /// Whether a decision waits for you.
    pub fn asks(&self) -> bool {
        self.ask.is_some() || self.blocked
    }
}

// An app action's name in words, such as "create_pr" as "Create pr". The app draws these buttons
// itself, and what it labels them is unverified, so the card says the action's name.
fn app_action(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A tool Claude is about to use, which names what an approval is for and what a working tile
/// shows Claude doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    /// The command, for a tool that runs one.
    pub command: Option<String>,
    /// The file, for a tool that edits one.
    pub file: Option<String>,
    /// What the tool acts on, for its activity line: the file's name for a tool that reads or
    /// writes one, the pattern or query for a search, the host for a fetch, the skill's name, or
    /// the description Claude gave a command or an agent.
    pub target: Option<String>,
}

impl Tool {
    /// The approval's summary line, as record/areas/hooks/01-claude-code-hooks.md words it.
    pub fn summary(&self) -> String {
        let verb = if self.command.is_some() {
            "run"
        } else if self.file.is_some() {
            "edit"
        } else {
            "use"
        };
        format!("Wants to {verb}: {}", self.subject())
    }

    /// What an approval is for: the command's first line, the file, or the tool's name.
    pub fn subject(&self) -> String {
        if let Some(command) = &self.command {
            first_line(command)
        } else if let Some(file) = &self.file {
            file.clone()
        } else {
            self.name.clone()
        }
    }

    /// What a working tile shows Claude doing with the tool, such as "Reading plan.md".
    pub fn activity(&self) -> String {
        let target = self.target.as_deref();
        match (self.name.as_str(), target) {
            ("Read", Some(t)) => format!("Reading {t}"),
            ("Edit" | "MultiEdit" | "NotebookEdit", Some(t)) => format!("Editing {t}"),
            ("Write", Some(t)) => format!("Writing {t}"),
            ("Grep", Some(t)) => format!("Searching for {t}"),
            ("Glob", Some(t)) => format!("Finding {t}"),
            ("WebFetch", Some(t)) => format!("Fetching {t}"),
            ("WebSearch", Some(t)) => format!("Searching the web for {t}"),
            ("Skill", Some(t)) => format!("Using the {t} skill"),
            ("Bash" | "PowerShell" | "Agent" | "Task", Some(t)) => t.to_string(),
            ("Bash" | "PowerShell", None) => format!("Running {}", self.subject()),
            // An MCP tool's name is mcp__<server>__<tool>, and its last part reads best.
            (name, _) => format!("Using {}", name.rsplit("__").next().unwrap_or(name)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub folder: PathBuf,
    /// The session's transcript, which Plates reads on starting to catch up on a turn that
    /// ended while it was closed.
    pub transcript: Option<PathBuf>,
    /// The title the Claude desktop app shows for the session, from its own file for it.
    pub app_title: Option<String>,
    /// The Claude desktop app's own ID for the session, which a link opens it by.
    pub app_id: Option<String>,
    /// The app's own ID for the session this one was started from, which makes this one a
    /// sub-session.
    pub app_parent: Option<String>,
    pub custom_title: Option<String>,
    pub first_prompt: Option<String>,
    pub state: SessionState,
    /// When the session last changed state, for the hover text, such as "waiting 12m".
    pub state_since: SystemTime,
    pub summary: Option<String>,
    /// Claude's last message in the turn, whole, for the hover card.
    pub message: Option<String>,
    /// What Claude passed the turn's wrap-up tool.
    pub wrap_up: Option<WrapUp>,
    /// The turn's newest tools, oldest first, at most `RECENT_TOOLS` of them.
    pub recent: Vec<Recent>,
    /// The tasks still running when the turn ended.
    pub background: Vec<BackgroundTask>,
    /// The first line of the newest text Claude wrote in the turn.
    pub status: Option<String>,
    /// When `status` last changed.
    pub status_at: SystemTime,
    /// The newest tool Claude used in the turn, in words, such as "Reading plan.md".
    pub activity: Option<String>,
    /// When that tool started.
    pub activity_at: SystemTime,
    /// Whether that tool is still running.
    pub tool_running: bool,
    /// Whether Claude Code is compacting the session's context, between `PreCompact` and
    /// `PostCompact`.
    pub compacting: bool,
    /// Whether the turn's last wrap-up asked you something, or `None` for a turn with no
    /// wrap-up, such as a session in a terminal, whose end counts as your turn.
    pub asks: Option<bool>,
    /// Whether you've read a turn that's your turn, by opening the session or clicking its tile.
    pub read: bool,
    /// What a working tile shows: `status` for `STATUS_HOLDS` after it changes, then `activity`
    /// while its tool runs and for at least `TOOL_HOLDS`, and `THINKING` otherwise.
    /// `refresh_line` keeps it current.
    pub line: Option<String>,
    pub pending_tool: Option<Tool>,
    /// Whether what the session waits on in `NeedsApproval` is a question for you, from Claude's
    /// `AskUserQuestion` tool or an MCP server, rather than a tool to approve.
    pub question: bool,
    /// Whether you set or canceled the tile's timer yourself during this state, which a look then
    /// leaves alone.
    pub own_timer: bool,
    pub ended_at: Option<SystemTime>,
    /// When you dismissed the session, while it waits off the rack and shows in the archive.
    pub dismissed_at: Option<SystemTime>,
}

/// One of the turn's tools in words, such as "Reading plan.md", and whether it failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recent {
    pub text: String,
    pub failed: bool,
}

impl Session {
    pub fn new(id: &str, folder: &Path, now: SystemTime) -> Self {
        Session {
            id: id.to_string(),
            folder: folder.to_path_buf(),
            transcript: None,
            app_title: None,
            app_id: None,
            app_parent: None,
            custom_title: None,
            first_prompt: None,
            state: SessionState::New,
            state_since: now,
            summary: None,
            message: None,
            wrap_up: None,
            recent: Vec::new(),
            background: Vec::new(),
            status: None,
            status_at: now,
            activity: None,
            activity_at: now,
            tool_running: false,
            compacting: false,
            asks: None,
            read: false,
            line: None,
            pending_tool: None,
            question: false,
            own_timer: false,
            ended_at: None,
            dismissed_at: None,
        }
    }

    /// Recomputes `line` for `now`, and returns whether it changed.
    pub fn refresh_line(&mut self, now: SystemTime) -> bool {
        let within =
            |at: SystemTime, hold: Duration| now.duration_since(at).is_ok_and(|d| d < hold);
        let waiting = self.waiting_on();
        let line = match (&self.status, &self.activity) {
            _ if self.compacting => COMPACTING.to_string(),
            _ if waiting == 1 => "Waiting on 1 background task".to_string(),
            _ if waiting > 1 => format!("Waiting on {waiting} background tasks"),
            (Some(status), _) if within(self.status_at, STATUS_HOLDS) => status.clone(),
            (_, Some(activity)) if self.tool_running || within(self.activity_at, TOOL_HOLDS) => {
                activity.clone()
            }
            _ => THINKING.to_string(),
        };
        let line = Some(line);
        let changed = line != self.line;
        self.line = line;
        changed
    }

    /// How many of the tasks still running at the turn's end Claude waits on to take the
    /// session up again.
    pub fn waiting_on(&self) -> usize {
        self.background.iter().filter(|t| t.reports_back()).count()
    }

    /// The title the Claude desktop app shows for the session, or else its custom title, or else
    /// the first line of its first prompt, or else, until one arrives, the name of its folder.
    pub fn title(&self) -> String {
        self.app_title
            .clone()
            .or_else(|| self.custom_title.clone())
            .or_else(|| self.first_prompt.as_deref().map(prompt_title))
            .filter(|t| !t.is_empty())
            .or_else(|| {
                self.folder
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Session".to_string())
    }
}

/// The first line of what you typed in a prompt.
/// The Claude desktop app puts blocks of context before the typed text, such as
/// `<artifact-view-context ...>...</artifact-view-context>`, so the title skips each tag block at
/// the start (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md).
pub fn prompt_title(prompt: &str) -> String {
    let mut rest = prompt.trim_start();
    while let Some(after) = rest.strip_prefix('<') {
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        let Some(open_end) = rest.find('>') else {
            break;
        };
        if name.is_empty() {
            break;
        }
        if rest[..open_end].ends_with('/') {
            rest = rest[open_end + 1..].trim_start();
            continue;
        }
        let close = format!("</{name}>");
        match rest.find(&close) {
            Some(i) => rest = rest[i + close.len()..].trim_start(),
            None => break,
        }
    }
    first_line(rest)
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
            target: None,
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
    fn a_tools_activity_names_what_it_acts_on() {
        let tool = |name: &str, target: Option<&str>, command: Option<&str>| Tool {
            name: name.into(),
            command: command.map(Into::into),
            file: None,
            target: target.map(Into::into),
        };
        assert_eq!(
            tool("Read", Some("plan.md"), None).activity(),
            "Reading plan.md"
        );
        assert_eq!(
            tool("PowerShell", Some("Restart Plates"), Some("Stop-Process")).activity(),
            "Restart Plates"
        );
        assert_eq!(
            tool("Bash", None, Some("cargo test\necho done")).activity(),
            "Running cargo test"
        );
        assert_eq!(
            tool("mcp__ccd_turn__update_plan", None, None).activity(),
            "Using update_plan"
        );
    }

    #[test]
    fn a_working_line_holds_new_text_then_shows_the_tool_then_thinking() {
        let t0 = UNIX_EPOCH + Duration::from_secs(100);
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        let mut s = Session::new("a", Path::new("/w"), t0);
        assert!(s.refresh_line(t0));
        assert_eq!(s.line.as_deref(), Some(THINKING), "before any text or tool");

        s.activity = Some("Running cargo test".into());
        s.activity_at = t0;
        s.tool_running = true;
        s.status = Some("Now the tests.".into());
        s.status_at = t0;
        s.refresh_line(at(1));
        assert_eq!(s.line.as_deref(), Some("Now the tests."));
        s.refresh_line(t0 + STATUS_HOLDS);
        assert_eq!(
            s.line.as_deref(),
            Some("Running cargo test"),
            "while it runs"
        );

        s.tool_running = false;
        s.refresh_line(at(5));
        assert_eq!(s.line.as_deref(), Some(THINKING), "once it finished");

        s.activity = Some("Reading plan.md".into());
        s.activity_at = at(10);
        s.refresh_line(at(10));
        assert_eq!(
            s.line.as_deref(),
            Some("Reading plan.md"),
            "a tool that already finished still shows for TOOL_HOLDS"
        );
        s.refresh_line(at(10) + TOOL_HOLDS);
        assert_eq!(s.line.as_deref(), Some(THINKING));
    }

    #[test]
    fn a_prompts_title_skips_the_context_blocks_before_it() {
        let prompt = "<artifact-view-context artifact=\"49dc\">\n{\"mode\":\"canvas\"}\n<artifact-content-authored-by-others/>\n</artifact-view-context>\n\nI tried it.\n\nMore";
        assert_eq!(prompt_title(prompt), "I tried it.");
        assert_eq!(
            prompt_title("<note/> <b>bold</b> rest"),
            "rest",
            "self-closing and plain blocks both go"
        );
        assert_eq!(
            prompt_title("a < b, says the prompt"),
            "a < b, says the prompt"
        );
        assert_eq!(prompt_title("<unclosed> text"), "<unclosed> text");
    }

    #[test]
    fn a_title_falls_back_from_custom_to_first_prompt_to_folder() {
        let mut s = Session::new("a", Path::new("/work/plates"), UNIX_EPOCH);
        assert_eq!(s.title(), "plates");
        s.first_prompt = Some("\n  Fix the dock hold\nand more".into());
        assert_eq!(s.title(), "Fix the dock hold");
        s.custom_title = Some("Dock tuning".into());
        assert_eq!(s.title(), "Dock tuning");
        s.app_title = Some("Tune the dock".into());
        assert_eq!(s.title(), "Tune the dock");
    }
}
