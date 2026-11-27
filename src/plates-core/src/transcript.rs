//! Reading a session's first prompt from its transcript, for a session Plates first saw after
//! that prompt.
//!
//! A transcript is JSON Lines, one record per line. A prompt a person typed is a record with
//! `type` `user`, `origin.kind` `human`, and `isSidechain` false, whose `message.content` is a
//! string or a list of parts, and the Claude desktop app can put a `<system-reminder>` part
//! before the prompt. (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md)

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

use crate::session::{PlanChange, Step, UPDATE_PLAN, WrapUp, finished};

/// Records past this many lines are left unread, since the first prompt comes near the top.
const MAX_LINES: usize = 2000;

pub fn first_prompt(path: &Path) -> io::Result<Option<String>> {
    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines().take(MAX_LINES) {
        let line = line?;
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(prompt) = typed_prompt(&record) {
            return Ok(Some(prompt));
        }
    }
    Ok(None)
}

/// How much of a transcript's end `latest_status` reads, which holds the newest records of even a
/// long turn.
const TAIL_BYTES: u64 = 256 * 1024;

/// The newest text Claude wrote since the last prompt you typed, read from the transcript's end,
/// for a working tile's status line. Claude Code writes the transcript as it goes, so the newest
/// text can be a record or two behind (documented, https://code.claude.com/docs/en/hooks.md).
/// An assistant record holds one part, `text`, `thinking`, or `tool_use`
/// (measured on Windows 11 25H2, 2026-11-15, Claude Code 2.1.325).
pub fn latest_status(path: &Path) -> io::Result<Option<String>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A read that starts mid-file starts mid-line, so its first line is left out.
    let lines = text.lines().skip(usize::from(start > 0));
    for line in lines.collect::<Vec<_>>().into_iter().rev() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if typed_prompt(&record).is_some() {
            return Ok(None);
        }
        if let Some(said) = assistant_text(&record) {
            return Ok(Some(said));
        }
    }
    Ok(None)
}

/// Where a session's newest turn stands, read from the transcript's end, for catching up on a
/// session whose events came while Plates was closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Turn {
    /// Claude ended the turn, with its newest text and the turn's last wrap-up, or `None` for a
    /// turn with no wrap-up.
    Finished {
        last_text: Option<String>,
        wrap_up: Option<WrapUp>,
    },
    /// You interrupted Claude, or denied a tool, and whether it was a denial.
    Interrupted { denied: bool },
    /// The turn goes on.
    Running,
}

/// The newest turn in the transcript, and the time of the record that says where it stands:
/// Claude's record with a `stop_reason` of `end_turn` for a finished turn, the marker record for
/// an interrupted one, and the newest record you or Claude added for one that goes on.
/// (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md)
pub fn newest_turn(path: &Path) -> io::Result<Option<(Turn, SystemTime)>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let records: Vec<Value> = text
        .lines()
        .skip(usize::from(start > 0))
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|r| {
            let kind = r.get("type").and_then(Value::as_str);
            matches!(kind, Some("user" | "assistant"))
                && r.get("isSidechain").and_then(Value::as_bool) != Some(true)
        })
        .collect();
    let Some(newest) = records.last() else {
        return Ok(None);
    };
    let Some(at) = newest
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
    else {
        return Ok(None);
    };
    let stop_reason = newest
        .get("message")
        .and_then(|m| m.get("stop_reason"))
        .and_then(Value::as_str);
    let turn = if is_marker(newest) {
        let denied = records.len() >= 2
            && records[records.len() - 2]
                .get("toolDenialKind")
                .and_then(Value::as_str)
                == Some("user-rejected");
        Turn::Interrupted { denied }
    } else if newest.get("type").and_then(Value::as_str) == Some("assistant")
        && stop_reason == Some("end_turn")
    {
        let turn: Vec<&Value> = records
            .iter()
            .rev()
            .take_while(|r| typed_prompt(r).is_none())
            .collect();
        Turn::Finished {
            last_text: turn.iter().find_map(|r| assistant_text(r)),
            wrap_up: turn.iter().find_map(|r| wrap_up(r)),
        }
    } else {
        Turn::Running
    };
    Ok(Some((turn, at)))
}

