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
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows, and for what a working tile shows Claude doing |
| `PermissionRequest` | moves the tile to needs approval as Claude Code is about to ask you, and starts its timer |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval if it isn't already, which covers an approval that fired no `PermissionRequest` |
| `PostToolUse`, or `PostToolUseFailure` for a tool that failed | moves a tile that needed approval back to working, and stops its timer, since the tool ran |
| `Stop` | moves the tile to your turn, and starts its timer |
| `StopFailure`, which fires in place of `Stop` when an API error ends the turn | moves the tile to your turn, with the error's text as its summary line, or its `error` when the text is missing, and starts its timer |
| `SessionEnd` | moves the tile to ended |
| `PreCompact` | moves the tile to working, with "Compacting session…", the Claude desktop app's words, as its summary line until `PostCompact` |
| `PostCompact` | ends that line, and for a `trigger` of `manual`, a `/compact` you typed, moves the tile to your turn with "Compacted session" as its summary line, since no turn follows one ([hooks log](log.md#2026-11-15-claude-code-documents-33-hook-events-and-the-apps-status-line-shows-8-states)) |
| `Elicitation`, when an MCP server asks you to fill in a form or open a page | moves the tile to needs approval, with its `mcp_server_name`, " asks: ", and the first line of its `message` as its summary line, such as "github asks: Pick a repository", and starts its timer |
| `ElicitationResult`, once you've answered | moves a tile that needed approval back to working |

Claude Code documents 23 more events, and Plates installs no hook for them, because none changes what a tile shows (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15):

| Event | Why a tile doesn't need it |
| --- | --- |
| `UserPromptExpansion` | a slash command that expands into a prompt also fires `UserPromptSubmit` (unverified) |
| `PermissionDenied` | auto mode denied a tool, and Claude goes on with its turn |
| `PostToolBatch` | `PostToolUse` already marks each tool of a batch done |
| `MessageDisplay` | carries Claude's text as it streams, which could replace reading the transcript for the summary line, and we haven't tried it |
| `SubagentStart`, `SubagentStop` | the `Agent` tool's `PreToolUse` and `PostToolUse` already bracket a subagent |
| `TaskCreated`, `TaskCompleted`, `TeammateIdle` | they follow Claude's own task list and agent teams, not the turn |
| `Setup`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged`, `WorktreeCreate`, `WorktreeRemove`, `PreModelSwitch`, `PostModelSwitch` | they change a session's setup, not whether it needs you |

- When Claude Code documents an event that isn't in either table, add it to one of them, so no event goes unconsidered.
- Of `Notification`'s 14 documented `notification_type` values, Plates takes only `permission_prompt`. The others report things a tile already shows, such as `idle_prompt`, or things outside a session's turn, such as `auth_success`.
- The Claude desktop app's status line also shows states that no event carries: a thinking step, an API request it's retrying, a message it's sending, Claude stopping after you press stop, and how many background tasks run ([hooks log](log.md#2026-11-15-claude-code-documents-33-hook-events-and-the-apps-status-line-shows-8-states)).
  Plates shows "Thinking…" for all of Claude's work between tools, as [Titles and summaries](#titles-and-summaries) says, and shows none of the others.
- An event from a `session_id` Plates doesn't show adds that session's tile, in the state the event moves a tile to, and in working for an event that moves none, such as `PreToolUse`.
- In the Claude desktop app, `/clear` ends the session, with a `SessionEnd` `reason` of `other`, and goes on under a new `session_id`, so its tile ends and a new tile starts ([hooks log](log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)).
- Plates ignores `Notification` with `notification_type` `idle_prompt`, which arrives about 60s after Claude finishes ([hooks log](log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)), because `Stop` has already moved the tile to your turn.
- After you interrupt Claude, or deny a tool, no event arrives until the session's next one ([hooks log](log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)).
  Claude Code does write a marker to the transcript: a `user` record whose text is `[Request interrupted by user]` for an interrupt, and for a denial, a tool result with `toolDenialKind` `user-rejected` followed by `[Request interrupted by user for tool use]` ([hooks log](log.md#2026-11-15-an-interrupt-and-a-denial-each-write-a-marker-record-to-the-transcript)).
  So once a second, Plates reads what the transcript of each working or needs-approval session has gained since your last prompt.
  If the newest record it read is a marker, Plates moves the tile to your turn and starts its timer, with "Denied: " and the command, file, or tool's name as its summary line when a `user-rejected` result came just before the marker, and "Interrupted" otherwise.
  How soon after an interrupt the marker reaches the file is unverified, and timing an interrupt against its tile would settle it.
- After you allow a tool, no event arrives until its `PostToolUse`, once the tool finishes, so a tile you approved for a long `cargo test` shows needs approval until the test ends (same entry).
- A resumed session keeps its `session_id`, so its tile carries on (same entry).
- `UserPromptSubmit` also fires for a subagent's report and a task notification, which start a turn as a prompt does, so the tile moves to working (same entry).
- Every hook runs in the background, and Claude Code ignores what a background hook decides, so the approval stays yours (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).

## Titles and summaries

- A tile's title is the one the Claude desktop app shows for the session, which Plates reads from the app's own file for that session, the file whose `cliSessionId` or `priorCliSessionIds` holds the hook's `session_id` ([hooks log](log.md#2026-11-15-the-claude-desktop-app-keeps-a-file-per-session-with-its-title-and-its-claude-code-session-ids)).
  Plates reads the file again on any hook event after the app changes it, so a title the app generates or you rename shows with the session's next event.
  A session with no file, such as one in a terminal, takes its title from the sources below, and so does a session whose file Plates can't read.
  The app doesn't document these files, so an update to the app can make them unreadable.
- Without the app's title, a tile's title is the session's custom title, which `SessionStart` and `UserPromptSubmit` carry in `session_title` when one is set, such as with `/rename`, and leave out for a title the Claude app generates.
  A title set during a session reaches Plates with your next prompt.
- A session with neither takes the first line of the first prompt you typed in it as its title.
  The transcript gives it for a session Plates first saw after that prompt, and `UserPromptSubmit`'s `prompt` gives it otherwise.
- The title skips the blocks of context the Claude desktop app puts before a prompt's text, such as `<artifact-view-context>` ([hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).
  A subagent's report or a task notification is nothing but such a block, so it can't name a session.
- The session's folder comes from `cwd`, and its name is the tile's title until another title arrives.
- An approval's summary line is built from the tool the `PermissionRequest` names, or else from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, "Wants to edit: " and the file for an edit, and "Wants to use: " and the tool's name for any other tool.
  Where neither names a tool, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of `Stop`'s `last_assistant_message`, because the transcript isn't guaranteed to hold Claude's final message when `Stop` fires.
- A working tile's summary line is the first line of the newest text Claude wrote since your last prompt, for 4s after that text changes, so the text can be read even when a tool follows it at once (user, 2026-11-15).
  No hook event carries the text mid-turn, so Plates reads it from the end of the transcript after each tool event, and it can trail Claude by a record or two.
- After those 4s, the line shows what Claude is doing with a tool from its `PreToolUse` until its `PostToolUse`, and for at least 1s, so a tool that runs for a few milliseconds can still be read.
- Otherwise the line is "Thinking…", the Claude desktop app's own word for a thinking step (read in the Claude desktop app 5.11492.5.0, ion-dist.asar, the function that labels a thinking step).
  No hook event says when Claude thinks, so the line says it for all of Claude's work between tools, such as before the first tool after your prompt (user, 2026-11-15).

What Claude is doing comes from the tool's `PreToolUse`:

| Tool | What the summary line shows |
| --- | --- |
| `Read` | "Reading " and the file's name, such as "Reading plan.md" |
| `Edit`, `MultiEdit`, `NotebookEdit` | "Editing " and the file's name |
| `Write` | "Writing " and the file's name |
| `Grep` | "Searching for " and the pattern |
| `Glob` | "Finding " and the pattern |
| `WebFetch` | "Fetching " and the URL's host |
| `WebSearch` | "Searching the web for " and the query |
| `Skill` | "Using the ", the skill's name, and " skill" |
| `Bash`, `PowerShell`, `Agent`, `Task` | the description Claude gave it, or "Running " and the command's first line for a shell command with none |
| any other tool | "Using " and the tool's name, the last part of an MCP tool's `mcp__<server>__<tool>` |

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

Clicking a session tile opens `claude://code/continue?session=local_<id>`, with the app's own ID for the session from the app's file for it, and the Claude desktop app shows that session ([hooks log](log.md#2026-11-15-the-claude-desktop-app-opens-one-session-from-a-continue-link-with-its-own-id)).
The link is undocumented: we read it in the app's code and measured it from a shell, so an update to the app can change it.
Before it opens the link, Plates lets any program take the foreground, since the rack never takes it and the app must come to the front.
A session the app doesn't keep, such as one in a terminal, has no such ID, and clicking its tile opens nothing.
We haven't looked at how the app opens one session on macOS.

## Left open

- What Plates does when another program holds port 47231, which `receiver::PORT` sets.
- Sessions from before Plates started, which sent their events while no receiver was listening, and whose tiles appear only at their next event.
- Whether Plates keeps one tile across a `/clear`, through the app file's `priorCliSessionIds`, which may hold a session's earlier IDs (unverified).
