# Core: the model and its rules

```text
Purpose:     What plates-core holds, and the rules it applies to tiles, timers, and saving.
Depends on:  02-tiles (../rack/02-tiles.md), 01-claude-code-hooks (../hooks/01-claude-code-hooks.md), and decision 001 (../../decisions/001-rust-and-gpui.md).
Decides:     The split between the two crates, how events and time reach the model, the order of tiles, the timer's states, and what Plates saves.
```

Plates is two crates in the Cargo workspace under `src/`.
`plates-core` holds the rack's model and every rule [02-tiles](../rack/02-tiles.md) and [01-claude-code-hooks](../hooks/01-claude-code-hooks.md) give it, with no GPUI code, as [decision 001](../../decisions/001-rust-and-gpui.md) commits us to.
`plates` is the app: the window, the drawing, the hotkey, and the `plates hooks` commands, and it owns one `plates-core` `Rack`.

## Events and time

- The model is plain data that changes only through `Rack`'s methods, on the app's UI thread.
- Every method that depends on time takes the current time as a `SystemTime` argument, so a test sets the clock and the model never reads it.
- A session's hook events reach the model as `SessionEvent` values, one per Claude Code hook event that changes a tile.
  Turning the hooks' JSON into `SessionEvent` values is the receiver's job, in step 3, and the model doesn't see JSON.
- The app calls `Rack::tick` once a second and on every event, which turns a timer that has run out into time-up and clears an ended tile 30 minutes after its session ended.

## Tiles

- A tile has an id that's unique for the life of the app, a body that's either a session or your own tile, and a timer.
- Session tiles come first, in the order their sessions first reached Plates, and your own tiles follow, in the order you added them.
  A new session's tile goes after the last session tile, so no session tile above it moves.
- A session tile's title is the session's custom title, or else the first line of its first prompt, or else, until either arrives, the name of the session's folder.
- A session tile's state and timer follow [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events), event by event.
- A tile needs you while it's an approval, your turn, or time-up, and the model keeps the time it started needing you, which the rack uses to brighten it once.

## The timer

A timer is idle, running until a time, or time-up since a time.

| From | What happens | To |
| --- | --- | --- |
| idle, or time-up | you start a timer of N minutes | running for N minutes |
| running | you start a timer of N minutes | running for N minutes, from now |
| running | you cancel it | idle |
| running | its time passes, at the next tick | time-up, since the time it ran out |
| time-up | you click the tile | idle |
| any | a session tile moves to your turn or needs approval | running for 5 minutes |
| any | a session tile moves to working, or ends | idle |

A timer runs on the wall clock, so a timer that runs out while Plates is closed, or the PC sleeps, is time-up at the first tick after.

## Saving

- Plates saves your own tiles, in order, with their titles and timers, as JSON in `rack.json` in its data folder, and loads them when it starts.
- Session tiles aren't saved, since a session's next hook event brings its tile back.
- Each save writes a new file beside the old one and renames it over the old one, so a crash mid-save leaves the old file whole.
- A file that doesn't parse is renamed `rack.json.broken`, so the next save can't overwrite what you had, and Plates starts with no tiles of your own.

## Left open

- How long the app waits after a change before saving, which step 4 picks once the rack makes changes.
