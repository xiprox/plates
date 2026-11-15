//! Reading a session's first prompt from its transcript, for a session Plates first saw after
//! that prompt.
//!
//! A transcript is JSON Lines, one record per line. A prompt a person typed is a record with
//! `type` `user`, `origin.kind` `human`, and `isSidechain` false, whose `message.content` is a
//! string or a list of parts, and the Claude desktop app can put a `<system-reminder>` part
//! before the prompt. (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md)

use std::{
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

use serde_json::Value;

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
    fn a_transcript_with_no_typed_prompt_has_none() {
        use serde_json::json;
        let path = transcript("none", &[json!({ "type": "last-prompt" })]);
        assert_eq!(first_prompt(&path).unwrap(), None);
    }
}
