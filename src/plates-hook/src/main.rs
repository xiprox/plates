//! The program Claude Code's hooks run to hand each hook event to Plates.
//!
//! An `http` hook straight to Plates' receiver waits about 2s per event on Windows while Plates
//! is closed, since a connection to a port nothing listens on is refused only after about 2s
//! (record/areas/hooks/log.md). So each hook is an async command hook running this program, which
//! Claude Code starts and doesn't wait for, and which reads the event from stdin, forwards it,
//! and gives up quietly when Plates isn't listening. It prints nothing, since Claude Code would
//! hand a JSON line on stdout to Claude. The one exception is `stop-check`, below.

// Without a console of its own, no window flashes up for each event.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{io::Read, time::Duration};

use plates_core::{
    hooks,
    receiver::{self, PORT},
    transcript,
};

/// How long to wait for Plates to take the connection. On 127.0.0.1 a listening Plates takes it
/// in well under 1ms, so 300ms only bounds how long the process lives while Plates is closed.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

fn main() {
    let mut body = Vec::new();
    if std::io::stdin().read_to_end(&mut body).is_err() || body.is_empty() {
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("stop-check") {
        stop_check(&body);
        return;
    }
    let _ = receiver::forward(PORT, &body, CONNECT_TIMEOUT);
}

// `plates-hook stop-check` is the one hook Claude Code waits for, on `Stop`. It reads the turn's
// calls from the session's transcript, and when `hooks::stop_nudge` gives a reason, prints the
// decision that keeps Claude from ending its turn, which Claude Code reads from stdout on exit
// code 0 (documented, https://code.claude.com/docs/en/hooks.md, "Stop decision control", as of
// 2026-11-27). It forwards nothing, since the async `Stop` hook does.
fn stop_check(body: &[u8]) {
    let Ok(stop) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    let Some(path) = stop.get("transcript_path").and_then(|p| p.as_str()) else {
        return;
    };
    let Ok(turn) = transcript::newest_turn_calls(std::path::Path::new(path)) else {
        return;
    };
    if let Some(reason) = hooks::stop_nudge(&stop, turn) {
        println!(
            "{}",
            serde_json::json!({ "decision": "block", "reason": reason })
        );
    }
}
