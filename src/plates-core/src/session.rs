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
    /// Claude ended a turn waiting on something outside the session, such as a pull request's
    /// checks, which starts its next turn, so the tile waits without needing you
    /// (record/areas/tasks/02-waits-and-pings.md). Its `summary` is what it waits on.
    Waiting,
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
    /// `PreCompact`, with whether you typed the `/compact` rather than Claude Code starting it on
    /// its own.
    CompactStarted { manual: bool },
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
    /// `PreToolUse` for the Claude desktop app's `mcp__ccd_turn__update_plan`, which changes the
    /// plan Claude keeps for a request.
    PlanChanged(PlanChange),
    /// `PreToolUse` for the plates server's `wait`: the turn ends waiting on `on`.
    WaitCalled { on: String },
    /// `PreToolUse` for the plates server's `ping`: the turn ends with news you requested a ping
    /// for.
    PingCalled { message: String },
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
    /// Whether this is a prompt you typed, rather than one that carries only the app's context,
    /// such as a task notification's, or the one `hooks::catch_up` makes up for a turn.
    pub fn is_typed_prompt(&self) -> bool {
        matches!(
            self,
            SessionEvent::PromptSubmitted { prompt: Some(prompt), .. } if !prompt_title(prompt).is_empty()
        )
    }

    /// The state this event moves a tile to, if it moves one.
    pub fn moves_to(&self) -> Option<SessionState> {
        match self {
            SessionEvent::Started { .. } => Some(SessionState::New),
            SessionEvent::PromptSubmitted { .. }
            | SessionEvent::CompactStarted { manual: true } => Some(SessionState::Working),
            SessionEvent::CompactStarted { manual: false }
            | SessionEvent::ToolPending { .. }
            | SessionEvent::ToolFinished { .. }
            | SessionEvent::CompactFinished { .. }
            | SessionEvent::InputGiven
            | SessionEvent::WrappedUp(_)
            | SessionEvent::PlanChanged(_)
            | SessionEvent::WaitCalled { .. }
            | SessionEvent::PingCalled { .. }
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

/// The most steps the Claude desktop app keeps in a plan.
pub const PLAN_STEPS: usize = 12;

/// The Claude desktop app's tool for the plan Claude keeps for a request.
pub const UPDATE_PLAN: &str = "mcp__ccd_turn__update_plan";

/// Whether a plan has no step pending, so a new turn drops it: a finished request leaves none
/// pending (record/areas/hooks/01-claude-code-hooks.md, "The app's plan").
pub fn finished(plan: &[Step]) -> bool {
    !plan.iter().any(|step| step.status == StepStatus::Pending)
}

/// One step of the plan the Claude desktop app has Claude keep for a request, which the app shows
/// under Claude's latest message (record/areas/hooks/01-claude-code-hooks.md, "The app's plan").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub text: String,
    pub status: StepStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
}

/// A call to the app's `update_plan`. Plates changes its copy of the plan the way the app changes
/// its own, so the hover card shows the steps the app does (read in the Claude desktop app
/// 6.28899.2.0, app.asar, its handler for `update_plan`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanChange {
    /// `items`: the whole plan, which is empty for an `items` the app refuses, since the app
    /// clears its plan before it checks one.
    Replace(Vec<Step>),
    /// `done`, `start`, and `advance`, whose step numbers count from 1. A number that isn't a
    /// whole number of at least 1 is kept as 0, which names no step.
    Update {
        done: Vec<usize>,
        start: Option<usize>,
        advance: bool,
    },
}

