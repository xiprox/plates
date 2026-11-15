# Plates design

The spec agreed in the mockup review. The mockups are the visual reference, and this file is the source of truth for behaviour.

## The rack

One small floating window.

- Borderless, rounded corners (14px), always on top, no taskbar or Dock entry. On macOS it shows on every Space and over full-screen apps.
- 304px wide. No title. The header is a 28px strip with a centred horizontal grip. Padding is 12px on the left, right and bottom.
- **Drag from anywhere.** A press becomes a drag only after the pointer moves about 5px, so clicks on tiles still work, and a drag that starts on a tile doesn't also click it.
- **Throw.** Releasing mid-drag keeps the velocity. The rack slides with friction and bounces off screen edges. Throwing only moves it and never docks it.
- **Dock.** Drag the rack against a side edge and hold it there. A bar fills along that edge, and after about 1.5 seconds the rack tucks into a strip of pips. Clicking the strip slides the rack back out. Hovering the strip lists what needs you. The strip nudges out once when something new needs you.
- **Focus.** Clicking the rack never takes keyboard focus from the app you're in. Only adding or renaming a tile takes focus, and it hands focus back when you finish.
- The app drags the window itself, so OS snapping (Windows Snap, macOS tiling) should never trigger. Verify this in the window test.

## Tiles

- Every tile is 60px tall. Tiles never reorder. Sessions come first, then your own tiles, with no divider or gap between the groups.
- **The left marker is status; the right side is the timer.** A circle marks a session and a square marks your own tile.
- When there's no summary line, the title wraps to two lines before it truncates.
- Hovering a tile brightens its own background slightly instead of replacing it.

### Session tiles

Session tiles are driven by Claude Code hooks.

| State | Trigger | Look |
|---|---|---|
| Working | `UserPromptSubmit` | Blue dot that breathes slowly. This is the only continuous motion in the app. |
| Your turn | `Stop` | Amber dot. Summary line in amber: the first line of Claude's last message. A 5-minute timer starts on its own. |
| Needs approval | `Notification` (`permission_prompt`), detail from `PreToolUse` | Orange dot. Summary line in orange, e.g. "Wants to run: cargo test -p collector". A 5-minute timer starts on its own. |
| New | `SessionStart` | Slides in, and its background brightens blue once. |
| Ended | `SessionEnd` | Dimmed ring marker. Clears itself after 30 minutes. Hovering shows an × that dismisses it. |

- Clicking a session tile opens that session in the Claude app. If that isn't possible, it brings the app forward. The hover tooltip shows the status detail.
- Session tiles can't be renamed, because the change wouldn't reach the Claude app. The title is the session's custom title when the hooks provide one, otherwise a fallback (the first prompt, trimmed, with the folder name in the tooltip).
- The auto timer stops when you reply. There's no time-based escalation beyond it.

### Your own tiles

- Gray square and a title. No clock of their own.
- **Add:** the last item in the rack is a dashed phantom tile with a faint + and a faint "Ctrl + Shift + Space" hint. Clicking it, or pressing the global hotkey, brings the rack forward (sliding out of the dock if needed) and turns the phantom into an inline name field in place. Enter adds the tile, Esc cancels.
- **Rename:** double-click the tile to get the same inline field. Enter saves, Esc cancels.
- **Done:** a check appears on hover. Clicking it removes the tile, with an Undo toast for about 5 seconds. There's no archive.

## The timer (hourglass)

- **Click** starts 5 minutes. **Right-click** opens a menu with 5, 10, 15, 20 or 30 minutes, or 1 hour. Picking one while a timer runs restarts it.
- **Running:** the hourglass becomes a yellow ring (radius 9 in a 24-unit box) that drains. Hovering shows a faint × (about 55% opacity), and clicking cancels the timer.
- **Time's up:** this becomes the tile's status.
  - Your own tiles: the square turns yellow and the tile takes a yellow tint.
  - Session tiles: the tile takes a tint of its own status color (amber or orange). The dot keeps showing Claude's state.
  - In both cases the background brightens once, and the right side goes back to a plain hourglass. Clicking the hourglass gives another 5 minutes, and clicking the tile dismisses the reminder.
- Timers run on wall-clock time.
- The timer slot (icon, divider and hover) is translucent white, so it picks up any tint without adding a color of its own.

## Attention

- Nothing flashes or pulses. When a tile starts needing you, its background brightens once in its status color (about 1.4 seconds) and settles.
- "Needs you" means an approval, your turn, or a finished timer.
- No sounds and no quiet mode. The app is meant to be the focus aid.

## Colors (dark)

| Use | Value |
|---|---|
| Rack / border | `#131417` / `#2A2C31` |
| Tile | `#1C1D21` |
| Text / secondary | `#ECEDEF` / `#A3A6AE` |
| Working, new | `#74AEFF` |
| Your turn (dot / text) | `#F2B441` / `#F2C46A` |
| Approval (dot / text) | `#FF7A45` / `#FF9B6E` |
| Timer ring, time's up | `#F4D35E` |
| Own-tile marker | `#5E626B` |
| Tints: yellow / amber / orange | `#2B2715` / `#2F2616` / `#321F17` |

Type is Inter: titles 13.5px semibold, secondary text 12px.

## Data sources

- **Claude Code HTTP hooks** POST to a local receiver on `127.0.0.1`. The events are `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `Notification` (`permission_prompt`, `idle_prompt`), `Stop` and `SessionEnd`. Each payload includes `session_id`, `transcript_path`, `cwd` and `hook_event_name`. `SessionStart` also carries `session_title`, but only for custom titles. The docs say hooks fire in desktop app sessions too; confirm this on a real build.
- **The transcript file** gives the last assistant message (the summary line) and the first prompt (the title fallback).
- **Persistence:** your own tiles and running timers are saved as JSON in the app's data directory. Sessions are rebuilt from hooks and transcripts.

## Open questions

- A deep link to a specific desktop session. The `claude://` handler is registered, but the link format isn't documented.
- How good the title fallback looks on real sessions.
- Not decided for v1: hiding from screen capture, hiding over full-screen apps, launching at login, and remembering the position per monitor.
