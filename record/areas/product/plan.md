# Product: plan

This plan builds Plates' first version from [01-aims](01-aims.md) and the designs in [record/index.md](../../index.md), in numbered steps, each ending with something you watch happen.
It's the one place Plates' status lives.

## 1. Window test

An experiment in `experiments/areas/rack/`: a throwaway GPUI app, first on Windows, then on macOS, that tries the window [01-window](../rack/01-window.md) designs.
Done when, on both OSes, you see:

- a rounded, borderless window that stays on top of every other window and has no taskbar or Dock entry
- the window dragged from anywhere and thrown, with its position set on every frame, and no visible stutter
- the window docking to a strip when held against a side edge, and coming back out when the strip is clicked
- clicks on the window leaving keyboard focus in the app you were in, and a text field in it taking focus and handing it back
- Windows Snap and macOS window tiling leaving the window alone
- on macOS, the window on every Space and over a full-screen app

The findings go in `record/areas/rack/log.md`.
If GPUI can't give the rack this window, a decision for Slint reverses [decision 001](../../decisions/001-rust-and-gpui.md).

## 2. Core

The `plates-core` crate: the tile model, the session states, the timer rules, and saving, from [02-tiles](../rack/02-tiles.md).
Done when `cargo test` covers each session state change and each timer rule, and a saved rack loads back the same.

## 3. Claude Code hookup

The hook receiver, `plates hooks install` and `uninstall`, and the transcript reader, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md) designs them.
Done when every hook event from a real session in the Claude desktop app reaches the receiver, and a session with Plates closed shows no hook error or delay.
The hooks log then has an entry for what each event carried, which event follows an approval you allow and one you deny, whether a resumed session keeps its `session_id`, and the format of a desktop session's transcript.

## 4. The rack

The interface from [01-window](../rack/01-window.md) and [02-tiles](../rack/02-tiles.md), drawn from the core's state.
Done when every state in [02-tiles](../rack/02-tiles.md) shows on the rack from real sessions and your own tiles.

## 5. Global hotkey

The hotkey [02-tiles](../rack/02-tiles.md#your-own-tiles) describes, which opens a new tile.
Done when the hotkey opens the new tile's field from any app, with the rack docked or not.

## 6. Opening a session from its tile

Finding whether the Claude desktop app's `claude://` link handler, or `claude --desktop --resume`, can open one desktop session, and opening a session's own view from its tile if one can ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session)).
Done when clicking a session tile opens that session, or else brings the Claude app forward and the hooks log says why nothing more is possible.

## 7. macOS

Each step's behavior on the Mac, run in one session there, as [CLAUDE.md](../../../CLAUDE.md) asks.
Done when every step's "done when" holds on macOS too.

## Later

Packaging as an installer and an app bundle, launching at login, and the questions [01-window](../rack/01-window.md#left-open) leaves open.
