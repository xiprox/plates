//! The MCP server through which Claude keeps a session's task list, which the rack shows inside
//! the session's tile (record/areas/tasks/01-task-list.md).
//!
//! Claude Code starts one for each session and speaks MCP to it over stdin and stdout, one
//! JSON-RPC message per line. Every session keeps its server running, so this is a small program
//! of its own rather than a mode of the GPUI app.
//!
//! The server finds its session from `CLAUDE_CODE_HOST_SESSION_ID`, the Claude desktop app's ID
//! for it, which stays the same when a rewind moves the session to a new Claude Code session
//! (record/areas/tasks/log.md). A session in a terminal has no app ID, so it falls back on
//! `CLAUDE_CODE_SESSION_ID`.

// Without a console of its own, no window flashes up as each session starts.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{
    io::{BufRead, Write},
    path::Path,
    time::SystemTime,
};

use plates_core::tasks::{self, Change, Field, State, Task, TaskList};
use serde_json::{Value, json};

/// What Claude Code puts in the session's system prompt. The limits come from `Field::max`, which
/// the list checks. When you change a tool's name here, change it in `tools` too, and for `wait`
/// and `ping`, in `call` and in plates-core's `hooks::plates_call`, which reads them from the
/// hooks. When you change a rule here, change it in "How Claude keeps a list" in
/// record/areas/tasks/01-task-list.md too, or for `wait` and `ping`, in "What Claude is told" in
/// record/areas/tasks/02-waits-and-pings.md.
fn instructions() -> String {
    format!(
        "\
Plates, the rack of session tiles beside the Claude app, shows this session's task list on its tile, so the user can see at a glance who holds each piece of the work: the user, you, someone else, or nobody yet.

Start a list without being asked once you and the user agree on work that runs past the current request: at least two pieces of work, or one piece that's waiting on someone. The steps of the request you're working on don't count, since the app's own plan covers those.
When you start a list, say so in one line, such as \"Started a Plates task list: 3 tasks.\"
If the user says not to keep a list in this session, call tasks_decline.

Keep the list to big pieces of work, about 3 to 10.
Write each title as a few words that name the piece, at most {title} characters, such as \"Windows probe\" or \"Footnote style\".
Write each status as what's new about the piece, at most {status} characters: where it stands, what's next, or what it waits for, such as \"Needs a session on the PC\". Don't repeat the title, and leave the status empty when the title says enough.
When a task waits on the user, set waiting_on to \"you\". When it waits on someone else, name them in waiting_on, such as \"Dana\", not in the status.
Update a task when its state changes or there's news on it, not at every step.
When the user's message answers what a task waits on, or you start work on a task, update the task, such as setting it to doing, in the same message as that work's first tool call, so the tile shows who holds it while you work.
Before you end a turn, update each task you worked on that the list shows wrong, such as setting it to wait on the user with a status saying what they need to do, so the tile shows what's on the user while you wait.
Mark a task dropped rather than leaving it stale.
Call tasks_replace only when the user asks to start over or to replace the list.
Call tasks_get before changing a list you haven't seen in this conversation.
If an update is due before you've seen the list, call tasks_get in the message the update was due in, and send the update in your next message, since task_update needs the task's ID.
This list is the only copy of the plan's tasks: don't copy it into your memory or any other notes, since a second copy goes stale.

When you end a turn to wait on something outside this session that will start your next turn, such as a background command watching a pull request's checks, call wait with what you're waiting on, at most {on} characters, so the tile doesn't show the turn as needing the user.
When the user has asked you to tell them when something happens, call ping in the turn it happens, with what happened, at most {message} characters, so the tile rings at once. Ping only when they asked.

The tools are tasks_get, task_add, task_update, tasks_replace, tasks_decline, wait, and ping, from the plates server.",
        title = Field::Title.max(),
        status = Field::Status.max(),
        on = Field::On.max(),
        message = Field::Message.max(),
    )
}

