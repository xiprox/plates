//! Answering a session's wrap-up from its hover card, by pressing the Claude desktop app's own
//! button for the choice, as a click on it would: through UI Automation on Windows, and through
//! the Accessibility API, the interface VoiceOver reads apps through, on macOS.
//!
//! No supported channel lets a program outside the app send a message into one of its sessions,
//! and a press through UI Automation reaches the session's transcript as the same `user` record a
//! click writes (measured on Windows 11 25H2, 2026-11-15, record/areas/hooks/log.md).

use std::time::Duration;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win;

#[cfg(target_os = "macos")]
use mac as platform;
#[cfg(windows)]
use win as platform;

/// How long the press waits for the app to show the session and its button. The app showed a
/// session's wrap-up within 843ms of its link on Windows, so this leaves room for a slower switch.
const WAIT: Duration = Duration::from_secs(3);

/// How often the press looks again while it waits.
const POLL: Duration = Duration::from_millis(100);

/// Presses the button labeled `choice` under the wrap-up of the session titled `title`, once the
/// app shows that session, on a thread of its own. If the button doesn't show within `WAIT`, it
/// presses nothing, and the session stays open in the app for you to answer.
pub fn press_choice(title: String, choice: String) {
    let spawned = std::thread::Builder::new()
        .name("plates-press".into())
        .spawn(move || {
            if let Err(e) = platform::press(&title, &choice) {
                eprintln!("Couldn't press \u{201c}{choice}\u{201d} in the Claude app: {e}");
            }
        });
    if let Err(e) = spawned {
        eprintln!("Couldn't start a thread to press a choice: {e}");
    }
}
