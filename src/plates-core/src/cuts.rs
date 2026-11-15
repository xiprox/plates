//! Noticing when you interrupt Claude or deny a tool. Claude Code fires no hook event for either
//! until your next prompt, so without this a tile would keep saying Claude is working or waiting
//! for an approval. Claude Code does write a record to the transcript at once, so the app watches
//! the transcript of each session in a turn and reads what it gains.
//!
//! An interrupt writes a `user` record whose text is `[Request interrupted by user]`, and a denial
//! writes the tool's result with `toolDenialKind` `user-rejected` and then a record whose text is
//! `[Request interrupted by user for tool use]` (measured on Windows 11 25H2, 2026-11-15,
//! record/areas/hooks/log.md).

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
};

use serde_json::Value;

use crate::{hooks::HookMessage, session::SessionEvent};

/// The `hook` of a message `Cuts` makes, since no hook event carries it.
pub const FROM_TRANSCRIPT: &str = "Transcript";

const MARKER: &str = "[Request interrupted by user";

/// The transcripts of the sessions in a turn, and how far each has been read.
#[derive(Default)]
pub struct Cuts {
    watched: HashMap<String, Watched>,
}

struct Watched {
    path: PathBuf,
    cwd: PathBuf,
    read_to: u64,
    /// Whether the newest record read so far is a tool result you denied, which a marker
    /// arriving in a later read turns into a denial.
    denied: bool,
}

impl Cuts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts or stops watching a session's transcript after one of its hook events.
    /// A turn's end stops it. A prompt starts it from the transcript's current end, so a marker
    /// from an earlier turn isn't read as this one's.
    pub fn saw(&mut self, message: &HookMessage) {
        if message.event.is_none() {
            return;
        }
        if matches!(
            message.hook.as_str(),
            "Stop" | "StopFailure" | "SessionEnd" | "SessionStart" | FROM_TRANSCRIPT
        ) {
            self.watched.remove(&message.session_id);
            return;
        }
        let Some(path) = &message.transcript_path else {
            return;
        };
        if message.hook != "UserPromptSubmit" && self.watched.contains_key(&message.session_id) {
            return;
        }
        let read_to = std::fs::metadata(path).map_or(0, |m| m.len());
        self.watched.insert(
            message.session_id.clone(),
            Watched {
                path: path.clone(),
                cwd: message.cwd.clone(),
                read_to,
                denied: false,
            },
        );
    }

    /// Reads what each watched transcript gained since the last call, and returns a message for
    /// each session it shows you interrupted or denied, which it then stops watching.
    pub fn poll(&mut self) -> Vec<HookMessage> {
        let mut found = Vec::new();
        self.watched.retain(|session_id, w| match read_new(w) {
            Ok(Some(denied)) => {
                found.push(HookMessage {
                    hook: FROM_TRANSCRIPT.to_string(),
                    session_id: session_id.clone(),
                    cwd: w.cwd.clone(),
                    transcript_path: Some(w.path.clone()),
                    event: Some(SessionEvent::Interrupted { denied }),
                    status: None,
                    app: None,
                });
                false
            }
            Ok(None) => true,
            // A transcript that can't be read is left for the next call.
            Err(_) => true,
        });
        found
    }
}

// Reads the whole lines the transcript gained, and returns whether they end the turn with a
// marker: `Some(true)` for a denial and `Some(false)` for an interrupt.
fn read_new(w: &mut Watched) -> io::Result<Option<bool>> {
    let mut file = File::open(&w.path)?;
    let len = file.metadata()?.len();
    if len <= w.read_to {
        w.read_to = len;
        return Ok(None);
    }
    file.seek(SeekFrom::Start(w.read_to))?;
    let mut bytes = Vec::new();
    file.take(len - w.read_to).read_to_end(&mut bytes)?;
    // Claude Code may be partway through writing the last line, so it waits for the next call.
    let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else {
        return Ok(None);
    };
    w.read_to += end as u64 + 1;
    let mut cut = None;
    for line in String::from_utf8_lossy(&bytes[..end]).lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match kind(&record) {
            Kind::Marker => cut = Some(w.denied),
            Kind::Denial => {
                w.denied = true;
                cut = None;
            }
            Kind::Turn => {
                w.denied = false;
                cut = None;
            }
            Kind::Other => {}
        }
    }
    Ok(cut)
}

