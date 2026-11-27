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

Every step's behavior on the Mac, the window test's macOS half included, run in one session there from the macOS installer on step 8's release, as [CLAUDE.md](../../../CLAUDE.md) and [decision 002](../../decisions/002-build-on-windows-first.md) ask.
Done when every step's "done when" holds on macOS too, with no Dock entry for the rack, the rack on every Space and over a full-screen app, and macOS window tiling leaving it alone.
If GPUI can't give the rack its window on macOS, we weigh the options then.

## 8. Releases

A Release workflow that builds Windows and macOS, packs each with Velopack, and attaches the installers to a GitHub release, as [decision 003](../../decisions/003-releases-with-release-please-and-velopack.md) and [releasing](../build/releasing.md) describe.
Done when a GitHub release holds both platforms' installers, built by the workflow.

Done on 2026-11-16, with [v0.1.0](https://github.com/xiprox/plates/releases/tag/v0.1.0).

## 9. Holding pings while you're busy

The rule [03-holding-pings](../rack/03-holding-pings.md) gives, on Windows.
Step 7 runs its macOS half, which measures whether `CGEventSourceSecondsSinceLastEventType` reports key-downs without the Input Monitoring permission and, if step 9 holds pings for reading on macOS, tests the bundle-ID check for the Claude app in front ([03-holding-pings](../rack/03-holding-pings.md#left-open)).
Done when a timer that runs out while you type in another app, or while you read a reply in the Claude app, turns its tile time-up in silence.
It rings and brightens once you've gone 30s without typing and left the Claude app, or 3s after you send Claude a prompt.

## 10. Task lists

The `plates-tasks` server, its install, and the task list inside a session's tile, as [01-task-list](../tasks/01-task-list.md) designs them.
Before this step is done on Windows, we'll register the probe server there to learn whether a server for an app session gets `CLAUDE_CODE_HOST_SESSION_ID`.
Done when, on Windows, after `plates hooks install`, a session in the Claude desktop app starts a list once you agree on a plan with it, its tile shows the list's bar with its counts, the list opens under the bar and follows Claude's changes, and the list's file is gone once its tile goes for good.
Step 7's Mac session runs its macOS half.

## 11. Waits and pings

The `wait` and `ping` tools and the waiting state, as [02-waits-and-pings](../tasks/02-waits-and-pings.md) designs them.
Done when, on Windows, a session you ask to ping you once a pull request's checks pass shows waiting while they run, and when they pass, its tile moves to your turn with the ping's message and rings at once.
Step 7's Mac session runs its macOS half.

Done on 2026-11-22.

## 12. Updates

Plates looking for its own updates, and installing one from the rack's menu, as [01-updates](../build/01-updates.md) designs it.
Done when, on Windows, a copy installed from one release finds the next, shows the teal dot on the grip, and after a click on "Update to …" in the rack's menu, runs the new version.
Step 7's Mac session runs its macOS half.

## 13. Pull requests on tiles

The pull request chip on a session's tile, its pop-out, and `pr_link`, as [04-pull-requests](../rack/04-pull-requests.md) designs them.
Done when, on Windows and macOS, a session that opens a pull request shows its chip within a second, the chip's icon turns red within a minute of a check failing, and the tile's timer starts if the session isn't working.
The chip and its pop-out ran on macOS 26.5.2 on 2026-11-27, and we haven't yet seen a check fail under a watched tile.

## Later

The questions [01-window](../rack/01-window.md#left-open) leaves open.