fn main() {
    let folder = tasks::folder();
    let session = session_id();
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = respond(&line, &folder, session.as_deref(), SystemTime::now()) {
            // Claude Code has closed the pipe if this fails, and the next read ends the loop.
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

fn session_id() -> Option<String> {
    ["CLAUDE_CODE_HOST_SESSION_ID", "CLAUDE_CODE_SESSION_ID"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|id| !id.is_empty())
}

/// The reply to one line from Claude Code, or none for a notification, which gets no reply.
fn respond(line: &str, folder: &Path, session: Option<&str>, now: SystemTime) -> Option<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Some(error(Value::Null, -32700, "The line isn't JSON.".into()));
    };
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => json!({
            // Claude Code 2.1.325 asked for 2025-11-25 (record/areas/tasks/log.md), and the
            // server uses nothing that differs between versions, so it answers with the client's.
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "plates", "version": env!("CARGO_PKG_VERSION") },
            "instructions": instructions(),
        }),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let (text, failed) = match call(name, &args, folder, session, now) {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            json!({ "content": [{ "type": "text", "text": text }], "isError": failed })
        }
        "ping" => json!({}),
        // Claude Code also sends `server/discover` before `initialize`, and goes on once it
        // hears the server doesn't know it (record/areas/tasks/log.md).
        _ => {
            return Some(error(
                id,
                -32601,
                format!("The server doesn't know {method}."),
            ));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Runs one tool on the session's list, and returns what Claude reads, or why it failed.
fn call(
    name: &str,
    args: &Value,
    folder: &Path,
    session: Option<&str>,
    now: SystemTime,
) -> Result<String, String> {
    // Plates learns of these two from the hooks, so the server only checks their text and keeps
    // nothing (record/areas/tasks/02-waits-and-pings.md).
    let line = |key: &str, field: Field| {
        let text = args.get(key).and_then(Value::as_str).unwrap_or("");
        tasks::check_line(text, field).map_err(|e| e.to_string())
    };
    match name {
        "wait" => {
            let on = line("on", Field::On)?;
            return Ok(format!(
                "Unless this turn asks the user something, when it ends the tile shows the session waiting on {on}."
            ));
        }
        "ping" => {
            line("message", Field::Message)?;
            return Ok(
                "When this turn ends, the tile moves to the user's turn with this message and rings."
                    .to_string(),
            );
        }
        _ => {}
    }
    let session = session.ok_or(
        "Plates can't tell which session this is, since Claude Code passed neither \
         CLAUDE_CODE_HOST_SESSION_ID nor CLAUDE_CODE_SESSION_ID.",
    )?;
    let path = tasks::path(folder, session)
        .ok_or_else(|| format!("The session ID {session:?} can't name a file."))?;
    let mut list =
        tasks::load(&path).map_err(|e| format!("Plates couldn't read the task list: {e}"))?;
    let text = match name {
        "tasks_get" => return Ok(shown(&list)),
        "task_add" => {
            let task = list.add(change(args)?, now).map_err(|e| e.to_string())?;
            shown_task(task)
        }
        "task_update" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
                .ok_or("task_update needs the task's id, a number from tasks_get.")?;
            let task = list
                .update(id, change(args)?, now)
                .map_err(|e| e.to_string())?;
            shown_task(task)
        }
        "tasks_replace" => {
            let items = args
                .get("tasks")
                .and_then(Value::as_array)
                .ok_or("tasks_replace needs tasks, a list that may be empty.")?;
            let changes = items.iter().map(change).collect::<Result<Vec<_>, _>>()?;
            list.replace(changes, now).map_err(|e| e.to_string())?;
            shown(&list)
        }
        "tasks_decline" => {
            list.decline();
            "Recorded: no task list in this session. The tasks already on it stay.".to_string()
        }
        _ => return Err(format!("The plates server has no tool named {name}.")),
    };
    tasks::save(&list, &path).map_err(|e| format!("Plates couldn't save the task list: {e}"))?;
    Ok(text)
}

/// A task's fields from a tool's arguments, each left out where Claude left it out.
fn change(args: &Value) -> Result<Change, String> {
    let text = |key: &str| -> Result<Option<String>, String> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("{key} needs to be text.")),
        }
    };
    let state = match text("state")? {
        Some(name) => Some(State::parse(&name).map_err(|e| e.to_string())?),
        None => None,
    };
    Ok(Change {
        title: text("title")?,
        state,
        status: text("status")?,
        waiting_on: text("waiting_on")?,
    })
}