enum Kind {
    /// The record Claude Code writes when you interrupt Claude or deny a tool.
    Marker,
    /// A tool's result that says you denied it.
    Denial,
    /// Anything else Claude or you add to the conversation, which means the turn goes on.
    Turn,
    /// A record that isn't part of the conversation, or is a subagent's.
    Other,
}

fn kind(record: &Value) -> Kind {
    let kind = record.get("type").and_then(Value::as_str);
    if record.get("isSidechain").and_then(Value::as_bool) == Some(true)
        || !matches!(kind, Some("user" | "assistant"))
    {
        return Kind::Other;
    }
    if record.get("toolDenialKind").and_then(Value::as_str) == Some("user-rejected") {
        return Kind::Denial;
    }
    let content = record.get("message").and_then(|m| m.get("content"));
    let first_text = match content {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(parts)) => parts
            .iter()
            .find(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|p| p.get("text"))
            .and_then(Value::as_str),
        _ => None,
    };
    if kind == Some("user") && first_text.is_some_and(|t| t.starts_with(MARKER)) {
        Kind::Marker
    } else {
        Kind::Turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, io::Write, path::Path};

    fn transcript(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "plates-core-cuts-{name}-{}.jsonl",
            std::process::id()
        ));
        fs::write(&path, "").unwrap();
        path
    }

    fn append(path: &Path, records: &[Value]) {
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        for r in records {
            writeln!(file, "{r}").unwrap();
        }
    }

    fn message(hook: &str, path: &Path) -> HookMessage {
        HookMessage {
            hook: hook.into(),
            session_id: "s".into(),
            cwd: PathBuf::from("/w"),
            transcript_path: Some(path.to_path_buf()),
            event: Some(SessionEvent::ToolFinished),
            status: None,
            app: None,
        }
    }

    fn said(text: &str) -> Value {
        json!({ "type": "user", "isSidechain": false,
                "message": { "role": "user", "content": [ { "type": "text", "text": text } ] } })
    }

    fn assistant() -> Value {
        json!({ "type": "assistant", "isSidechain": false,
                "message": { "role": "assistant", "content": [ { "type": "tool_use", "name": "Bash" } ] } })
    }

    fn denied(cuts: &mut Cuts) -> Option<bool> {
        let found = cuts.poll();
        found.first().map(|m| match m.event {
            Some(SessionEvent::Interrupted { denied }) => denied,
            _ => panic!("a cut should be Interrupted"),
        })
    }

    #[test]
    fn an_interrupt_after_the_prompt_is_found_and_an_earlier_one_isnt() {
        let path = transcript("interrupt");
        append(&path, &[said("[Request interrupted by user]")]);
        let mut cuts = Cuts::new();
        cuts.saw(&message("UserPromptSubmit", &path));
        append(&path, &[said("Go"), assistant()]);
        assert_eq!(denied(&mut cuts), None);
        append(&path, &[said("[Request interrupted by user]")]);
        assert_eq!(denied(&mut cuts), Some(false));
        assert_eq!(denied(&mut cuts), None, "a found cut stops the watch");
    }

    #[test]
    fn a_denial_split_across_two_reads_is_still_a_denial() {
        let path = transcript("denial");
        let mut cuts = Cuts::new();
        cuts.saw(&message("PermissionRequest", &path));
        append(
            &path,
            &[
                json!({ "type": "user", "isSidechain": false, "toolDenialKind": "user-rejected",
                      "message": { "role": "user", "content": [ { "type": "tool_result", "is_error": true } ] } }),
            ],
        );
        assert_eq!(denied(&mut cuts), None);
        append(&path, &[said("[Request interrupted by user for tool use]")]);
        assert_eq!(denied(&mut cuts), Some(true));
    }

    #[test]
    fn a_marker_followed_by_more_conversation_isnt_a_cut() {
        let path = transcript("resumed");
        let mut cuts = Cuts::new();
        cuts.saw(&message("PreToolUse", &path));
        append(
            &path,
            &[said("[Request interrupted by user]"), said("Go on")],
        );
        assert_eq!(denied(&mut cuts), None);
    }

    #[test]
    fn a_turns_end_stops_the_watch() {
        let path = transcript("stopped");
        let mut cuts = Cuts::new();
        cuts.saw(&message("PreToolUse", &path));
        cuts.saw(&message("Stop", &path));
        append(&path, &[said("[Request interrupted by user]")]);
        assert_eq!(denied(&mut cuts), None);
    }
}
