//! Adding Plates' hooks to Claude Code's user settings, and its task server to the MCP servers
//! Claude Code starts for every project, and taking both out again.
//!
//! Each hook is an async command hook in exec form, which runs `plates-hook` with no shell and
//! doesn't wait for it, so a closed Plates never slows a session (record/areas/hooks/log.md).
//! A hook is Plates' own when it runs a program named `plates-hook`, or when it's an `http` hook
//! to `receiver::PATH` on `127.0.0.1` from an earlier install, so uninstalling leaves every other
//! hook as it was.
//!
//! A write of Plates' to `.claude.json`, the file that registers the task server, could be lost
//! to one of Claude Code's, which writes the file while its sessions run (unverified,
//! record/areas/tasks/log.md). So Claude Code's own `claude mcp` command registers the server,
//! and this module only reads the file, to choose the command.

use std::path::Path;

use serde_json::{Map, Value, json};

use crate::receiver::PATH;

/// The hook events Plates listens to, with the matcher each takes, from
/// record/areas/hooks/01-claude-code-hooks.md. A `Notification` hook matches only
/// `permission_prompt`, since Plates ignores every other notification.
pub const EVENTS: [(&str, Option<&str>); 14] = [
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("PreToolUse", None),
    ("PermissionRequest", None),
    ("Notification", Some("permission_prompt")),
    ("PostToolUse", None),
    ("PostToolUseFailure", None),
    ("Stop", None),
    ("StopFailure", None),
    ("SessionEnd", None),
    ("PreCompact", None),
    ("PostCompact", None),
    ("Elicitation", None),
    ("ElicitationResult", None),
];

fn is_ours(hook: &Value) -> bool {
    let field = |key: &str| hook.get(key).and_then(Value::as_str);
    match field("type") {
        Some("command") => field("command").is_some_and(|c| {
            Path::new(c)
                .file_stem()
                .is_some_and(|stem| stem == "plates-hook")
        }),
        Some("http") => {
            field("url").is_some_and(|u| u.starts_with("http://127.0.0.1:") && u.ends_with(PATH))
        }
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// The settings file isn't a JSON object, or its `hooks` isn't one.
    NotAnObject(&'static str),
    /// `.claude.json` registers a server named `SERVER` that runs this program, which isn't
    /// Plates' task server.
    NameTaken(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NotAnObject(what) => write!(f, "{what} isn't a JSON object"),
            InstallError::NameTaken(program) => write!(
                f,
                "a server named {SERVER} that runs {program:?} is already registered, and it isn't Plates'. \
                 If you don't need it, remove it with `claude mcp remove --scope user {SERVER}`, then install again."
            ),
        }
    }
}

/// The name Plates' task server is registered under. Claude Code names the server's tools after
/// it, as in `mcp__plates__wait`, which `hooks::plates_call` reads, so change both together.
pub const SERVER: &str = "plates";

/// What `claude mcp` has to do to `.claude.json` for an install or an uninstall.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ServerStep {
    Nothing,
    Add,
    /// Remove the server, then add it again, as when Plates moved.
    Replace,
    Remove,
}

fn is_tasks_program(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .is_some_and(|stem| stem == "plates-tasks")
}

/// The step that leaves `.claude.json`, Claude Code's file that holds the MCP servers registered
/// for every project, registering Plates' server to run `program`, or with `None`, registering
/// no server of Plates'. An uninstall leaves a server of someone else's named `SERVER` as it is,
/// and an install refuses to replace it.
pub fn server_step(
    claude_json: &Value,
    program: Option<&Path>,
) -> Result<ServerStep, InstallError> {
    let Some(entry) = claude_json.get("mcpServers").and_then(|s| s.get(SERVER)) else {
        return Ok(if program.is_some() {
            ServerStep::Add
        } else {
            ServerStep::Nothing
        });
    };
    let command = entry.get("command").and_then(Value::as_str).unwrap_or("");
    let no_args = entry
        .get("args")
        .and_then(Value::as_array)
        .is_none_or(|a| a.is_empty());
    if !is_tasks_program(command) {
        return match program {
            Some(_) => Err(InstallError::NameTaken(command.to_string())),
            None => Ok(ServerStep::Nothing),
        };
    }
    Ok(match program {
        None => ServerStep::Remove,
        Some(p) if command == p.to_string_lossy() && no_args => ServerStep::Nothing,
        Some(_) => ServerStep::Replace,
    })
}

impl std::error::Error for InstallError {}

/// The settings with every hook of Plates' taken out, dropping groups and events left empty.
pub fn without(settings: &Value) -> Result<Value, InstallError> {
    let mut settings = settings.clone();
    let root = settings
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("the settings file"))?;
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(settings);
    };
    let events = hooks
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("`hooks`"))?;
    for groups in events.values_mut() {
        let Some(list) = groups.as_array_mut() else {
            continue;
        };
        for group in list.iter_mut() {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|h| !is_ours(h));
            }
        }
        list.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|h| !h.is_empty())
        });
    }
    events.retain(|_, groups| groups.as_array().is_none_or(|l| !l.is_empty()));
    if events.is_empty() {
        root.remove("hooks");
    }
    Ok(settings)
}