/// The app's plan as the transcript's calls to `update_plan` leave it, for a session whose plan
/// Plates lost when it closed. A plan can begin many turns back, so this reads the whole
/// transcript, and JSON-parses only the records that could matter: a call, or a prompt that starts
/// a turn, which drops a finished plan as `Rack::apply` does when a turn begins live.
pub fn plan(path: &Path) -> io::Result<Vec<Step>> {
    let reader = BufReader::new(File::open(path)?);
    let mut plan = Vec::new();
    for line in reader.split(b'\n').map_while(Result::ok) {
        let line = String::from_utf8_lossy(&line);
        if !line.contains(UPDATE_PLAN)
            && !line.contains("\"origin\"")
            && !line.contains("queued_command")
        {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if starts_turn(&record) {
            if finished(&plan) {
                plan.clear();
            }
            continue;
        }
        for input in calls(&record, UPDATE_PLAN) {
            if let Some(change) = PlanChange::read(input) {
                change.apply(&mut plan);
            }
        }
    }
    Ok(plan)
}

/// What the transcript's newest turn did that the `Stop` nudge weighs: whether it started a
/// command in the background, the task `id`s Claude Code gave the commands it started, and
/// whether it called the `plates` server's `wait` or `ping`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnCalls {
    pub backgrounded: bool,
    pub started: Vec<String>,
    pub waited: bool,
}

/// What the newest turn called, from the prompt that started it to the end of the transcript. A
/// turn can run long, so this reads the whole transcript, as `plan` does, and JSON-parses only
/// the records that could matter.
pub fn newest_turn_calls(path: &Path) -> io::Result<TurnCalls> {
    let reader = BufReader::new(File::open(path)?);
    let mut turn = TurnCalls::default();
    for line in reader.split(b'\n').map_while(Result::ok) {
        let line = String::from_utf8_lossy(&line);
        if !line.contains("run_in_background")
            && !line.contains("backgroundTaskId")
            && !line.contains("mcp__plates__")
            && !line.contains("\"origin\"")
            && !line.contains("queued_command")
        {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if starts_turn(&record) {
            turn = TurnCalls::default();
            continue;
        }
        let backgrounded = ["Bash", "PowerShell"].iter().any(|tool| {
            calls(&record, tool)
                .any(|input| input.get("run_in_background").and_then(Value::as_bool) == Some(true))
        });
        turn.backgrounded |= backgrounded;
        // A background command's result is a `user` record whose `toolUseResult` holds the task's
        // `backgroundTaskId`, such as `bgnnbfb5e` (seen in this repository's own sessions'
        // transcripts, 2026-11-27).
        if let Some(id) = record
            .get("toolUseResult")
            .and_then(|r| r.get("backgroundTaskId"))
            .and_then(Value::as_str)
        {
            turn.started.push(id.to_string());
        }
        turn.waited |= calls(&record, "mcp__plates__wait").next().is_some()
            || calls(&record, "mcp__plates__ping").next().is_some();
    }
    Ok(turn)
}

// A prompt, which can start a turn: one you typed, a task notification's, or another agent's
// report. Claude Code writes one as a `user` record whose `origin` has a `kind`, such as `human`
// or `peer`, while a tool's result is a `user` record with no `origin`, and writes one that waited
// for Claude, as task notifications do, as an `attachment` record of type `queued_command`
// (measured on Windows 11 25H2, 2026-11-21, record/areas/hooks/log.md).
fn starts_turn(record: &Value) -> bool {
    fn kind(r: &Value) -> Option<&str> {
        r.get("type").and_then(Value::as_str)
    }
    match kind(record) {
        Some("user") => record
            .get("origin")
            .and_then(|o| o.get("kind"))
            .is_some_and(Value::is_string),
        Some("attachment") => record.get("attachment").and_then(kind) == Some("queued_command"),
        _ => false,
    }
}

// The inputs of an assistant record's calls to the tool `name`.
fn calls<'a>(record: &'a Value, name: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(move |p| {
            p.get("type").and_then(Value::as_str) == Some("tool_use")
                && p.get("name").and_then(Value::as_str) == Some(name)
        })
        .filter_map(|p| p.get("input"))
}