impl PlanChange {
    /// Reads `update_plan`'s input, or `None` for a call the app refuses without changing its
    /// plan, one that passes none of `items`, `done`, `start`, and `advance`.
    pub fn read(input: &Value) -> Option<PlanChange> {
        if let Some(items) = input.get("items") {
            let scoped = matches!(
                input.get("scope").and_then(Value::as_str),
                Some("first_task" | "continuation" | "new_task")
            );
            // Claude can pass the list as a JSON string rather than a list, as with the wrap-up's
            // `actions`.
            let items = match items {
                Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                v => v.clone(),
            };
            let steps = items
                .as_array()
                .filter(|_| scoped)
                .into_iter()
                .flatten()
                .filter_map(|item| {
                    let text = first_line(item.get("content")?.as_str()?);
                    let status = match item.get("status").and_then(Value::as_str) {
                        Some("completed") => StepStatus::Completed,
                        Some("in_progress") => StepStatus::InProgress,
                        _ => StepStatus::Pending,
                    };
                    (!text.is_empty()).then_some(Step { text, status })
                })
                .take(PLAN_STEPS)
                .collect();
            return Some(PlanChange::Replace(steps));
        }
        let number = |v: &Value| {
            v.as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= 1.0)
                .map_or(0, |n| n as usize)
        };
        let done: Vec<usize> = input
            .get("done")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(number)
            .collect();
        let start = input.get("start").map(number);
        let advance = matches!(input.get("advance"), Some(Value::Bool(true)))
            || input.get("advance").and_then(Value::as_str) == Some("true");
        (advance || !done.is_empty() || start.is_some()).then_some(PlanChange::Update {
            done,
            start,
            advance,
        })
    }

    /// Changes `plan` as the app changes its own: `done`, then `start`, then `advance`, and
    /// nothing at all when a number names no step.
    pub fn apply(self, plan: &mut Vec<Step>) {
        let (done, start, advance) = match self {
            PlanChange::Replace(steps) => {
                *plan = steps;
                return;
            }
            PlanChange::Update {
                done,
                start,
                advance,
            } => (done, start, advance),
        };
        let len = plan.len();
        if !done.iter().chain(&start).all(|n| (1..=len).contains(n)) {
            return;
        }
        for n in done {
            plan[n - 1].status = StepStatus::Completed;
        }
        if let Some(n) = start {
            plan[n - 1].status = StepStatus::InProgress;
        }
        if advance {
            let current = plan.iter().position(|s| s.status == StepStatus::InProgress);
            if let Some(i) = current {
                plan[i].status = StepStatus::Completed;
            }
            let pending = |from: usize| {
                plan.iter()
                    .skip(from)
                    .position(|s| s.status == StepStatus::Pending)
                    .map(|i| from + i)
            };
            if let Some(next) = pending(current.map_or(0, |i| i + 1)).or_else(|| pending(0)) {
                plan[next].status = StepStatus::InProgress;
            }
        }
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
    /// What the turn's `wait` call waits on, which its `Stop` turns into the waiting state.
    pub wait: Option<String>,
    /// The turn's `ping` message, which its `Stop` turns into your turn with a ring.
    pub ping: Option<String>,
    /// When a ping last moved the tile to your turn, which the app rings its tone for.
    pub pinged_at: Option<SystemTime>,
    /// The turn's newest tools, oldest first, at most `RECENT_TOOLS` of them.
    pub recent: Vec<Recent>,
    /// The app's plan for the request, which a working tile's hover card shows in place of
    /// `recent`, or empty for none.
    pub plan: Vec<Step>,
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
    /// Whether Claude Code is compacting the session's context in a running turn: from a
    /// `PreCompact` for a `/compact` you typed, or one Claude Code started while a turn ran, to
    /// its `PostCompact`, or else to the session's next prompt, tool, or turn's end.
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
    /// The latest time the Claude desktop app says you opened the session, which the tile's
    /// task list compares each task's last change with, to mark what changed since you were last
    /// there. A session the app doesn't keep, such as one in a terminal, has none.
    pub seen_at: Option<SystemTime>,
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
            wait: None,
            ping: None,
            pinged_at: None,
            recent: Vec::new(),
            plan: Vec::new(),
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
            seen_at: None,
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

    fn plan_of(statuses: &[StepStatus]) -> Vec<Step> {
        statuses
            .iter()
            .enumerate()
            .map(|(i, &status)| Step {
                text: format!("Step {}", i + 1),
                status,
            })
            .collect()
    }

    fn statuses(plan: &[Step]) -> Vec<StepStatus> {
        plan.iter().map(|s| s.status).collect()
    }

    fn change(plan: &mut Vec<Step>, input: serde_json::Value) {
        PlanChange::read(&input)
            .expect("the input should be a change")
            .apply(plan);
    }

    #[test]
    fn items_replace_the_plan_with_their_steps_and_ignore_an_update_beside_them() {
        let mut plan = plan_of(&[StepStatus::Completed]);
        let input = serde_json::json!({
            "items": [
                { "content": " Read how the card works ", "status": "completed" },
                { "content": "Draw the plan", "status": "in_progress" },
                { "content": "   ", "status": "pending" },
                { "content": "Test it", "status": "blocked" },
                "not a step",
            ],
            "scope": "continuation",
            "advance": true,
        });
        change(&mut plan, input);
        let texts: Vec<&str> = plan.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Read how the card works", "Draw the plan", "Test it"],
            "a step with no text is left out"
        );
        assert_eq!(
            statuses(&plan),
            [
                StepStatus::Completed,
                StepStatus::InProgress,
                StepStatus::Pending
            ],
            "a status the app doesn't know is pending, and the advance changed nothing"
        );

        let many: Vec<_> = (0..15)
            .map(|i| serde_json::json!({ "content": format!("Step {i}"), "status": "pending" }))
            .collect();
        change(
            &mut plan,
            serde_json::json!({ "items": many, "scope": "new_task" }),
        );
        assert_eq!(plan.len(), PLAN_STEPS);

        change(
            &mut plan,
            serde_json::json!({ "items": [{ "content": "Go", "status": "pending" }] }),
        );
        assert!(
            plan.is_empty(),
            "the app refuses items with no scope, and has cleared its plan by then"
        );
    }

    #[test]
    fn an_update_marks_steps_as_the_app_does() {
        use StepStatus::{Completed as C, InProgress as I, Pending as P};
        let mut plan = plan_of(&[I, P, P, P]);
        change(&mut plan, serde_json::json!({ "advance": true }));
        assert_eq!(statuses(&plan), [C, I, P, P]);

        change(&mut plan, serde_json::json!({ "done": [2, 3], "start": 4 }));
        assert_eq!(statuses(&plan), [C, C, C, I]);

        // With the last step in progress, advance finishes it and wraps to the first pending one.
        let mut plan = plan_of(&[P, C, I]);
        change(&mut plan, serde_json::json!({ "advance": "true" }));
        assert_eq!(statuses(&plan), [I, C, C]);

        // A number that names no step changes nothing, not even the steps the others name.
        let mut plan = plan_of(&[I, P]);
        change(&mut plan, serde_json::json!({ "done": [1, 3] }));
        assert_eq!(statuses(&plan), [I, P]);
        change(&mut plan, serde_json::json!({ "done": [1], "start": 1.5 }));
        assert_eq!(statuses(&plan), [I, P]);

        let mut none = Vec::new();
        change(&mut none, serde_json::json!({ "advance": true }));
        assert!(
            none.is_empty(),
            "an update with no plan has nothing to change"
        );

        assert_eq!(PlanChange::read(&serde_json::json!({ "done": [] })), None);
        assert_eq!(
            PlanChange::read(&serde_json::json!({ "advance": false })),
            None
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
