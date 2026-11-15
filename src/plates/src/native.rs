//! The calls the rack makes to the OS through its window's native handle, for what GPUI's window
//! options leave out: placing the window, keeping it above other apps, reading the cursor and the
//! screens, and starting at login. win.rs holds Windows' calls and mac.rs macOS', with the same
//! signatures, so the rest of Plates calls `native::` on both.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win;

#[cfg(target_os = "macos")]
pub use mac::*;
#[cfg(windows)]
pub use win::*;
