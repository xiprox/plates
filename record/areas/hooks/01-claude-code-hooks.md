# Hooks: Claude Code's hooks

```text
Purpose:     How Plates learns what each Claude Code session is doing.
Depends on:  02-tiles (../rack/02-tiles.md), for the states a session tile shows.
Decides:     Which hook events Plates listens to and what each does to a tile, where a tile's title and summary come from, how the hooks get installed, and what clicking a session tile opens.
```

Claude Code sends each session's hook events as HTTP requests to a receiver Plates runs on `127.0.0.1`, and Plates turns them into session tiles.
Each event carries the session's `session_id`, which picks its tile, and the path of the session's transcript, which Plates reads for a tile's title (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
The documentation says these hooks fire for sessions in the Claude desktop app too ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), and [the plan's step 3](../product/plan.md#3-claude-code-hookup) measures it.

## Events

| Event | What Plates does |
| --- | --- |
| `SessionStart` | adds the session's tile, as new, or leaves a tile it already shows as it is, since `SessionStart` also fires on a resume, a `/clear`, and a compaction |
| `UserPromptSubmit` | moves the tile to working, and stops its timer |
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval, and starts its timer |
| `PostToolUse`, or `PostToolUseFailure` for a tool that failed | moves a tile that needed approval back to working, since the tool ran |
| `Stop` | moves the tile to your turn, and starts its timer |
| `StopFailure`, which fires in place of `Stop` when an API error ends the turn | moves the tile to your turn, with the error as its summary line, and starts its timer |
| `SessionEnd` | moves the tile to ended |

An event from a `session_id` Plates doesn't show adds that session's tile, in the state the event moves a tile to.
Plates ignores `Notification` with `notification_type` `idle_prompt`, which arrives about 60s after Claude finishes, because `Stop` has already moved the tile to your turn.

Two things about approvals are unverified, and step 3 settles both: which event arrives first after you allow a tool, and which event follows a tool you deny, since a denial fires no event of its own ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
Whether a resumed session keeps its `session_id` is unverified too, and step 3 checks it.

## Titles and summaries

- A tile's title is the session's custom title, which `SessionStart` and `UserPromptSubmit` carry in `session_title` when one is set, such as with `/rename`, and leave out for a title the Claude app generates ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
  A title set during a session reaches Plates with your next prompt.
- A session with no custom title takes its first prompt as its title, trimmed, read from the transcript.
- The session's folder, which the tile's tooltip shows ([02-tiles](../rack/02-tiles.md#session-tiles)), comes from `cwd`.
- An approval's summary line is built from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, and "Wants to edit: " and the file for an edit.
  Where no `PreToolUse` came first, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of `Stop`'s `last_assistant_message`, because the transcript isn't guaranteed to hold Claude's final message when `Stop` fires ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
- The transcript's format is unverified for a desktop session, and step 3 reads a real one before the reader is written.

## Installing

`plates hooks install` adds Plates' hooks to Claude Code's user settings file, `~/.claude/settings.json`.
It shows you the exact change, backs the file up beside itself, and writes the change only once you confirm.
`plates hooks uninstall` removes only the entries Plates added.
Each hook is of type `http`, with the receiver's URL and a timeout of a few seconds, since the documented default of 600s would let a stuck receiver hold up Claude Code.
The receiver answers each request before it acts on the event, because all of a session's `SessionEnd` hooks share 1.5s ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
While Plates is closed, each hook's connection fails, which Claude Code treats as a non-blocking error before going on (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15), and step 3 measures whether a desktop session shows that error or slows down.

## Opening a session

On Windows, the Claude desktop app registers a `claude://` link handler, and we haven't looked on macOS.
No documented link opens one desktop session ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)).
`claude --desktop --resume <session-id>` opens a session in the app by its ID, but it's documented only for sessions started from the CLI ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
[The plan's step 6](../product/plan.md#6-opening-a-session-from-its-tile) tries it on a desktop session, and until a way is found, clicking a session tile brings the Claude app forward.

## Left open

- The receiver's port, which step 3 picks.
  A fixed port lets the settings file name it, so the receiver has to handle that port being taken by another program.
- Sessions from before Plates started, which sent their events while no receiver was listening, and whose tiles appear only at their next event.
