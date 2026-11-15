//! The program Claude Code's hooks run to hand each hook event to Plates.
//!
//! An `http` hook straight to Plates' receiver waits about 2s per event on Windows while Plates
//! is closed, since a connection to a port nothing listens on is refused only after about 2s
//! (record/areas/hooks/log.md). So each hook is an async command hook running this program, which
//! Claude Code starts and doesn't wait for, and which reads the event from stdin, forwards it,
//! and gives up quietly when Plates isn't listening. It prints nothing, since Claude Code would
//! hand a JSON line on stdout to Claude.

// Without a console of its own, no window flashes up for each event.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{io::Read, time::Duration};

use plates_core::receiver::{self, PORT};

/// How long to wait for Plates to take the connection. On 127.0.0.1 a listening Plates takes it
/// in well under 1ms, so 300ms only bounds how long the process lives while Plates is closed.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

fn main() {
    let mut body = Vec::new();
    if std::io::stdin().read_to_end(&mut body).is_err() || body.is_empty() {
        return;
    }
    let _ = receiver::forward(PORT, &body, CONNECT_TIMEOUT);
}
