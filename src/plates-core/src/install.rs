//! Adding Plates' hooks to Claude Code's user settings, and taking them out again.
//! A hook is Plates' own when it's an `http` hook whose URL is on `127.0.0.1` and ends in
//! `receiver::PATH`, so uninstalling leaves every other hook as it was.

use serde_json::{Map, Value, json};

use crate::receiver::{self, PATH};

/// The hook events Plates listens to, with the matcher each takes, from
/// record/areas/hooks/01-claude-code-hooks.md. A `Notification` hook matches only
/// `permission_prompt`, since Plates ignores every other notification.
pub const EVENTS: [(&str, Option<&str>); 10] = [
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
];

/// Each hook's timeout, in seconds. The receiver answers at once, so this only bounds how long a
/// stuck Plates can hold up Claude Code, against the default of 600s.
pub const TIMEOUT_SECS: u64 = 2;

fn is_ours(hook: &Value) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("http")
        && hook
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|u| u.starts_with("http://127.0.0.1:") && u.ends_with(PATH))
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// The settings file isn't a JSON object, or its `hooks` isn't one.
    NotAnObject(&'static str),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::NotAnObject(what) => write!(f, "{what} isn't a JSON object"),
        }
    }
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

/// Plates' hooks alone, as they're added to the settings, which `plates hooks install` shows you
/// before it writes them.
pub fn ours(port: u16) -> Map<String, Value> {
    let mut events = Map::new();
    for (event, matcher) in EVENTS {
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), json!(m));
        }
        group.insert(
            "hooks".into(),
            json!([{ "type": "http", "url": receiver::url(port), "timeout": TIMEOUT_SECS }]),
        );
        events.insert(event.into(), json!([Value::Object(group)]));
    }
    events
}

/// The settings with Plates' hooks added, replacing any Plates hooks already there, and leaving
/// every other hook as it was.
pub fn with(settings: &Value, port: u16) -> Result<Value, InstallError> {
    let mut settings = without(settings)?;
    let root = settings
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("the settings file"))?;
    let events = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or(InstallError::NotAnObject("`hooks`"))?;
    for (event, groups) in ours(port) {
        let list = events.entry(event).or_insert_with(|| json!([]));
        if let (Some(list), Some(new)) = (list.as_array_mut(), groups.as_array()) {
            list.extend(new.iter().cloned());
        }
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn installing_adds_one_http_hook_per_event_and_keeps_theirs() {
        let s = with(&theirs(), 47231).unwrap();
        assert_eq!(s["model"], "opus");
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], "check.sh");
        assert_eq!(
            pre[1]["hooks"][0],
            json!({ "type": "http", "url": "http://127.0.0.1:47231/plates/hook", "timeout": 2 })
        );
        assert_eq!(
            s["hooks"]["Notification"][0]["matcher"],
            "permission_prompt"
        );
        assert_eq!(s["hooks"].as_object().unwrap().len(), EVENTS.len());
    }

    #[test]
    fn installing_twice_leaves_one_set() {
        let once = with(&theirs(), 47231).unwrap();
        let twice = with(&once, 47231).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn uninstalling_takes_out_only_plates_hooks() {
        let installed = with(&theirs(), 47231).unwrap();
        assert_eq!(without(&installed).unwrap(), theirs());
        let bare = with(&json!({}), 47231).unwrap();
        assert_eq!(without(&bare).unwrap(), json!({}));
    }

    #[test]
    fn a_settings_file_that_isnt_an_object_is_refused() {
        assert_eq!(
            with(&json!([1, 2]), 47231),
            Err(InstallError::NotAnObject("the settings file"))
        );
    }
}