fn is_marker(record: &Value) -> bool {
    if record.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let content = record.get("message").and_then(|m| m.get("content"));
    let text = match content {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(parts)) => parts
            .iter()
            .find(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str),
        _ => None,
    };
    text.is_some_and(|t| t.starts_with("[Request interrupted by user"))
}

// An assistant record's call to the Claude desktop app's wrap-up tool, as hooks::parse reads the
// same call from its PreToolUse.
fn wrap_up(record: &Value) -> Option<WrapUp> {
    let parts = record.get("message")?.get("content")?.as_array()?;
    let input = parts.iter().find_map(|p| {
        (p.get("type").and_then(Value::as_str) == Some("tool_use")
            && p.get("name").and_then(Value::as_str) == Some("mcp__ccd_turn__wrap_up"))
        .then(|| p.get("input"))
        .flatten()
    })?;
    Some(WrapUp::read(input))
}

/// Reads a transcript record's `timestamp`, such as `2026-11-15T20:22:39.123Z`, which Claude
/// Code writes in UTC.
pub fn parse_timestamp(text: &str) -> Option<SystemTime> {
    let (date, rest) = text.split_once('T')?;
    let mut ymd = date.split('-').map(str::parse::<i64>);
    let (y, m, d) = (ymd.next()?.ok()?, ymd.next()?.ok()?, ymd.next()?.ok()?);
    let clock = rest.strip_suffix('Z')?;
    let (hms, frac) = clock.split_once('.').unwrap_or((clock, "0"));
    let mut parts = hms.split(':').map(str::parse::<i64>);
    let (h, min, s) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    let ms: i64 = format!("{frac:0<3}").get(..3)?.parse().ok()?;
    // Days from 1970-01-01 to the date, by Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 86_400 + h * 3_600 + min * 60 + s) * 1_000 + ms)
        .try_into()
        .ok()?;
    Some(UNIX_EPOCH + Duration::from_millis(total))
}

fn assistant_text(record: &Value) -> Option<String> {
    if record.get("type").and_then(Value::as_str) != Some("assistant")
        || record.get("isSidechain").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let parts = record.get("message")?.get("content")?.as_array()?;
    parts
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .find(|t| !t.trim().is_empty())
        .map(str::to_string)
}

