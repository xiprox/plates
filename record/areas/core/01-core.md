# Core: the model and its rules

```text
Purpose:     What plates-core holds, and the rules it applies to tiles, timers, and saving.
Depends on:  02-tiles (../rack/02-tiles.md), 01-claude-code-hooks (../hooks/01-claude-code-hooks.md), and decision 001 (../../decisions/001-rust-and-gpui.md).
Decides:     The split between the crates, how events and time reach the model, the order of tiles, the timer's states, and how Plates saves.
```

Plates is three crates in the Cargo workspace under `src/`.
`plates-core` holds the rack's model and every rule [02-tiles](../rack/02-tiles.md) and [01-claude-code-hooks](../hooks/01-claude-code-hooks.md) give it.
It also holds the receiver, `hooks::parse`, which turns a hook's JSON into a `SessionEvent`, the settings-file edits `plates hooks install` makes, and the transcript reader.
None of it is GPUI code, as [decision 001](../../decisions/001-rust-and-gpui.md) commits us to.
`plates` is the program: the window, the drawing, the hotkey, and the `plates hooks` and `plates listen` commands, and it owns one `Rack`.
`plates-hook` is the small program each hook runs, which forwards the event to the receiver.

## Events and time

- The model is plain data that changes only through `Rack`'s methods, on the app's UI thread.
- Every method that depends on time takes the current time as a `SystemTime` argument, so a test sets the clock and the model never reads it.
- A session's hook events reach the model as `SessionEvent` values, one per Claude Code hook event that changes a tile.
  `hooks::parse` makes them from the hooks' JSON, so `Rack` never sees JSON.
  An interrupt or a denial reaches the model as `SessionEvent::Interrupted`, which `Cuts` makes from the session's transcript, since no hook event carries it.
- The app calls `Rack::tick` once a second and on every event, which turns a timer that has run out into time-up, brings back a dismissed session whose timer set for tomorrow has run out, and clears an ended tile once [02-tiles](../rack/02-tiles.md#session-tiles) says it clears itself.

## Tiles

- A tile has an id that's unique for the life of the app, a body that's either a session or your own tile, and a timer.
- Session tiles come first, in the order their sessions first reached Plates, and your own tiles follow, in the order you added them.
  A new session's tile goes after the last session tile, so no session tile above it moves.
  While its parent's tile is on the rack, a sub-session's tile goes instead right after its parent's family: the parent's tile, and the tiles right below it whose parent is in the family, such as a sub-session's own sub-sessions.
  Otherwise it goes after the last session tile, as any other session's does.
  A dismissed sub-session that comes back goes there too, and a restored sub-session's tile moves after its parent's family before the rack first shows it, so no tile moves under the cursor.
  When the Claude desktop app detaches a sub-session from its parent, the sub-session's tile, with its own sub-sessions, moves from the parent's family to just past it, so the parent's other sub-sessions stay under the parent (user, 2026-11-16).
  A detached tile that sat anywhere else stays where it is.
  An Undo puts a tile back in its own group, at the place it had there.
- A dismissed session's tile leaves `Rack::tiles` for a list the model keeps apart, where its events still apply, so the tile is current when it comes back, as [02-tiles](../rack/02-tiles.md#session-tiles) says it does.
- A session tile's title comes from the sources [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#titles-and-summaries) lists, in its order.
  For a session Plates first sees after you typed its first prompt, the app reads that prompt from the transcript and sets it over any prompt the first event carried.
- A session tile's state and timer follow [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events), event by event.
- A tile needs you while it's an approval, your turn, unread, or time-up, and the model keeps why it needs you and since when.
  A session that's done, one whose finished turn you've read, doesn't need you.
  The rack brightens the tile each time that reason changes, so a your-turn tile brightens at `Stop` and again, in its time-up tint, when its timer runs out.
  A time-up need starts at the tick that finds the timer run out, so a timer that ran out while Plates was closed brightens its tile at the first tick.

## The timer

A timer is idle, running until a time, set for tomorrow until a time, or time-up since a time.
[02-tiles](../rack/02-tiles.md#the-timer) gives its lengths, and when a timer set for tomorrow runs out.

| From | What happens | To |
| --- | --- | --- |
| idle, or time-up | you start a timer | running |
| running, or set for tomorrow | you start a timer | running, from now |
| any | you pick Tomorrow | set for tomorrow |
| running, or set for tomorrow | you cancel it | idle |
| running, or set for tomorrow | its time passes, at the next tick | time-up, since the time it ran out |
| time-up | you click the tile | idle |
| idle, running, or time-up | a session tile moves to your turn or needs approval | running for the default length |
| idle, running, or time-up | a session tile moves to working, or ends, or you read a your-turn tile, or dismiss it | idle |

A timer runs on the wall clock, so a timer that runs out while Plates is closed, or the PC sleeps, is time-up at the first tick after.

## Saving

- Plates saves what [02-tiles](../rack/02-tiles.md#saving) lists, in order, each timer as it stands, as JSON in `rack.json`, and loads it when it starts.
- Each save writes `rack.json.new` beside the old file and renames it over `rack.json`, so if Plates crashes mid-save, the old file is whole.
  After a power cut mid-save, whether the renamed file holds its data is unverified on Windows and macOS, since the save doesn't flush the new file to disk before renaming it.
- A file that doesn't parse, or whose `version` isn't 1, is renamed `rack.json.broken`, replacing any earlier one, and Plates starts with no tiles of your own.
- Session tiles go to `sessions.json` in the same way, with what [02-tiles](../rack/02-tiles.md#saving) lists for them.
  What changes within a turn, such as the activity line, isn't saved, since the session's next event brings it again.
  A file without a tile's message, wrap-up, or background tasks, such as one an older Plates wrote, loads them empty.
  The app saves `rack.json` after each change to your own tiles, and `sessions.json` at the next tick after a session tile changes, so a burst of tool events writes it once.
- `hooks::catch_up` runs on the restored tiles before the rack opens, and moves each one to what its transcript's newest turn shows, if the transcript's record for that turn is later than the tile's last change.
