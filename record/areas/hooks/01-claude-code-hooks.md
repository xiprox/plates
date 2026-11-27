# Hooks: Claude Code's hooks

```text
Purpose:     How Plates learns what each Claude Code session is doing, and how it opens and answers one.
Depends on:  02-tiles (../rack/02-tiles.md), for the states a session tile shows.
Decides:     Which hook events Plates listens to and what each does to a tile, which background tasks keep a tile working, how Plates follows the app's plan for a request, where a tile's title and summary come from, which session started a session, which tile a session keeps when the app moves it to a new Claude Code session, what happens to a session the app archives, how Plates answers a wrap-up's choice, how the hooks get installed and reach Plates, and what clicking a session tile opens.
```

Claude Code runs a small program, `plates-hook`, for each session's hook events, and `plates-hook` forwards each event to a receiver Plates runs on `127.0.0.1`, which turns it into a change to a session tile.
Each event carries the session's `session_id`, which picks its tile, and the path of the session's transcript (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
Sessions in the Claude desktop app send these events, and pick up newly installed hooks without a restart (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).

## Events

[The hooks log](log.md#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt) holds what the documentation says about each event below.

| Event | What Plates does |
| --- | --- |
| `SessionStart` | adds the session's tile, as new, unless its `source` is `resume`, which the Claude desktop app sends when you open a session that wasn't running ([hooks log](log.md#2026-11-16-on-macos-the-app-keeps-its-session-files-under-application-support-and-opening-a-session-adds-its-tile-through-a-resume)); brings back a tile it shows as ended, as new; any other tile it shows stays as it is, since `SessionStart` also fires on a resume, a `/clear`, and a compaction |
| `UserPromptSubmit` | moves the tile to working, which stops its timer, while a tile already working keeps a timer you started on it |
| `PreToolUse` | keeps the tool's name and input, for the summary line of an approval that follows, and for what a working tile shows Claude doing, and for `mcp__ccd_turn__wrap_up`, what the paragraph below the table says; for `mcp__plates__wait` and `mcp__plates__ping`, their text, as [02-waits-and-pings](../tasks/02-waits-and-pings.md#how-plates-learns-of-a-call) says; and moves a waiting tile to working ([02-waits-and-pings](../tasks/02-waits-and-pings.md#the-waiting-state)) |
| `PermissionRequest` | moves the tile to needs approval as Claude Code is about to ask you, and starts its timer |
| `PreToolUse` or `PermissionRequest` for `AskUserQuestion`, Claude's tool for asking you to pick among options | moves the tile to needs approval as a question, which the hover card calls "Needs your input", with "Asks: " and the first line of the first question as its summary line, such as "Asks: Which design?", and starts its timer; the Claude desktop app asks for the tool through `PermissionRequest`, which would otherwise read as an approval ([hooks log](log.md#2026-11-16-the-claude-desktop-app-asks-for-askuserquestion-through-permissionrequest)) |
| `PreToolUse` for `mcp__ccd_turn__update_plan`, the Claude desktop app's tool for Claude's plan | changes Plates' copy of the plan, as [The app's plan](#the-apps-plan) says, and moves a waiting tile to working, but keeps no tool name, so the call never shows as what Claude is doing |
| `Notification` with `notification_type` `permission_prompt` | moves the tile to needs approval if it isn't already, which covers an approval that fired no `PermissionRequest` |
| `PostToolUse`, or `PostToolUseFailure` for a tool that failed | moves a tile that needed approval back to working, and stops its timer, since the tool ran |
| `Stop` | moves the tile to your turn, working, waiting, or unread, in the order [02-waits-and-pings](../tasks/02-waits-and-pings.md#where-a-turns-end-moves-the-tile) gives (user, 2026-11-15 and 2026-11-20) |
| `StopFailure`, which fires in place of `Stop` when an API error ends the turn | moves the tile to your turn, with the error's text as its summary line, or its `error` when the text is missing, and starts its timer |
| `SessionEnd` | moves the tile to ended |
| `PreCompact` | for a `trigger` of `manual`, a `/compact` you typed, moves the tile to working, and for `auto`, leaves the tile's state as it is; a tile that's then working shows "Compacting session…", the Claude desktop app's words, as its summary line until `PostCompact`, a prompt, a tool call, or the turn's end, unless the compaction is `auto` and the tile works only for background agents |
| `PostCompact` | ends "Compacting session…"; for a `trigger` of `manual`, a `/compact` you typed, it moves the tile to your turn with "Compacted session" as its summary line and starts its timer, since no turn follows one ([hooks log](log.md#2026-11-15-claude-code-documents-33-hook-events-and-the-apps-status-line-shows-8-states)), and for `auto` the turn goes on |
| `Elicitation`, when an MCP server asks you to fill in a form or open a page | moves the tile to needs approval as a question, which the hover card calls "Needs your input", with its `mcp_server_name`, " asks: ", and the first line of its `message` as its summary line, such as "github asks: Pick a repository", and starts its timer |
| `ElicitationResult`, once you've answered | moves a tile that needed approval back to working |

For `mcp__ccd_turn__wrap_up`, the tool the Claude desktop app has Claude call at a turn's end, `PreToolUse` keeps the call's `recap` and `ask`, and for each of its `actions`, its `label`, or where that's empty, its `action` in words, such as "Commit" for `commit`, for the [hover card](../rack/02-tiles.md#the-hover-card).
It also keeps whether the call asks you something, through a non-empty `ask` or a `blocked` of true (read in the tool's own description, Claude desktop app 5.11492.5.0).

Claude Code documents 19 more events, and Plates installs no hook for them, because none changes what a tile shows (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15):

| Event | Why a tile doesn't need it |
| --- | --- |
| `UserPromptExpansion` | a slash command that expands into a prompt also fires `UserPromptSubmit` (unverified) |
| `PermissionDenied` | auto mode denied a tool, and Claude receives the rejection message (documented); whether Claude then goes on with its turn is unverified ([hooks log](log.md#2026-11-15-an-interrupt-and-a-denial-each-write-a-marker-record-to-the-transcript)) |
| `PostToolBatch` | `PostToolUse` already marks each tool of a batch done |
| `MessageDisplay` | the summary line already takes Claude's text from the transcript; `MessageDisplay` carries that text as it streams, and we haven't tried reading it from there |
| `SubagentStart`, `SubagentStop` | the `Agent` tool's `PreToolUse` and `PostToolUse` bracket a foreground subagent; whether they also cover a background subagent, a resumed one, or a teammate's message is unverified |
| `TaskCreated`, `TaskCompleted`, `TeammateIdle` | they follow Claude's own task list and agent teams, not the turn |
| `Setup`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `DirectoryAdded`, `FileChanged`, `WorktreeCreate`, `WorktreeRemove`, `PreModelSwitch`, `PostModelSwitch` | they change a session's setup, not whether it needs you |

- When Claude Code documents an event that isn't in either table, add it to one of them, so no event goes unconsidered.
- A `PreCompact` with a `trigger` of `auto` leaves a tile that isn't working, or works only for background agents, as it is, because Claude Code fired a `PreCompact` in an idle session with no compaction and no `PostCompact` after it (measured on Windows 11 25H2, 2026-11-20, [hooks log](log.md#2026-11-20-claude-code-fired-a-precompact-in-an-idle-session-with-no-compaction-and-no-postcompact-after-it)).
  We don't know whether that one's `trigger` was `auto`.
  If it was `manual`, the tile still moves to working, and keeping the payloads of the hooks Claude Code fires in an idle session would settle it.
- Of `Notification`'s 14 documented `notification_type` values, Plates takes only `permission_prompt`.
  The others report things outside a session's turn, such as `auth_success`, or things another event already shows (unverified: the docs list the values without saying what each reports, and an `agent_needs_input` arriving in a session would show what it means).
- The Claude desktop app's status line also shows states that no event carries as they happen: a thinking step, an API request being retried, a wait for something such as a message being sent, and Claude stopping after you press stop ([hooks log](log.md#2026-11-15-claude-code-documents-33-hook-events-and-the-apps-status-line-shows-8-states)).
  It also counts background tasks, and only `Stop` and `SubagentStop` carry them, as the `background_tasks` still in flight when the turn or the subagent ends (documented, [hooks](https://code.claude.com/docs/en/hooks.md#stop-input), as of 2026-11-15).
  Of these states, Plates shows thinking, as [Titles and summaries](#titles-and-summaries) says, and the background tasks that keep a tile working, as [Background tasks](#background-tasks) says.
- An event from a `session_id` Plates doesn't show adds that session's tile, in the state the event moves a tile to, and in working for an event that moves none, such as `PostToolUse` for a tool that ran without an approval.
  A `SessionStart` with a `source` of `resume` is one exception, since opening a session in the app isn't something happening in it, and an event from a Claude Code session the app has moved away from, while a tile holds the app's session, is another ([Sessions the app moves](#sessions-the-app-moves)).
  A `PreCompact` with a `trigger` of `auto` is a third, for the idle `PreCompact` above, so a compaction in a turn of such a session adds its tile at the turn's next event instead.
- Opening a session in the Claude desktop app fires no event, but the app then saves the time in its file for the session as `lastFocusedAt` (read in the Claude desktop app 5.11492.5.0, app.asar, the handler that logs `desktop_ccd_session_visibility_changed`; [hooks log](log.md#2026-11-15-the-app-records-when-you-open-a-session-but-not-whether-a-turn-asked-you-anything)).
  So once a second, Plates reads each session's file, and a `lastFocusedAt` later than a turn's end reads the turn: an unread tile moves to done, and a your-turn tile is marked read.
  The app sets it only as a session becomes visible, so a turn that ends while you watch its session stays unread until you click its tile or open the session again.
- In the Claude desktop app, `/clear` ends the session, with a `SessionEnd` `reason` of `other`, and goes on under a new `session_id`, so its tile ends and a new tile starts ([hooks log](log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)), unless the app's file names the new session, which is unmeasured ([Left open](#left-open)).
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

## Background tasks

`Stop`'s `background_tasks` lists the tasks still running as the turn ends, each with an `id`, a `type`, a `status`, and a `description` (documented, [hooks](https://code.claude.com/docs/en/hooks.md#stop-input), as of 2026-11-15).

- A `subagent` task, a background agent, reports back to the session, and its report starts a new turn, which fires `UserPromptSubmit` ([hooks log](log.md#2026-11-15-stop-lists-the-tasks-still-running-and-a-background-agents-report-starts-a-new-turn)).
  So while one of them runs, the tile stays working, as [02-tiles](../rack/02-tiles.md#session-tiles) says, until the turn the report starts.
- A `workflow` task counts too, since a workflow's result arrives as a task notification when it completes (read in the Workflow tool's own description, Claude Code 2.1.284), and so does a `teammate` task, on the guess that it reports back the same way (unverified; ending a turn with a teammate running and watching for the turn its message starts would settle it).
- A `shell` task doesn't keep the tile working, because a background command in a desktop session has no time limit and runs until it exits or someone stops it (documented, [tools reference](https://code.claude.com/docs/en/tools-reference.md#background-commands), as of 2026-11-15), so counting one would keep a dev server's tile working for good.
  A turn that leaves a background build running so Claude can pick up its result therefore shows as your turn or unread, unless Claude calls `wait`, as [02-waits-and-pings](../tasks/02-waits-and-pings.md) designs.
  The build's exit starts a new turn in a desktop session (measured on macOS 26.5.2, 2026-11-20, [hooks log](log.md#2026-11-20-on-macos-a-background-commands-exit-starts-a-new-turn-in-a-desktop-session), and on Windows 11 25H2, 2026-11-20, [tasks log](../tasks/log.md#2026-11-20-a-session-ended-a-turn-waiting-on-two-background-commands-without-calling-wait)).
  On macOS that turn fires `UserPromptSubmit` (measured on macOS 26.5.2, 2026-11-23, [hooks log](log.md#2026-11-23-on-macos-a-background-commands-notification-that-arrives-after-its-turn-has-ended-fires-userpromptsubmit)), and on Windows a task notification fired `UserPromptSubmit` ([Events](#events)), though not one we know arrived after its turn had ended.
- A `monitor` task doesn't keep the tile working, because what a `monitor` entry is remains unverified: the hooks docs give it the MCP `server` and `tool` fields an `MCP task` has, while the Monitor tool's watch ends within 30 minutes and has Claude take up each line it prints (documented, [tools reference](https://code.claude.com/docs/en/tools-reference.md#monitor-tool), as of 2026-11-15).
  Reading a `Stop` body from a turn that ends with a watch running would settle it.
- A `cloud session` or `MCP task` entry doesn't keep the tile working either, since whether either reports back into the session is unverified; ending a turn with one running and watching for a turn its result starts would settle it.

## The app's plan

The Claude desktop app has Claude keep a plan for each request that edits files or runs more than one command: a list of steps, each pending, in progress, or completed, which the app shows under Claude's latest message (read in the system prompt the app gives Claude, Claude desktop app 6.28899.2.0 on Windows 11 25H2, as of 2026-11-21).
Claude keeps it with the app's `mcp__ccd_turn__update_plan` tool, and Plates keeps a copy from each call's `PreToolUse`, for a working tile's [hover card](../rack/02-tiles.md#the-hover-card).
Each call changes the copy as the app changes its own plan (read in the Claude desktop app 6.28899.2.0, `app.asar`, its handler for `update_plan`):

| The call passes | What it does to the plan |
| --- | --- |
| `items`, a list of items each with a `content` and a `status`, beside a `scope` of `first_task`, `continuation`, or `new_task` | replaces it with the first 12 items that have text in their `content`, each pending unless its `status` is `completed` or `in_progress`; a call whose `scope` is missing or another value, or whose `items` holds no such item, clears it |
| `done`, a list of step numbers counted from 1 | marks each of those steps completed |
| `start`, a step number | marks that step in progress |
| `advance` of `true` or `"true"` | marks the first step in progress completed, then marks in progress the first pending step after it; with no step in progress, or no pending step after it, it marks in progress the first pending step in the plan |

- A call that passes `items` ignores any `done`, `start`, or `advance` beside it.
  A call without `items` applies `done`, then `start`, then `advance`, and changes nothing if a number it passes names no step, such as 0, or 9 in a plan of 8 steps.
- Plates ignores the call's `PostToolUse` and `PostToolUseFailure`.
  Claude sends the call in the same message as the next step's first tool call (read in the system prompt the app gives Claude, Claude desktop app 6.28899.2.0 on Windows 11 25H2, as of 2026-11-21), so the call's `PostToolUse` can arrive while that tool still runs, and counting it would mark that tool finished early (unverified; logging the hooks of a message that pairs the call with a long `Bash` command would settle it).
- When a turn begins, Plates keeps a plan that still has a pending step, so Claude can carry it on with `advance`, and drops a plan with none.
  The app's instructions have Claude finish a request with no step pending: Claude passes `advance` beside its wrap-up after the last step, or, in another wording of the instructions, leaves the last step in progress for the app to check off as the turn ends (read in the system prompt the app gives Claude, Claude desktop app 6.28899.2.0 on Windows 11 25H2, as of 2026-11-21).
  Whether the app keeps a plan from one turn to the next is unverified; a turn that advances the plan left by the turn before it, watched in the app, would settle it.
- Plates doesn't save the plan in `sessions.json`.
  When Plates starts, it replays each session's calls to `update_plan` from the session's transcript, which brings back the calls Claude made while Plates was closed too, and leaves out a subagent's calls.
  At each prompt the transcript holds, the replay drops a plan with no pending step, as a turn's start does; a prompt that came while Claude was busy, as a task notification does, is a `queued_command` attachment rather than a `user` record ([hooks log](log.md#2026-11-21-a-transcript-records-a-prompt-that-waited-for-claude-as-a-queued_command-attachment)).
  Whether such a prompt starts a turn of its own or joins the running one is unverified, and if it joins, the replay can drop a finished plan while its turn still runs.
- A session whose tile Plates adds while its plan runs, such as one you cleared from the rack, shows the turn's newest tools until Claude's next call with `items`, since only startup replays a transcript.

## Titles and summaries

- A tile's title is the one the Claude desktop app shows for the session, which Plates reads from the app's own file for that session, the file whose `cliSessionId` or `priorCliSessionIds` holds the hook's `session_id` ([hooks log](log.md#2026-11-15-the-claude-desktop-app-keeps-a-file-per-session-with-its-title-and-its-claude-code-session-ids)).
  Plates reads the file again on any hook event after the app changes it, and in the once-a-second read of each session's file that [Events](#events) describes, so a title the app generates or you rename shows within about 1s ([hooks log](log.md#2026-11-16-the-app-shows-a-session-detached-from-its-parent-on-its-own)).
  A session with no file, such as one in a terminal, takes its title from the sources below, and so does a session whose file Plates can't read.
  The app doesn't document these files, so an update to the app can make them unreadable.
- Without the app's title, a tile's title is the session's custom title, which `SessionStart` and `UserPromptSubmit` carry in `session_title` when one is set, such as with `/rename`, and leave out for a title the Claude app generates.
  A title set during a session reaches Plates with your next prompt.
- A session with neither takes the first line of the first prompt you typed in it as its title.
  The transcript gives it for a session Plates first saw after that prompt, and `UserPromptSubmit`'s `prompt` gives it otherwise.
- The title skips the blocks of context the Claude desktop app puts before a prompt's text, such as `<artifact-view-context>` ([hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).
  A subagent's report or a task notification is nothing but such a block, so it can't name a session.
- The session's folder comes from `cwd`, and its name is the tile's title until another title arrives.
- The app's file for a session that another of its sessions started holds `spawnedFrom`, whose `sessionId` is the app's own ID for the session that started it ([hooks log](log.md#2026-11-15-the-apps-file-for-a-sub-session-names-the-session-that-started-it)).
  Plates reads it with the title, and when the event that adds a sub-session's tile finds the file, Plates moves the tile under its parent's before the rack draws it.
  A new sub-session's first events find no file naming its parent, so its tile first appears after the last session tile ([hooks log](log.md#2026-11-16-a-new-sub-sessions-first-events-come-before-the-apps-file-names-its-parent)).
  Plates searches again 2s after a miss, doubling the wait after each miss up to 30s, and the first search that finds the file naming the parent moves the tile under it, once, whether or not an event has come.
- A sub-session's file can also hold `lineageDetached` true, sometimes with a `lineageDetachedBy` of `user` or `agent`, and the app's sidebar then shows the session on its own rather than under the session that started it (seen by the user on Windows 11 25H2, 2026-11-16, [hooks log](log.md#2026-11-16-the-app-shows-a-session-detached-from-its-parent-on-its-own)).
  Plates gives such a session no parent, so within about 1s of the app detaching a session, its tile leaves its parent's family without the indent, as [01-core](../core/01-core.md#tiles) says.
- An approval's summary line is built from the tool the `PermissionRequest` names, or else from the `PreToolUse` before it: "Wants to run: " and the command for a shell command, "Wants to edit: " and the file for an edit, and "Wants to use: " and the tool's name for any other tool.
  Where neither names a tool, the summary is the `Notification`'s `message`.
- Your turn's summary line is the first line of `Stop`'s `last_assistant_message`, because the transcript isn't guaranteed to hold Claude's final message when `Stop` fires (documented, [hooks](https://code.claude.com/docs/en/hooks.md#stop-input), as of 2026-11-15).
  The hover card shows the rest of the message, as [02-tiles](../rack/02-tiles.md#the-hover-card) says.
- A working tile's summary line is the first line of the newest text Claude wrote since your last prompt, for 4s after that text changes, so the text can be read even when a tool follows it at once (user, 2026-11-15).
  No hook event carries the text mid-turn, so Plates reads it from the end of the transcript after each tool event, and it can trail Claude by a record or two.
- After those 4s, the line shows what Claude is doing with a tool from its `PreToolUse` until its `PostToolUse`, and for at least 1s after its `PreToolUse`, so a tool that runs for a few milliseconds can still be read.
- Otherwise the line is "Thinking…", the Claude desktop app's own word for a thinking step (read in the Claude desktop app 5.11492.5.0, ion-dist.asar, the status-line component whose props include `isCompacting`, `apiRetry`, and `toolLabel`, [results/2026-11-15-events-and-app-states.txt](results/2026-11-15-events-and-app-states.txt)).
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

When Plates starts, it reads each restored session's transcript to catch up on what the session did while Plates was closed ([hooks log](log.md#2026-11-15-a-transcript-shows-when-a-turn-ended-through-claudes-end_turn-record)).
Its newest `user` or `assistant` record, outside a subagent, says where the newest turn stands: a marker record says you interrupted Claude or denied a tool, a Claude record with a `stop_reason` of `end_turn` says the turn finished, with its text and its last wrap-up, and any other record says the turn goes on.
The turn counts only if that record's `timestamp` is later than the tile's last change.

## Sessions the app moves

The Claude desktop app can move one of its sessions to a new Claude Code session, and the hooks then carry the new session's `session_id` (measured on Windows 11 25H2, 2026-11-16, [hooks log](log.md#2026-11-16-a-rewind-moves-a-desktop-session-to-a-new-claude-code-session)).
It did so after what the app's file records as a rewind, which nobody watched happen.
On macOS the app does the same after a rewind, and the old Claude Code session sends `SessionEnd` before the new one's first event (measured on macOS 26.5.2, 2026-11-20, [hooks log](log.md#2026-11-20-on-macos-a-rewind-splits-a-sessions-tile-for-about-6s-before-it-merges-back)).
The app's file for the session keeps its `sessionId`, names the new session as `cliSessionId`, and lists the earlier one in `priorCliSessionIds`.

In the once-a-second read of each session's file that [Events](#events) describes for `lastFocusedAt`, when the file's `cliSessionId` names another Claude Code session, the tile follows the session to it, in the same place on the rack.
An event from a `session_id` Plates doesn't show does the same when the app's file already names that ID as its `cliSessionId` and a tile, shown or dismissed, holds the file's `sessionId`.
A tile that holds the app's `sessionId` follows only the file with that `sessionId`, so if another of the app's files lists the tile's `session_id` in its `priorCliSessionIds`, which we haven't seen, that file moves nothing.
A tile that follows its session catches up with the new session's transcript, as a restored tile does, so a move made while Plates was closed shows within about 1s of Plates starting.
Without an event from the new session to give its transcript's path, Plates takes `<new session_id>.jsonl` beside the old transcript, which is where Claude Code put the new one (same entry).
If Claude Code hasn't written that file yet when the tile follows, Plates next reads it when Plates starts, and until then the new session's own events keep the tile up to date.
If the old session's tile shows ended, such as after the old session's `SessionEnd`, it comes back as new before it catches up, since the session it now follows goes on.

If the new session's events reach Plates before the app's file names it, they give it a tile of its own at the end of the rack.
At the next read of the file, the old tile takes that tile's state, and that tile goes.
The old tile keeps its place, on the rack or off it, and a snooze or a reminder for tomorrow.
Without either, it takes that tile's timer.
Until Plates reads the file for the new session, it keeps the app's title and parent.
A dismissed tile that isn't snoozed goes instead, since the new session's events have brought the session back to the rack, as they would have brought the dismissed tile back.
An event from a Claude Code session the app has moved away from, which the app's file lists in `priorCliSessionIds`, adds no tile while a tile holds the app's session.

## Sessions the app archives

The app's file for a session holds `isArchived` on Windows and macOS (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-the-claude-desktop-app-keeps-a-file-per-session-with-its-title-and-its-claude-code-session-ids), and on macOS 26.5.2, 2026-11-27, [hooks log](log.md#2026-11-27-the-claude-desktop-apps-file-says-whether-the-app-archived-the-session)).
In the once-a-second read of each session's file that [Events](#events) describes, an `isArchived` of true takes the session's tile off the rack for good, whether it's shown, dismissed, or snoozed, since the app still keeps the session (user, 2026-11-27).
Its task list's file goes with it ([01-task-list](../tasks/01-task-list.md#saving)), and the tiles of its sub-sessions stay until the app archives each of them.
While the file says the session is archived, its events bring no tile back, so an event it sends after it's archived, such as a `SessionEnd`, adds no ended tile (unverified whether archiving sends any).
A session you unarchive gets a new tile with its next event that brings one, as a session you cleared does.

A session in the app can archive other sessions with the app's `archive_session` tool (read in the tool list the Claude desktop app gives a session, as of 2026-11-27), as an orchestrating session might archive its sub-sessions once their work lands.
Their tiles leave the rack at Plates' next read of their files after the app writes `isArchived` (unverified that the tool sets it as archiving in the app does).

## Installing

`plates hooks install` adds Plates' hooks to Claude Code's user settings file, `~/.claude/settings.json`.
It shows you the exact change, backs the file up beside itself, and writes the change only once you confirm.
`plates hooks uninstall` removes only the entries Plates added.
`plates hooks install` also registers Plates' task server, and `plates hooks uninstall` removes it, as [01-task-list](../tasks/01-task-list.md#installing) describes.

"Connect to Claude Code", in the rack's menu on its grip band and in the menu of Plates' icon in the tray or the menu bar, makes the same change (user, 2026-11-24):

- The item is checked while Claude Code's settings hold every hook this copy of Plates writes, and none other of Plates', and register this copy's task server.
- While the settings hold Plates' hooks or task server but not as this copy writes them, as after Plates moved or once a newer Plates listens to another event, the rack's menu shows "Update needed" in the your-turn text color where the check would be, and the tray's item reads "Connect to Claude Code (update needed)", still checked.
- Clicking it opens a dialog that says what changes and asks first, in place of the install's question: Connect, Update, or Disconnect beside Cancel on macOS, and OK and Cancel on Windows.
- The rack reads the settings every 5s, so the item also follows a change `plates hooks install` makes.

Plates opened from Finder or at login may not have the folder `claude` is in on its `PATH`, so it runs the first `claude` it finds on `PATH`, then in `~/.local/bin`, then on macOS, where an interactive login shell finds it ([hooks log](log.md#2026-11-24-on-macos-a-login-shell-started-from-a-bare-path-didnt-find-claude-and-an-interactive-login-shell-did)).
If it finds none, Plates still connects the hooks, the dialog that follows lists the `claude` command left to run, and the item reads "update needed" until it runs.

On Windows, uninstalling Plates disconnects it first, from Velopack's uninstall hook, which Velopack runs only on Windows (read in velopack 1.2.161, `src/app.rs`, `on_before_uninstall_fast_callback`).
On macOS, moving Plates to the Trash runs nothing of Plates', so its hooks stay in Claude Code's settings until you disconnect.

Each hook is an async command hook, which Claude Code starts and doesn't wait for, and which runs `plates-hook` directly, with no shell.
`plates-hook` reads the event from stdin and forwards it to the receiver on `127.0.0.1:47231`.
While Plates is closed, `plates-hook` gives up after 300ms, in the background, and exits without an error.
An `http` hook straight to the receiver would cost a session about 2s for each event while Plates is closed, because Windows refuses a connection to a port nothing listens on only after about 2s ([hooks log](log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).

The receiver answers each request with an empty 200 before it acts on the event, so a slow Plates never holds up `plates-hook`.

## Opening a session

On Windows, clicking a session tile opens `claude://code/continue?session=local_<id>`, where `local_<id>` is the `sessionId` in the app's file for the session, and the Claude desktop app shows that session (measured on Windows 11 25H2, 2026-11-15, [hooks log](log.md#2026-11-15-the-claude-desktop-app-opens-one-session-from-a-continue-link-with-its-own-id)).
The link is undocumented: we read it in the app's code and measured it from a shell, so an update to the app can change it.
On Windows, a program can bring its window to the front only in a few cases, such as when a process that can set the foreground allows it (documented, [SetForegroundWindow](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow), as of 2026-11-15).
The rack never takes the foreground, but its process got the last input, the click on the tile, which lets it call `AllowSetForegroundWindow` with `ASFW_ANY`, so Plates makes that call before it opens the link (documented, [AllowSetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow), as of 2026-11-15).
Whether the app then comes to the front over another window is unverified, and clicking a tile while another window covers the app would settle it.
A session the app doesn't keep, such as one in a terminal, has no such ID, and clicking its tile does nothing.
On macOS, the app keeps its session files in `~/Library/Application Support/Claude/claude-code-sessions/`, in the same form, and macOS hands `claude://` links to the app (measured on macOS 26.5.2, 2026-11-16, [hooks log](log.md#2026-11-16-on-macos-the-app-keeps-its-session-files-under-application-support-and-opening-a-session-adds-its-tile-through-a-resume)), so clicking a tile there opens the same link.
The app showed the session from it (seen by the user on macOS 26.5.2, 2026-11-16).
The rack takes the keyboard on a click without making Plates the active app, and macOS activates an app at another's request once the active app yields to it (documented, `activateFromApplication:options:` in AppKit's `NSRunningApplication.h`).
So on a tile's click, Plates makes itself the active app, waits up to 300ms for macOS to make it so, yields to the Claude app, opens the link through `NSWorkspace`, and asks for the app to be activated from Plates.
The Claude app was in front within 100ms, and your typing went to it ([rack log](../rack/log.md#2026-11-16-on-a-mac-a-tiles-click-hands-the-keyboard-to-the-claude-app-once-plates-makes-itself-active-and-yields-to-it)).
Opening the link through the `open` command, or through `NSWorkspace` without Plates being active, showed the session while the rack kept the keyboard (same entry).
Before macOS 14, which brought yielding, Plates asks for the app to be activated without yielding.

## Answering a wrap-up

No supported channel lets a program outside the Claude desktop app send a message into one of its sessions ([hooks log](log.md#2026-11-15-no-supported-channel-puts-a-message-into-a-claude-desktop-app-session)), so when you click one of a wrap-up's choices on a [hover card](../rack/02-tiles.md#the-hover-card), Plates presses the app's own button for that choice.
On Windows it presses through UI Automation ([hooks log](log.md#2026-11-15-ui-automation-can-press-a-sessions-wrap-up-button-and-send-a-message-from-its-message-box)), and on macOS through the Accessibility API, the interface VoiceOver reads apps through ([hooks log](log.md#2026-11-16-on-macos-the-accessibility-api-shows-the-apps-session-page-under-the-name-ui-automation-does)).

1. Plates opens the session's link, as [Opening a session](#opening-a-session) says.
   On Windows the app then shows the session, and on macOS whether it does is unverified, as that section says.
2. Plates looks through the app's windows for the session's web page, named "<session title> - Claude Code", with the title from the app's file for the session.
   Matching the title means Plates can press a button in the wrong session only if that session has the same title.
   On Windows, the windows are the visible `Chrome_WidgetWin_1` windows of `claude.exe`, and the page is a `Document`.
   On macOS, the windows are those of the app with the bundle ID `com.anthropic.claudefordesktop`, and the page is an `AXWebArea` whose `AXTitle` holds the name.
   The app is built on Electron, which turns on its accessibility features when a third-party program sets the app's `AXManualAccessibility` attribute, and on its own while VoiceOver runs (documented, [Electron accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility), as of 2026-11-16), so on macOS Plates sets the attribute before it looks.
   Whether the page's tree of controls is empty without it is unverified, since our macOS probe, `experiments/areas/hooks/ax-answer/probe.swift`, always set it.
3. In that page, it looks for the group named "Suggested next actions", and in it, an enabled button named with the choice's label, looking again every 100ms.
   If none shows within 3s, Plates presses nothing and leaves the session open in the app.
   On macOS, every named element the probe read had its name in `AXTitle`, and one named by an `aria-label`, such as "Show sidebar", had it in `AXDescription` too (measured on macOS 26.5.2, 2026-11-16, [hooks log](log.md#2026-11-16-on-macos-the-accessibility-api-shows-the-apps-session-page-under-the-name-ui-automation-does)).
   Plates checks both, in case the wrap-up's group or buttons carry the name only in `AXDescription`, and which of the two held their names when a press worked is unmeasured.
4. It presses the button, through InvokePattern on Windows and the `AXPress` action on macOS, which sends the label as your message, as a click on it would.

Plates presses on a thread of its own, because the press waits on the app for as long as step 3 allows, and the rack's thread must keep drawing.
On Windows, that thread owns no window and runs in COM's multithreaded apartment, as Microsoft directs for a UI Automation client that may reach its own UI (documented, [Understanding Threading Issues](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading), as of 2026-11-16).

On Windows, pressing a choice from a hover card answered a throwaway session (measured on Windows 11 25H2, 2026-11-16).
On macOS, pressing a choice from a hover card pressed the app's button for it (seen by the user on macOS 26.5.2, 2026-11-16, [hooks log](log.md#2026-11-16-on-macos-the-accessibility-api-shows-the-apps-session-page-under-the-name-ui-automation-does)), and Plates walked all 1,348 elements of a session's page in 63ms (measured on macOS 26.5.2, 2026-11-16, same entry).

On macOS, a program can read and press another app's controls only once you turn it on under System Settings > Privacy & Security > Accessibility (documented, [AXIsProcessTrustedWithOptions](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions), as of 2026-11-16).
Until you do, a choice opens the session and presses nothing, and Plates asks macOS through `AXIsProcessTrustedWithOptions` with `kAXTrustedCheckOptionPrompt`, which has macOS offer to open that setting (how often macOS shows the offer is unverified; clicking two choices before turning it on would settle it).
macOS gives the permission to the program responsible for a process, so a `plates` started from a shell runs with the permission of whatever started the shell, which for a Claude Code shell is Claude Code's `claude` ([hooks log](log.md#2026-11-16-on-macos-the-accessibility-api-shows-the-apps-session-page-under-the-name-ui-automation-does)).
An opened `Plates.app` asks for its own, and whether turning it on then works is unverified: System Settings didn't list the probe wrapped in an app until it was added by hand, which we didn't finish (same entry).
Whether an update to `Plates.app` keeps the permission depends on how the release is signed, since macOS ties an ad-hoc signed app's permission to its exact build (unverified; updating an installed `Plates.app` with the permission on would settle it).

A press can miss.
For an app action, such as `commit`, the app draws the button with its own text and ignores the action's `label` (read in the wrap-up tool's own description, Claude desktop app 5.11492.5.0), while the card's choice carries that `label`, or Plates' words for the action where it's empty, such as "Commit" ([Events](#events)).
If the two differ, Plates presses nothing (unverified; a wrap-up offering `commit` would settle it).
The element names are undocumented, so an update to the app can rename them, and then nothing is pressed either.

On Windows, the UI Automation probe found the session's web document empty on its first walk, built 300ms later, and built on the first walk of every later run ([hooks log](log.md#2026-11-15-ui-automation-can-press-a-sessions-wrap-up-button-and-send-a-message-from-its-message-box)).
If a client's walk is what makes the app build the tree, the app keeps it built after Plates' first press for as long as it runs, and setting `AXManualAccessibility` could do the same on macOS (unverified; timing the app's CPU with and without Plates having pressed, on each OS, would settle both).

## Left open

- Whether Plates opened from Finder has `~/.local/bin` on its `PATH`, and whether the interactive shell finds `claude` from there, which the [hooks log](log.md#2026-11-24-on-macos-a-login-shell-started-from-a-bare-path-didnt-find-claude-and-an-interactive-login-shell-did) simulated from a terminal; clicking "Connect to Claude Code" in an installed `Plates.app` would settle it.
- Whether `claude` is in `~/.local/bin` on Windows, and whether a dialog of Plates' comes to the front there; clicking the item in an installed Plates on Windows would settle both.
- What Plates does when another program holds port 47231, which `receiver::PORT` sets.
- Sessions that started while Plates was closed, whose tiles appear only at their next event, since Plates restores only the sessions it had.
- Whether the app's file for a desktop session names the session a `/clear` starts as its `cliSessionId`. If it does, the new session's tile takes the ended tile's place, as [Sessions the app moves](#sessions-the-app-moves) says, rather than an ended tile staying beside a new one, as [Events](#events) says; typing `/clear` in a desktop session and reading the app's file for it would settle it.
- Whether, and how soon, the app writes `isArchived` after you archive a session, whether archiving ends the Claude Code session with a `SessionEnd`, and whether a session archiving another with `archive_session` sets `isArchived` the same way ([Sessions the app archives](#sessions-the-app-archives)); archiving a session by hand and from another session, while running `plates listen --log <file>` with Plates closed, and reading the session's file, would settle all three.
- Whether the old Claude Code session sends any event after the app's file names the new one, which Plates drops while a tile holds the app's session ([Sessions the app moves](#sessions-the-app-moves)); its `SessionEnd` comes before ([hooks log](log.md#2026-11-20-on-macos-a-rewind-splits-a-sessions-tile-for-about-6s-before-it-merges-back)), and running `plates listen --log <file>` with Plates closed, while rewinding a session as Claude replies, would settle the rest.