/// Plates' hooks alone, each running the `plates-hook` at `hook_program`, which
/// `plates hooks install` shows you before it writes them.
pub fn ours(hook_program: &Path) -> Map<String, Value> {
    let mut events = Map::new();
    for (event, matcher) in EVENTS {
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), json!(m));
        }
        group.insert(
            "hooks".into(),
            json!([{
                "type": "command",
                "command": hook_program.to_string_lossy(),
                "args": [],
                "async": true,
            }]),
        );
        events.insert(event.into(), json!([Value::Object(group)]));
    }
    events
}

/// The settings with Plates' hooks added, replacing any Plates hooks already there, and leaving
/// every other hook as it was.
pub fn with(settings: &Value, hook_program: &Path) -> Result<Value, InstallError> {
    let mut settings = without(settings)?;
    let root = settings
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("the settings file"))?;
    let events = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("`hooks`"))?;
    for (event, groups) in ours(hook_program) {
        let list = events.entry(event).or_insert_with(|| json!([]));
        if let (Some(list), Some(new)) = (list.as_array_mut(), groups.as_array()) {
            list.extend(new.iter().cloned());
        }
    }
    Ok(settings)
}

/// Plates' hooks in `settings`, in the shape `ours` gives them: for each event, the groups that
/// hold a hook of Plates', each with only Plates' hooks in it. Comparing the two tells whether
/// the settings hold this Plates' hooks whatever other hooks sit beside them.
pub fn installed(settings: &Value) -> Map<String, Value> {
    let mut events = Map::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return events;
    };
    for (event, groups) in hooks {
        let Some(groups) = groups.as_array() else {
            continue;
        };
        let found: Vec<Value> = groups
            .iter()
            .filter_map(|group| {
                let handlers: Vec<Value> = group
                    .get("hooks")?
                    .as_array()?
                    .iter()
                    .filter(|h| is_ours(h))
                    .cloned()
                    .collect();
                if handlers.is_empty() {
                    return None;
                }
                let mut found = Map::new();
                if let Some(matcher) = group.get("matcher") {
                    found.insert("matcher".into(), matcher.clone());
                }
                found.insert("hooks".into(), Value::Array(handlers));
                Some(Value::Object(found))
            })
            .collect();
        if !found.is_empty() {
            events.insert(event.clone(), Value::Array(found));
        }
    }
    events
}

/// Whether Claude Code's settings connect it to the Plates whose programs are `plates-hook` at
/// `hook_program` and `plates-tasks` at `tasks_program`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Connection {
    /// No hook or task server of Plates' is there.
    Off,
    /// Every hook `ours` gives is there, and nothing else of Plates', and the task server runs
    /// `tasks_program`.
    On,
    /// Some of Plates' hooks or its task server are there, but not as this Plates writes them, as
    /// after Plates moved, or once a newer Plates listens to another hook event.
    Stale,
}

