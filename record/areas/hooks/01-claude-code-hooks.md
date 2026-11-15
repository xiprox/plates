# Hooks: Claude Code's hooks

```text
Purpose:     How Plates learns what each Claude Code session is doing.
Depends on:  02-tiles (../rack/02-tiles.md), for the states a session tile shows.
Decides:     Which hook events Plates listens to and what each does to a tile, where a tile's title and summary come from, how the hooks get installed, and what clicking a session tile opens.
```

Claude Code sends each session's hook events as HTTP requests to a receiver Plates runs on `127.0.0.1`, and Plates turns them into session tiles.
Each event carries the session's `session_id`, which picks its tile, and the path of the session's transcript, which Plates reads for the tile's title and summary (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
The documentation says these hooks fire for sessions in the Claude desktop app too ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), and [the plan's step 3](../product/plan.md#3-claude-code-hookup) measures it.

## Events

| Event | What Plates does |
| --- | --- |
| `SessionStart` | adds the session's tile, as new |
| `UserPromptSubmit` | moves the tile to working, and stops its timer |
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval, and starts its timer |
| `PostToolUse` | moves a tile that needed approval back to working, since the tool ran |
| `Stop` | moves the tile to your turn, and starts its timer |
| `SessionEnd` | moves the tile to ended |

Plates ignores `Notification` with `notification_type` `idle_prompt`, which arrives about 60s after Claude finishes, because `Stop` has already moved the tile to your turn.
Which event arrives first after you answer an approval is unverified, and step 3 settles it.

## Titles and summaries

- A tile's title is the session's own title when `SessionStart` carries one in `session_title`, which it does only for a title set by hand, such as with `/rename` (documented, [hooks](https://code.claude.com/docs/en/hooks.md)).
  Otherwise the title is the session's first prompt, trimmed, read from the transcript, and the tooltip names the session's folder, from `cwd`.
- An approval's summary line is built from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, and "Wants to edit: " and the file for an edit.
  Where no `PreToolUse` came first, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of Claude's last message, read from the transcript.
- The transcript's format is unverified for a desktop session, and step 3 reads a real one before the reader is written.

## Installing

`plates hooks install` adds Plates' hooks to Claude Code's user settings file, `~/.claude/settings.json`.
It shows you the exact change, backs the file up beside itself, and writes the change only once you confirm.
`plates hooks uninstall` removes only the entries Plates added.
Each hook is of type `http`, with the receiver's URL and a timeout of a few seconds, since the documented default of 600s would let a stuck receiver hold up Claude Code.
What Claude Code does when the receiver isn't running is unverified: the hooks must not fail or slow a session while Plates is closed, and step 3 checks it.

## Opening a session

The Claude desktop app registers a `claude://` link handler, but no documented link opens one session ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)).
Until [the plan's step 6](../product/plan.md#6-opening-a-session-from-its-tile) finds one, clicking a session tile brings the Claude app forward.

## Left open

- The receiver's port.
  A fixed port lets the settings file name it, so the receiver has to handle that port being taken by another program.
- Sessions from before Plates started, which sent their events while no receiver was listening, and whose tiles appear only at their next event.
