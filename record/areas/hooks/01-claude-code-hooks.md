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

[The hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt) holds what the documentation says about each event below.

| Event | What Plates does |
| --- | --- |
| `SessionStart` | adds the session's tile, as new, and brings back a tile it shows as ended, as new; any other tile it shows stays as it is, since `SessionStart` also fires on a resume, a `/clear`, and a compaction |
| `UserPromptSubmit` | moves the tile to working, and stops its timer |
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows |
| `PermissionRequest` | moves the tile to needs approval as Claude Code is about to ask you, and starts its timer; the receiver's empty answer leaves the decision to you |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval if it isn't already, which covers an approval that fired no `PermissionRequest` |
| `PostToolUse`, or `PostToolUseFailure` for a tool that failed | moves a tile that needed approval back to working, and stops its timer, since the tool ran |
| `Stop` | moves the tile to your turn, and starts its timer |
| `StopFailure`, which fires in place of `Stop` when an API error ends the turn | moves the tile to your turn, with the error's text as its summary line, or its `error` when the text is missing, and starts its timer |
| `SessionEnd` | moves the tile to ended |

- An event from a `session_id` Plates doesn't show adds that session's tile, in the state the event moves a tile to, and in working for an event that moves none, such as `PreToolUse`.
- `/clear` fires `SessionEnd`, with a `reason` of `clear`, and then `SessionStart`, so its tile ends and comes back as new.
- Plates ignores `Notification` with `notification_type` `idle_prompt`, which arrives about 60s after Claude finishes ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), because `Stop` has already moved the tile to your turn.
- An interrupted turn fires neither `Stop` nor `StopFailure`, and canceling a running tool fires no `PostToolUseFailure`, so after you interrupt Claude its tile stays working until the session's next event.

Step 3 settles what the documentation leaves open:

- whether `PermissionRequest` fires in a desktop session, and whether the receiver's empty answer leaves Claude Code's prompt as it is
- whether any event arrives between your allowing a tool and its `PostToolUse`, which fires only once the tool finishes, so until then a tile you approved for a long `cargo test` shows needs approval
- which event follows a tool you deny, since a denial fires no event of its own
- whether a session keeps its `session_id` across a resume and a `/clear`
- what an interrupt fires in a desktop session

## Titles and summaries

- A tile's title is the session's custom title, which `SessionStart` and `UserPromptSubmit` carry in `session_title` when one is set, such as with `/rename`, and leave out for a title the Claude app generates.
  A title set during a session reaches Plates with your next prompt.
- A session with no custom title takes the first line of its first prompt as its title, read from the transcript.
- The session's folder, which the tile's tooltip shows ([02-tiles](../rack/02-tiles.md#session-tiles)), comes from `cwd`.
- An approval's summary line is built from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, "Wants to edit: " and the file for an edit, and "Wants to use: " and the tool's name for any other tool.
  Where no `PreToolUse` came first, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of `Stop`'s `last_assistant_message`, because the transcript isn't guaranteed to hold Claude's final message when `Stop` fires.
- The transcript's format is unverified for a desktop session, and step 3 reads a real one before the reader is written.

## Installing

`plates hooks install` adds Plates' hooks to Claude Code's user settings file, `~/.claude/settings.json`.
It shows you the exact change, backs the file up beside itself, and writes the change only once you confirm.
`plates hooks uninstall` removes only the entries Plates added.
Each hook is of type `http`, with the receiver's URL and a timeout of its own, since the documented default of 600s would let a stuck receiver hold up Claude Code ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
The receiver answers each request before it acts on the event, so it never holds up a prompt, and because a session's `SessionEnd` hooks share a budget of 1.5s, which the hooks' own timeout raises when it's longer.
While Plates is closed, each hook's connection fails, which Claude Code treats as a non-blocking error before going on, and step 3 measures whether a desktop session shows that error or slows down.

## Opening a session

On Windows, the Claude desktop app registers a `claude://` link handler (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), and we haven't looked on macOS.
Its one documented link, `claude://code/new`, opens the app's new-session page, and no documented link opens one session.
`claude --desktop --resume <session-id>` opens a session in the app by its ID, but it's documented only for a session started from the CLI and not still running ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
[The plan's step 6](../product/plan.md#6-opening-a-session-from-its-tile) tries it on a desktop session, and until a way is found, clicking a session tile brings the Claude app forward.

## Left open

- The receiver's port, which step 3 picks.
  A fixed port lets the settings file name it, so the receiver has to handle that port being taken by another program.
- The hooks' timeout, which step 3 picks, and which also sets a session's `SessionEnd` budget.
- Sessions from before Plates started, which sent their events while no receiver was listening, and whose tiles appear only at their next event.
- How Plates can tell that a turn was interrupted, since no hook event says so.