/// How `settings`, Claude Code's user settings, and `claude_json`, the file that registers MCP
/// servers for every project, stand toward this Plates.
pub fn connection(
    settings: &Value,
    claude_json: &Value,
    hook_program: &Path,
    tasks_program: &Path,
) -> Connection {
    let hooks = installed(settings);
    let server_current = server_step(claude_json, Some(tasks_program)) == Ok(ServerStep::Nothing);
    let server_there = server_step(claude_json, None) == Ok(ServerStep::Remove);
    if hooks == ours(hook_program) && server_current {
        Connection::On
    } else if hooks.is_empty() && !server_there {
        Connection::Off
    } else {
        Connection::Stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "/opt/plates/plates-hook";

    fn theirs() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [ { "type": "command", "command": "check.sh" } ] }
                ]
            }
        })
    }

    #[test]
    fn installing_adds_one_async_hook_per_event_and_keeps_theirs() {
        let s = with(&theirs(), Path::new(PROGRAM)).unwrap();
        assert_eq!(s["model"], "opus");
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], "check.sh");
        assert_eq!(
            pre[1]["hooks"][0],
            json!({ "type": "command", "command": PROGRAM, "args": [], "async": true })
        );
        assert_eq!(
            s["hooks"]["Notification"][0]["matcher"],
            "permission_prompt"
        );
        assert_eq!(s["hooks"].as_object().unwrap().len(), EVENTS.len());
    }

    #[test]
    fn installing_twice_leaves_one_set() {
        let once = with(&theirs(), Path::new(PROGRAM)).unwrap();
        let twice = with(&once, Path::new(PROGRAM)).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn installing_replaces_an_earlier_installs_http_hooks() {
        let mut earlier = theirs();
        earlier["hooks"]["Stop"] = json!([{ "hooks": [
            { "type": "http", "url": "http://127.0.0.1:47231/plates/hook", "timeout": 2 }
        ] }]);
        let s = with(&earlier, Path::new(PROGRAM)).unwrap();
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["type"], "command");
    }

    #[test]
    fn uninstalling_takes_out_only_plates_hooks() {
        let installed = with(&theirs(), Path::new(PROGRAM)).unwrap();
        assert_eq!(without(&installed).unwrap(), theirs());
        let bare = with(&json!({}), Path::new(PROGRAM)).unwrap();
        assert_eq!(without(&bare).unwrap(), json!({}));
    }

    const SERVER_PROGRAM: &str = "/opt/plates/plates-tasks";

    // `.claude.json` with a server named `plates` as `claude mcp add --scope user` writes it
    // (Claude Code 2.1.284, record/areas/tasks/log.md).
    fn registered(command: &str) -> Value {
        json!({
            "userID": "x",
            "mcpServers": {
                "other": { "type": "stdio", "command": "other-server", "args": [], "env": {} },
                "plates": { "type": "stdio", "command": command, "args": [], "env": {} },
            },
        })
    }

    #[test]
    fn installing_the_server_adds_it_once_and_replaces_one_that_moved() {
        let program = Some(Path::new(SERVER_PROGRAM));
        assert_eq!(server_step(&json!({}), program), Ok(ServerStep::Add));
        assert_eq!(
            server_step(&json!({ "mcpServers": {} }), program),
            Ok(ServerStep::Add)
        );
        assert_eq!(
            server_step(&registered(SERVER_PROGRAM), program),
            Ok(ServerStep::Nothing)
        );
        assert_eq!(
            server_step(&registered("/old/place/plates-tasks"), program),
            Ok(ServerStep::Replace)
        );
    }

    #[test]
    fn uninstalling_removes_only_plates_server() {
        assert_eq!(
            server_step(&registered(SERVER_PROGRAM), None),
            Ok(ServerStep::Remove)
        );
        assert_eq!(server_step(&json!({}), None), Ok(ServerStep::Nothing));
        assert_eq!(
            server_step(&registered("someone-elses-server"), None),
            Ok(ServerStep::Nothing)
        );
    }

    #[test]
    fn the_connection_is_on_only_with_every_hook_and_the_server_this_plates_writes() {
        let hook = Path::new(PROGRAM);
        let tasks = Path::new(SERVER_PROGRAM);
        let on = with(&theirs(), hook).unwrap();
        let state =
            |settings: &Value, claude_json: &Value| connection(settings, claude_json, hook, tasks);
        assert_eq!(state(&theirs(), &json!({})), Connection::Off);
        assert_eq!(state(&on, &registered(SERVER_PROGRAM)), Connection::On);

        // A hook of the user's added after Plates' leaves it on.
        let mut theirs_after = on.clone();
        theirs_after["hooks"]["Stop"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "hooks": [ { "type": "command", "command": "after.sh" } ] }));
        assert_eq!(
            state(&theirs_after, &registered(SERVER_PROGRAM)),
            Connection::On
        );

        assert_eq!(
            state(&on, &json!({})),
            Connection::Stale,
            "hooks without the server"
        );
        assert_eq!(
            state(&theirs(), &registered(SERVER_PROGRAM)),
            Connection::Stale,
            "the server without hooks"
        );
        let moved = with(&theirs(), Path::new("/old/place/plates-hook")).unwrap();
        assert_eq!(
            state(&moved, &registered(SERVER_PROGRAM)),
            Connection::Stale,
            "hooks running a plates-hook that moved"
        );
        let mut fewer = on.clone();
        fewer["hooks"]
            .as_object_mut()
            .unwrap()
            .remove("PostCompact");
        assert_eq!(
            state(&fewer, &registered(SERVER_PROGRAM)),
            Connection::Stale,
            "hooks from a Plates that listened to fewer events"
        );
        assert_eq!(
            state(&theirs(), &registered("someone-elses-server")),
            Connection::Off,
            "someone else's server named plates isn't Plates'"
        );
    }

    #[test]
    fn installing_refuses_to_replace_someone_elses_server_named_plates() {
        assert_eq!(
            server_step(
                &registered("someone-elses-server"),
                Some(Path::new(SERVER_PROGRAM))
            ),
            Err(InstallError::NameTaken("someone-elses-server".into()))
        );
    }

    #[test]
    fn a_settings_file_that_isnt_an_object_is_refused() {
        assert_eq!(
            with(&json!([1, 2]), Path::new(PROGRAM)),
            Err(InstallError::NotAnObject("the settings file"))
        );
    }
}
