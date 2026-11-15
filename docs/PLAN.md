# Build plan

Rust workspace layout:
- `plates-core`: the model, state machine, timers, persistence and hook receiver. No UI code, unit-tested.
- `plates`: the GPUI app, which draws the core's state and talks to the OS.

## 1. Window test (gate)

A throwaway GPUI app, first on Windows, then on macOS. It's done when all of these work:

- A rounded, borderless, always-on-top window with no taskbar or Dock entry.
- Drag from anywhere and throw, by setting the window position every frame. It has to stay smooth.
- Hold against a side edge to dock, which shrinks the window to the strip. Click to bring it back.
- Clicks don't steal focus from the app you're in, and an inline text field does take focus and hands it back.
- Windows Snap and macOS tiling don't kick in.
- macOS: the window shows on every Space and over full-screen apps.

If GPUI fights us here, the fallback is Slint.

## 2. Core

- Tile model (sessions and your own tiles), session state machine and timer rules, as in DESIGN.md.
- JSON persistence in the app data directory.
- Unit tests for the state transitions and timers.

## 3. Claude hookup

- Local HTTP receiver on `127.0.0.1` for the hook events.
- `plates hooks install` / `uninstall`: shows the exact change to `~/.claude/settings.json`, backs the file up, and applies the change only after you confirm.
- Transcript reader for the summary line and the title fallback.
- Check on a real desktop session that every event arrives.

## 4. The rack

Build the UI from DESIGN.md and the mockups:
- tiles, markers and summary lines
- the timer ring and the right-click menu
- the phantom tile and inline add, rename, and Done with Undo
- the one-time brighten
- the docked strip and its hover list

## 5. Global hotkey

Default Ctrl + Shift + Space (⌃⇧Space on the Mac), configurable.

## 6. Open a session from its tile

Find out what the `claude://` link handler accepts. If it can't open a specific session, bring the app forward instead.

## 7. macOS pass

Spaces, full-screen apps, focus behaviour and the hotkey on a real Mac.

## Later

Packaging (installer or app bundle), launch at login, and the open questions in DESIGN.md.
