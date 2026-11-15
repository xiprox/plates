# Hooks: Claude Code's hooks

```text
Purpose:     How Plates learns what each Claude Code session is doing.
Depends on:  02-tiles (../rack/02-tiles.md), for the states a session tile shows.
Decides:     Which hook events Plates listens to and what each does to a tile, where a tile's title and summary come from, how the hooks get installed and reach Plates, and what clicking a session tile opens.
```

Claude Code runs a small program, `plates-hook`, for each session's hook events, and `plates-hook` forwards each event to a receiver Plates runs on `127.0.0.1`, which turns it into a change to a session tile.
Each event carries the session's `session_id`, which picks its tile, and the path of the session's transcript (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
Sessions in the Claude desktop app send these events, and pick up newly installed hooks without a restart (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).

## Events

[The hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt) holds what the documentation says about each event below.

| Event | What Plates does |
| --- | --- |
| `SessionStart` | adds the session's tile, as new, and brings back a tile it shows as ended, as new; any other tile it shows stays as it is, since `SessionStart` also fires on a resume, a `/clear`, and a compaction |
| `UserPromptSubmit` | moves the tile to working, which stops its timer, while a tile already working keeps a timer you started on it |
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows |
| `PermissionRequest` | moves the tile to needs approval as Claude Code is about to ask you, and starts its timer |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval if it isn't already, which covers an approval that fired no `PermissionRequest` |
| `PostToolUse`, or `PostToolUseFailure` for a tool that failed | moves a tile that needed approval back to working, and stops its timer, since the tool ran |
| `Stop` | moves the tile to your turn, and starts its timer |
| `StopFailure`, which fires in place of `Stop` when an API error ends the turn | moves the tile to your turn, with the error's text as its summary line, or its `error` when the text is missing, and starts its timer |
| `SessionEnd` | moves the tile to ended |

- An event from a `session_id` Plates doesn't show adds that session's tile, in the state the event moves a tile to, and in working for an event that moves none, such as `PreToolUse`.
- In the Claude desktop app, `/clear` ends the session, with a `SessionEnd` `reason` of `other`, and goes on under a new `session_id`, so its tile ends and a new tile starts ([hooks log](log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)).
- Plates ignores `Notification` with `notification_type` `idle_prompt`, which arrives about 60s after Claude finishes ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), because `Stop` has already moved the tile to your turn.
- After you interrupt Claude, or deny a tool, no event arrives until the session's next one, so the tile stays working, or needs approval, until then ([hooks log](log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)).
- After you allow a tool, no event arrives until its `PostToolUse`, once the tool finishes, so a tile you approved for a long `cargo test` shows needs approval until the test ends (same entry).
- A resumed session keeps its `session_id`, so its tile carries on (same entry).
- `UserPromptSubmit` also fires for a subagent's report and a task notification, which start a turn as a prompt does, so the tile moves to working (same entry).
- Every hook runs in the background, and Claude Code ignores what a background hook decides, so the approval stays yours (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).

## Titles and summaries

- A tile's title is the session's custom title, which `SessionStart` and `UserPromptSubmit` carry in `session_title` when one is set, such as with `/rename`, and leave out for a title the Claude app generates.
  A title set during a session reaches Plates with your next prompt.
- A session with no custom title takes the first line of the first prompt you typed in it as its title.
  The transcript gives it for a session Plates first saw after that prompt, and `UserPromptSubmit`'s `prompt` gives it otherwise.
- The title skips the blocks of context the Claude desktop app puts before a prompt's text, such as `<artifact-view-context>` ([hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).
  A subagent's report or a task notification is nothing but such a block, so it can't name a session.
- The session's folder, which the tile's tooltip shows ([02-tiles](../rack/02-tiles.md#session-tiles)), comes from `cwd`.
- An approval's summary line is built from the tool the `PermissionRequest` names, or else from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, "Wants to edit: " and the file for an edit, and "Wants to use: " and the tool's name for any other tool.
  Where neither names a tool, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of `Stop`'s `last_assistant_message`, because the transcript isn't guaranteed to hold Claude's final message when `Stop` fires.
- A working tile's summary line is the first line of the newest text Claude wrote since your last prompt, which no hook event carries mid-turn, so Plates reads it from the end of the transcript after each tool event, and it can trail Claude by a record or two.

A transcript is JSON Lines, and a prompt you typed is a `user` record with `origin.kind` `human`, outside a subagent ([hooks log](log.md#2026-11-15-a-desktop-sessions-transcript-marks-a-typed-prompt-with-originkind-human-after-a-system-reminder)).

## Installing

`plates hooks install` adds Plates' hooks to Claude Code's user settings file, `~/.claude/settings.json`.
It shows you the exact change, backs the file up beside itself, and writes the change only once you confirm.
`plates hooks uninstall` removes only the entries Plates added.

Each hook is an async command hook, which Claude Code starts and doesn't wait for, and which runs `plates-hook` directly, with no shell.
`plates-hook` reads the event from stdin and forwards it to the receiver on `127.0.0.1:47231`.
While Plates is closed, `plates-hook` gives up after 300ms, in the background, and exits without an error.
An `http` hook straight to the receiver would cost a session about 2s for each event while Plates is closed, because Windows refuses a connection to a port nothing listens on only after about 2s ([hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).

The receiver answers each request with an empty 200 before it acts on the event, so a slow Plates never holds up `plates-hook`.

## Opening a session

On Windows, the Claude desktop app registers a `claude://` link handler (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), and we haven't looked on macOS.
Its one documented link, `claude://code/new`, opens the app's new-session page, and no documented link opens one session.
`claude --desktop --resume <session-id>` opens a session in the app by its ID, but it's documented only for a session started from the CLI and not still running ([hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)).
[The plan's step 6](../product/plan.md#6-opening-a-session-from-its-tile) tries it on a desktop session, and until a way is found, clicking a session tile brings the Claude app forward.

## Left open

- What Plates does when another program holds port 47231, which `receiver::PORT` sets.
- Sessions from before Plates started, which sent their events while no receiver was listening, and whose tiles appear only at their next event.
- How Plates can tell that you denied a tool or interrupted Claude, since no event says so.
  `Notification` with `idle_prompt`, which arrives about 60s after Claude finishes, may also arrive after either (unverified), and listening for it would show.
