# Rack: tiles

```text
Purpose:     What each tile in the rack shows, and how you act on it.
Depends on:  01-window (01-window.md), and 01-claude-code-hooks (../hooks/01-claude-code-hooks.md) for where a session's state comes from.
Decides:     The tiles' layout and states, the timer, how a tile asks for attention, adding, renaming, and removing tiles, the hotkey, what Plates saves, and the tiles' colors and type.
```

A tile is one 60px row of the rack: a session tile for each Claude Code session, and your own tiles for anything else you want kept in view.
On every tile, the marker on the left is its status, and the timer slot on the right shows an hourglass, or a draining ring while a timer runs (user, 2026-11-15).
[The mockups](https://claude.ai/artifact/A81TcK3BfzDVyQ2qaCqgNd) draw every state on this page.

## Layout

- Tiles never change places, so a change of state never moves a tile under your cursor, and only adding or clearing a tile moves the tiles below it.
- Session tiles come first, then your own tiles, with no divider or gap between the groups (user, 2026-11-15).
- A session tile's marker is a circle, and your own tile's is a square.
- A title wraps to two lines when the tile has no summary line, and truncates after the second.
- Hovering a tile lays 3.5% white over its background, so a tinted tile keeps its tint.

## Session tiles

| State | Starts when | The tile shows |
| --- | --- | --- |
| New | the session starts | the tile slides in, and its background brightens blue once |
| Working | Claude starts a turn, such as when you send a prompt | a blue dot that breathes on a 2.4s cycle |
| Your turn | Claude finishes its turn | an amber dot, the first line of Claude's last message as an amber summary line, and a timer that starts on its own |
| Needs approval | Claude asks you to approve a tool | an orange dot, what Claude wants to do as an orange summary line, such as "Wants to run: cargo test -p collector", and a timer that starts on its own |
| Ended | the session ends | a dimmed ring for its marker, and an × on hover that clears the tile |

[01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events) says which hook event starts each state.

- Clicking a session tile opens that session in the Claude desktop app, or brings the app forward where no link opens one session ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session)).
- Hovering a session tile shows the session's folder, and its state in words, such as "Your turn · waiting 12m".
- Session tiles can't be renamed ([01-aims](../product/01-aims.md#non-goals)).
- An ended tile clears itself 30 minutes after the session ends.

## Your own tiles

- Your own tile has a gray square marker and a title, and shows no time except a timer you start ([01-aims](../product/01-aims.md#non-goals)).
- The hotkey for a new tile is Ctrl + Shift + Space on Windows and ⌃⇧Space on macOS.
- The last item in the rack is a phantom tile: a dashed outline with a faint + and the hotkey's name beside it.
  Clicking it, or pressing the hotkey, brings the rack forward, slides it out of the dock if it's docked, and turns the phantom tile into an inline name field.
  Enter adds the tile, and Esc cancels.
- Double-clicking your own tile turns its title into the same inline field, to rename it.
- An × appears on your own tile when you hover it, as on an ended session's tile (user, 2026-11-15).
  Clicking the × removes the tile and shows an Undo button for 5s, and there's no archive (user, 2026-11-15).

## The timer

- Clicking the hourglass starts a 5-minute timer.
  Right-clicking it opens a menu of 5, 10, 15, 20, and 30 minutes, and 1 hour, and picking one while a timer runs starts it over.
- While a timer runs, the timer slot shows a yellow ring that drains.
  Hovering the ring shows a faint ×, at 55% opacity, and clicking the ring cancels the timer.
- A session tile starts a 5-minute timer on its own when it moves to your turn or needs approval, and the timer stops when the session moves to working or ends.
- When the timer runs out, the tile is time-up, and that becomes its status:
  - your own tile's square turns yellow, and the tile takes a yellow tint
  - a session tile takes a tint of its own status color, amber for your turn and orange for an approval, and its dot keeps showing Claude's state
  - the timer slot goes back to the hourglass, which starts another 5 minutes when you click it, and clicking anywhere else on the tile clears time-up
- A timer runs on the wall clock, whether or not anyone is at the desk.
- The timer slot's icon, divider, and hover highlight are translucent white, so they take on a tile's tint without adding a color of their own (user, 2026-11-15).

## Attention

- Nothing flashes or pulses, and the only motion that repeats is a working session's breathing dot (user, 2026-11-15).
- When a tile starts needing you, or needs you for a new reason, such as its timer running out on your turn, its background brightens once in its status color over 1.4s, then settles back.
- A tile needs you when it's an approval, your turn, or time-up.
- Plates plays no sounds and has no quiet mode ([01-aims](../product/01-aims.md#non-goals)).

## Saving

Plates saves your own tiles, each with its timer, as JSON in its data folder, `%APPDATA%\Plates\` on Windows and `~/Library/Application Support/Plates/` on macOS, and loads them when it starts.

## Colors and type

| Use | Color |
| --- | --- |
| Tile | `#1C1D21` |
| Text, and secondary text | `#ECEDEF`, `#A3A6AE` |
| Working and new | `#74AEFF` |
| Your turn: dot, and text | `#F2B441`, `#F2C46A` |
| Approval: dot, and text | `#FF7A45`, `#FF9B6E` |
| Timer ring, and your own tile's time-up square | `#F4D35E` |
| Your own tile's marker | `#5E626B` |
| Time-up tints: your own tile, your turn, and approval | `#2B2715`, `#2F2616`, `#321F17` |

The type is Inter: titles at 13.5px semibold, summary lines and secondary text at 12px, and the hotkey's name on the phantom tile at 11px.

## Left open

- Where you change the hotkey.