fn typed_prompt(record: &Value) -> Option<String> {
    let is = |key: &str, want: &str| record.get(key).and_then(Value::as_str) == Some(want);
    let human = record
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str)
        == Some("human");
    let sidechain = record.get("isSidechain").and_then(Value::as_bool) == Some(true);
    if !is("type", "user") || !human || sidechain {
        return None;
    }
    let content = record.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .filter(|t| !t.trim_start().starts_with("<system-reminder>"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    Some(text).filter(|t| !t.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn transcript(name: &str, lines: &[Value]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "plates-core-transcript-{name}-{}.jsonl",
            std::process::id()
        ));
        let text: Vec<String> = lines.iter().map(Value::to_string).collect();
        fs::write(&path, text.join("\n")).unwrap();
        path
    }

    #[test]
    fn the_newest_turns_calls_say_whether_it_backgrounded_a_command_and_waited() {
        use serde_json::json;
        let prompt = json!({ "type": "user", "isSidechain": false, "origin": { "kind": "human" },
                             "message": { "role": "user", "content": "Run the tests" } });
        let call = |name: &str, input: Value| {
            json!({ "type": "assistant", "isSidechain": false, "message": { "role": "assistant",
                    "content": [ { "type": "tool_use", "name": name, "input": input } ] } })
        };
        let background = call(
            "Bash",
            json!({ "command": "cargo test", "run_in_background": true }),
        );
        let result = |id: &str| {
            json!({ "type": "user", "isSidechain": false,
                    "message": { "role": "user", "content": [ { "type": "tool_result", "content": "" } ] },
                    "toolUseResult": { "stdout": "", "backgroundTaskId": id } })
        };
        let wait = call("mcp__plates__wait", json!({ "on": "test run" }));
        let path = transcript(
            "calls",
            &[
                prompt.clone(),
                background.clone(),
                result("old1"),
                wait,
                prompt.clone(),
                background,
                result("new1"),
            ],
        );
        let turn = newest_turn_calls(&path).unwrap();
        assert!(turn.backgrounded && !turn.waited);
        assert_eq!(turn.started, ["new1"]);
        let path = transcript(
            "calls-plain",
            &[prompt, call("Bash", json!({ "command": "ls" }))],
        );
        assert_eq!(newest_turn_calls(&path).unwrap(), TurnCalls::default());
    }

    #[test]
    fn the_first_typed_prompt_skips_reminders_tool_results_and_subagents() {
        use serde_json::json;
        let path = transcript(
            "first",
            &[
                json!({ "type": "queue-operation", "operation": "enqueue" }),
                json!({ "type": "user", "isSidechain": true, "origin": { "kind": "human" },
                        "message": { "role": "user", "content": "a subagent's task" } }),
                json!({ "type": "user", "isSidechain": false,
                        "message": { "role": "user", "content": [ { "type": "tool_result", "content": "" } ] } }),
                json!({ "type": "user", "isSidechain": false, "origin": { "kind": "human" },
                        "message": { "role": "user", "content": [
                            { "type": "text", "text": "<system-reminder>context</system-reminder>" },
                            { "type": "text", "text": "Fix the dock hold" } ] } }),
                json!({ "type": "user", "isSidechain": false, "origin": { "kind": "human" },
                        "message": { "role": "user", "content": "A later prompt" } }),
            ],
        );
        assert_eq!(
            first_prompt(&path).unwrap().as_deref(),
            Some("Fix the dock hold")
        );
    }

    #[test]
    fn the_status_is_the_newest_text_since_the_last_typed_prompt() {
        use serde_json::json;
        let said = |text: &str| {
            json!({ "type": "assistant", "isSidechain": false,
                    "message": { "role": "assistant", "content": [ { "type": "text", "text": text } ] } })
        };
        let tool = json!({ "type": "assistant", "isSidechain": false,
                           "message": { "role": "assistant", "content": [ { "type": "tool_use", "name": "Bash" } ] } });
        let prompt = json!({ "type": "user", "isSidechain": false, "origin": { "kind": "human" },
                             "message": { "role": "user", "content": "Next" } });
        let path = transcript(
            "status",
            &[
                said("An old line"),
                prompt.clone(),
                said("Reading the receiver first."),
                tool.clone(),
                said("Now the installer."),
                tool,
            ],
        );
        assert_eq!(
            latest_status(&path).unwrap().as_deref(),
            Some("Now the installer.")
        );
        let fresh = transcript("fresh", &[said("An old line"), prompt]);
        assert_eq!(latest_status(&fresh).unwrap(), None);
    }

    #[test]
    fn a_timestamp_reads_as_utc() {
        assert_eq!(
            parse_timestamp("1970-01-01T00:00:01.5Z"),
            Some(UNIX_EPOCH + Duration::from_millis(1_500))
        );
        assert_eq!(
            parse_timestamp("2026-11-15T16:32:25.402Z"),
            Some(UNIX_EPOCH + Duration::from_millis(1_794_760_345_402))
        );
        assert_eq!(parse_timestamp("2026-11-15 16:32:25"), None);
    }

    #[test]
    fn the_newest_turn_is_finished_interrupted_or_running() {
        use serde_json::json;
        let rec = |kind: &str, at: &str, content: Value, extra: Value| {
            let mut r = json!({ "type": kind, "isSidechain": false, "timestamp": at,
                                "message": { "role": kind, "content": content } });
            r.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            r
        };
        let prompt = rec(
            "user",
            "2026-11-15T10:00:00Z",
            json!("Ship it"),
            json!({ "origin": { "kind": "human" } }),
        );
        let wrap = rec(
            "assistant",
            "2026-11-15T10:00:05Z",
            json!([{ "type": "tool_use", "name": "mcp__ccd_turn__wrap_up",
                     "input": { "recap": "Pushed." } }]),
            json!({}),
        );
        let mut last = rec(
            "assistant",
            "2026-11-15T10:00:09Z",
            json!([{ "type": "text", "text": "Pushed as abc123." }]),
            json!({}),
        );
        last["message"]["stop_reason"] = json!("end_turn");
        let system = json!({ "type": "system", "subtype": "stop_hook_summary" });
        let finished = transcript("turn-finished", &[prompt.clone(), wrap, last, system]);
        assert_eq!(
            newest_turn(&finished).unwrap(),
            Some((
                Turn::Finished {
                    last_text: Some("Pushed as abc123.".into()),
                    wrap_up: Some(WrapUp {
                        recap: Some("Pushed.".into()),
                        ..WrapUp::default()
                    })
                },
                parse_timestamp("2026-11-15T10:00:09Z").unwrap()
            ))
        );

        let marker = rec(
            "user",
            "2026-11-15T10:00:03Z",
            json!([{ "type": "text", "text": "[Request interrupted by user]" }]),
            json!({}),
        );
        let interrupted = transcript("turn-interrupted", &[prompt.clone(), marker]);
        assert_eq!(
            newest_turn(&interrupted).unwrap().map(|(t, _)| t),
            Some(Turn::Interrupted { denied: false })
        );

        let running = transcript("turn-running", &[prompt]);
        assert_eq!(
            newest_turn(&running).unwrap().map(|(t, _)| t),
            Some(Turn::Running)
        );
    }

    #[test]
    fn the_plan_replays_the_calls_and_a_new_turn_drops_a_finished_one() {
        use crate::session::StepStatus::{Completed as C, InProgress as I, Pending as P};
        use serde_json::json;
        let call = |input: Value, sidechain: bool| {
            json!({ "type": "assistant", "isSidechain": sidechain,
                    "message": { "role": "assistant", "content": [
                        { "type": "tool_use", "name": UPDATE_PLAN, "input": input } ] } })
        };
        let items = |statuses: &[&str]| {
            let items: Vec<Value> = statuses
                .iter()
                .enumerate()
                .map(|(i, s)| json!({ "content": format!("Step {}", i + 1), "status": s }))
                .collect();
            call(json!({ "items": items, "scope": "new_task" }), false)
        };
        let typed = json!({ "type": "user", "isSidechain": false, "origin": { "kind": "human" },
                            "message": { "role": "user", "content": "Next" } });
        let result = json!({ "type": "user", "isSidechain": false,
                             "message": { "role": "user", "content": [ { "type": "tool_result", "content": "" } ] } });
        let queued = json!({ "type": "attachment", "isSidechain": false,
                             "attachment": { "type": "queued_command", "prompt": "<task-notification/>",
                                             "origin": { "kind": "task-notification" } } });
        let statuses = |lines: &[Value]| -> Vec<_> {
            let path = transcript("plan", lines);
            plan(&path).unwrap().iter().map(|s| s.status).collect()
        };

        let advanced = [
            items(&["in_progress", "pending", "pending"]),
            result.clone(),
            call(json!({ "advance": true }), false),
            call(json!({ "advance": true }), true),
        ];
        assert_eq!(
            statuses(&advanced),
            [C, I, P],
            "a subagent's call is left out"
        );

        let carried = [advanced.to_vec(), vec![queued.clone(), typed.clone()]].concat();
        assert_eq!(
            statuses(&carried),
            [C, I, P],
            "a plan with a step pending lasts into the next turn"
        );

        let finished = [
            advanced.to_vec(),
            vec![call(json!({ "done": [2, 3] }), false), result, queued],
        ]
        .concat();
        assert!(
            statuses(&finished).is_empty(),
            "a queued prompt starts a turn too"
        );

        assert!(statuses(&[typed]).is_empty());
    }

    #[test]
    fn a_transcript_with_no_typed_prompt_has_none() {
        use serde_json::json;
        let path = transcript("none", &[json!({ "type": "last-prompt" })]);
        assert_eq!(first_prompt(&path).unwrap(), None);
    }
}
