# Product: plan

This plan builds Plates' first version from [01-aims](01-aims.md) and the designs in [record/index.md](../../index.md), in numbered steps, each ending with something you watch happen.
It's the one place Plates' status lives.

## 1. Window test

An experiment in `experiments/areas/rack/`: a throwaway GPUI app that tries the window [01-window](../rack/01-window.md) designs, on Windows.
Its macOS half runs in step 7, with the rest of the app ([decision 002](../../decisions/002-build-on-windows-first.md)).
Done when, on Windows, you see:

- a rounded, borderless window that stays on top of every other window and has no taskbar entry
- the window dragged from anywhere and thrown, with its position set on every frame, and no visible stutter
- the window docking to a strip when held against a side edge, and coming back out when the strip is clicked
- clicks on the window leaving keyboard focus in the app you were in, and a text field in it taking focus and handing it back
- Windows Snap leaving the window alone

The findings are in [the rack log](../rack/log.md).

Done on 2026-11-15.

## 2. Core

The `plates-core` crate, as [01-core](../core/01-core.md) designs it: the tile model, the session states from [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events), the timer rules, and saving.
Done when `cargo test` covers each session state change and each timer rule, and a saved rack loads back the same.

Done on 2026-11-15.

## 3. Claude Code hookup

The hook receiver, `plates hooks install` and `uninstall`, and the transcript reader, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md) designs them.
Done when every hook event from a real session in the Claude desktop app reaches the receiver, and a session with Plates closed shows no hook error and starts Claude's turn within 100ms of the same prompt with the hooks uninstalled, as the median of 10 prompts each.
The hooks log then has an entry for what each event carried and for each question the design's Events section leaves to this step, and for the format of a desktop session's transcript.

Done on 2026-11-15.

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

Every step's behavior on the Mac, the window test's macOS half included, run in one session there, as [CLAUDE.md](../../../CLAUDE.md) and [decision 002](../../decisions/002-build-on-windows-first.md) ask.
Done when every step's "done when" holds on macOS too, with no Dock entry for the rack, the rack on every Space and over a full-screen app, and macOS window tiling leaving it alone.
If GPUI can't give the rack its window on macOS, we weigh the options then.

## Later

Packaging as an installer and an app bundle, launching at login on macOS, and the questions [01-window](../rack/01-window.md#left-open) leaves open.