fn task_json(task: &Task) -> Value {
    json!({
        "id": task.id,
        "title": task.title,
        "state": task.state,
        "status": task.status,
        "waiting_on": task.waiting_on,
        "updated": tasks::iso(task.updated),
    })
}

fn shown_task(task: &Task) -> String {
    serde_json::to_string_pretty(&task_json(task)).expect("a task should serialize")
}

fn shown(list: &TaskList) -> String {
    let replaced = list.replaced.as_ref().map(|r| {
        json!({
            "at": tasks::iso(r.at),
            "tasks": r.tasks.iter().map(task_json).collect::<Vec<_>>(),
        })
    });
    let shown = json!({
        "declined": list.declined,
        "tasks": list.tasks.iter().map(task_json).collect::<Vec<_>>(),
        "replaced": replaced,
    });
    serde_json::to_string_pretty(&shown).expect("a list should serialize")
}

/// The tools Claude Code lists for Claude. When you rename one, rename it in `instructions` too.
fn tools() -> Value {
    let fields = json!({
        "title": {
            "type": "string",
            "maxLength": Field::Title.max(),
            "description": "A few words naming the piece of work, such as \"Windows probe\".",
        },
        "state": {
            "type": "string",
            "enum": State::NAMES,
            "description": "todo, doing, waiting (on someone or something), done, or dropped.",
        },
        "status": {
            "type": "string",
            "maxLength": Field::Status.max(),
            "description": "What's new about the task: where it stands, what's next, or what it waits for, such as \"Needs a session on the PC\". Doesn't repeat the title. Replaces the previous status.",
        },
        "waiting_on": {
            "type": "string",
            "maxLength": Field::WaitingOn.max(),
            "description": "Who or what a waiting task waits on: \"you\" for the user, or a name, such as \"Dana\".",
        },
    });
    let mut update = fields.clone();
    update["id"] = json!({ "type": "integer", "description": "The task's id, from tasks_get." });
    json!([
        {
            "name": "tasks_get",
            "description": "Returns this session's Plates task list, and the last list tasks_replace replaced.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "task_add",
            "description": "Adds a task to this session's Plates task list, starting the list if there's none.",
            "inputSchema": { "type": "object", "properties": fields, "required": ["title"] },
        },
        {
            "name": "task_update",
            "description": "Changes a task's title, state, status, or waiting_on. Fields left out keep their values.",
            "inputSchema": { "type": "object", "properties": update, "required": ["id"] },
        },
        {
            "name": "tasks_replace",
            "description": "Replaces the whole list, keeping the old one, which tasks_get shows. An empty list wipes it. Only when the user asks to start over or to replace the list.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": { "type": "object", "properties": fields, "required": ["title"] },
                    },
                },
                "required": ["tasks"],
            },
        },
        {
            "name": "tasks_decline",
            "description": "Records that the user doesn't want a task list in this session, so later tasks are refused.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "wait",
            "description": "Says this turn ends waiting on something outside the session that will start the next turn, such as a background command watching CI, so the session's tile waits without asking for the user.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "on": {
                        "type": "string",
                        "maxLength": Field::On.max(),
                        "description": "What the session waits on, in a word or two, such as \"CI checks\".",
                    },
                },
                "required": ["on"],
            },
        },
        {
            "name": "ping",
            "description": "Says this turn ends with something the user asked to hear about, so the session's tile moves to the user's turn with the message and rings at once.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "maxLength": Field::Message.max(),
                        "description": "What happened, such as \"Checks passed; ready to merge #12\".",
                    },
                },
                "required": ["message"],
            },
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, UNIX_EPOCH},
    };

    const SESSION: &str = "local_0f1e2d3c-aaaa-bbbb-cccc-0123456789ab";

    // A folder of the test's own under the OS's temporary folder, emptied first.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plates-tasks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_795_174_931)
    }

    fn send(folder: &Path, id: u64, method: &str, params: Value) -> Value {
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        respond(&line.to_string(), folder, Some(SESSION), now()).expect("a request gets a reply")
    }

    fn tool(folder: &Path, name: &str, args: Value) -> (String, bool) {
        let reply = send(
            folder,
            1,
            "tools/call",
            json!({ "name": name, "arguments": args }),
        );
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap(),
        )
    }

    #[test]
    fn initialize_answers_with_the_instructions_and_the_clients_version() {
        let dir = folder("init");
        let reply = send(
            &dir,
            1,
            "initialize",
            json!({ "protocolVersion": "2025-11-25" }),
        );
        assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(reply["result"]["serverInfo"]["name"], "plates");
        assert!(
            reply["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("tasks_decline")
        );
    }

    #[test]
    fn wait_and_ping_check_their_text_and_keep_nothing() {
        let dir = folder("wait");
        let (text, failed) = tool(&dir, "wait", json!({ "on": "CI checks" }));
        assert!(!failed, "{text}");
        assert!(text.contains("CI checks"));
        let (_, failed) = tool(&dir, "ping", json!({ "message": "Checks passed" }));
        assert!(!failed);
        let (text, failed) = tool(&dir, "wait", json!({ "on": "" }));
        assert!(failed && text.contains("wait needs on"), "{text}");
        let (text, failed) = tool(&dir, "ping", json!({ "message": "x".repeat(41) }));
        assert!(failed && text.contains("40 characters"), "{text}");
        assert!(!dir.exists() || fs::read_dir(&dir).unwrap().next().is_none());
    }

    #[test]
    fn every_tool_the_instructions_name_is_listed() {
        let dir = folder("list");
        let reply = send(&dir, 2, "tools/list", json!({}));
        let names: Vec<&str> = reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for name in &names {
            assert!(
                instructions().contains(name),
                "{name} isn't in the instructions"
            );
        }
        assert_eq!(names.len(), 7);
        for word in instructions().split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if word.starts_with("task_") || word.starts_with("tasks_") {
                assert!(
                    names.contains(&word),
                    "the instructions name {word}, which isn't listed"
                );
            }
        }
    }

    #[test]
    fn a_notification_gets_no_reply_and_an_unknown_method_an_error() {
        let dir = folder("notify");
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert_eq!(respond(&note.to_string(), &dir, Some(SESSION), now()), None);
        let reply = send(&dir, 3, "server/discover", json!({}));
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[test]
    fn tools_keep_the_list_in_the_sessions_file() {
        let dir = folder("tools");
        let (text, failed) = tool(
            &dir,
            "task_add",
            json!({ "title": "Write the design", "state": "doing" }),
        );
        assert!(!failed, "{text}");
        let (_, failed) = tool(
            &dir,
            "task_update",
            json!({ "id": 1, "state": "waiting", "waiting_on": "you" }),
        );
        assert!(!failed);
        let saved = tasks::load(&tasks::path(&dir, SESSION).unwrap()).unwrap();
        assert!(saved.tasks[0].waits_on_you());
        let (text, _) = tool(&dir, "tasks_get", json!({}));
        assert!(
            text.contains("\"updated\": \"2026-11-20T11:42:11Z\""),
            "{text}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_call_is_an_error_claude_reads_and_changes_nothing() {
        let dir = folder("bad");
        let (text, failed) = tool(
            &dir,
            "task_add",
            json!({ "title": "x", "state": "blocked" }),
        );
        assert!(failed);
        assert!(
            text.contains("todo, doing, waiting, done, dropped"),
            "{text}"
        );
        let (_, failed) = tool(&dir, "task_update", json!({ "state": "done" }));
        assert!(failed);
        assert!(!tasks::path(&dir, SESSION).unwrap().exists());
    }

    #[test]
    fn a_declined_session_refuses_new_tasks() {
        let dir = folder("decline");
        tool(&dir, "tasks_decline", json!({}));
        let (text, failed) = tool(&dir, "task_add", json!({ "title": "More" }));
        assert!(failed);
        assert!(text.contains("not to keep a task list"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_session_id_every_tool_says_why_it_cant_work() {
        let dir = folder("nosession");
        let line = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "tasks_get", "arguments": {} },
        });
        let reply = respond(&line.to_string(), &dir, None, now()).unwrap();
        assert_eq!(reply["result"]["isError"], true);
    }
}
