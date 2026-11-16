# Hooks: log

What we found about Claude Code's hooks and the Claude desktop app, newest entry first.

## 2026-11-16: A new sub-session's first events come before the app's file names its parent

**Asked:** why does a sub-session started while Plates runs show as a session of its own, and go under its parent's tile only after Plates restarts?

**Expected:** a file the app had not yet written, which the hooks design left open: a tile whose first event finds no file stays after the last session tile.

**Found:** the sub-session's tile appeared after the last session tile, and stayed there while the session worked, and after a restart it sat under its parent's tile (seen by the user on macOS 26.5.2, Claude desktop app 5.11492.5, 2026-11-16).
So the first events found no file naming the parent, and the app's file named it by the restart.
Plates moved a tile under its parent only for the event that added the tile, and after a miss searched for the file again only 30s later, so nothing moved the tile once the file named the parent, and only a restart, which gathers each family as it loads, did.

**Changed:**

- The first event that learns a tile's parent moves the tile under it, once.
- A search that finds no file for a session tries again after 2s, doubling the wait after each miss up to the 30s it waited before.

How soon after a sub-session starts the app's file names its parent is unmeasured, and watching where a new sub-session's tile goes, second by second, would measure it.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md): the paragraph on sub-sessions.

## 2026-11-16: The Claude desktop app asks for AskUserQuestion through PermissionRequest

**Asked:** why did a session waiting on Claude's question show as needing approval, with "Wants to use: AskUserQuestion"?

**Expected:** a question to arrive as `PreToolUse` for `AskUserQuestion`, which Plates read as an ordinary tool.

**Found:** the tile moved to needs approval, which only `PermissionRequest` and the `permission_prompt` notification do, with the `AskUserQuestion` tool's summary line, while the app showed the session as "Needs your input" with the question (seen by the user on macOS 26.5.2, Claude desktop app 5.11492.5, 2026-11-16).
The app hosts Claude Code through the Agent SDK's `canUseTool` callback, which asks for every tool that way ([entry](#2026-11-15-claude-code-documents-the-final-message-on-stop-failure-events-and-the-session-title-on-every-prompt)), so a question asks for permission like any tool.

**Changed:** `PreToolUse` and `PermissionRequest` for `AskUserQuestion` move the tile to needs approval as a question, with "Asks: " and the first line of the first question as its summary line, and the hover card says "Needs your input" for it, as it now does for an MCP server's `Elicitation`.
The tool's `PostToolUse` moves the tile back to working once you answer, as for any tool.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md): a row for `AskUserQuestion`, and the `Elicitation` row. [02-tiles](../rack/02-tiles.md): the needs approval row.

## 2026-11-16: On macOS, the app keeps its session files under Application Support, and opening a session adds its tile through a resume

**Asked:** why does clicking a session tile on macOS open nothing, and why does opening a session the rack doesn't show add its tile, with nothing new in the session?

**Expected:** no app ID behind the click, since Plates looked for the app's session files only on Windows, and behind the tile, the `SessionStart` with a `source` of `resume` that opening an older session sent on Windows ([entry](#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)).

**Found:** on macOS, the app keeps the same files as on Windows, and the open came through `SessionStart` (measured on macOS 26.5.2, Claude desktop app 5.11492.5, 2026-11-16).

- The app keeps a file per session at `~/Library/Application Support/Claude/claude-code-sessions/<id>/<id>/local_<id>.json`, 200 files and 63MB on this Mac, with the keys Plates reads on Windows: `sessionId`, `cliSessionId`, `title`, and `lastFocusedAt`.
- macOS hands `claude://` links to `/Applications/Claude.app`.
- Two sessions the user opened in the app, with no prompt since, had tiles in the new state, which only `SessionStart` sets.

**Changed:**

- Plates reads the app's session files on macOS from that folder, so a tile there has the app's ID and title, and a click opens its `claude://` link.
- A `SessionStart` with a `source` of `resume` adds no tile for a session Plates doesn't show, and the tile comes with the session's next prompt or tool.
- On macOS, a wrap-up's choices in the hover card stay labels, since Plates presses the app's buttons through Windows' UI Automation.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md): the `SessionStart` row, the rule for a session Plates doesn't show, and "Opening a session".

## 2026-11-15: UI Automation can press a session's wrap-up button and send a message from its message box

**Asked:** with no supported channel into a desktop session, can another program answer one through Windows UI Automation, by pressing the app's own wrap-up button or typing into its message box?

**Expected:** the buttons exposed, but a value set from outside left unsent, since the message box is a ProseMirror editor that listens for input events.

**Found:** a probe in `experiments/areas/hooks/uia-answer/` read the app's UI Automation tree, then pressed a button and sent a message in a throwaway session the user named (measured on Windows 11 25H2, 2026-11-15, Claude desktop app 5.11492.5.0; [the probe's results](../../../experiments/areas/hooks/uia-answer/results/)).

- The app's provider is Chromium's own UI Automation provider, and its tree holds only the session the app shows.
  The session's web document is named "<session title> - Claude Code".
- The session's wrap-up showed in the tree 765ms to 843ms after `claude://code/continue?session=local_<id>`, across three runs.
- The probe's first walk found 24 elements, with the session's web document empty, and a walk 300ms later found 1,068; every later run found the tree built on its first walk.
  `UiaClientsAreListening` was already true before that first walk, so whether the app builds the tree only once a client walks it is unverified.
- The wrap-up's buttons are `Button` elements named with their labels, such as "Yes" and "No", with InvokePattern, in a group named "Suggested next actions", beside a `Dismiss` button.
  Their AutomationIds, such as `_r_6b16_`, look generated, so the probe found them by name.
- `Invoke` on "No" sent "No": 4s later the transcript held a `user` record with `origin.kind` `human` and the text "No", and Claude's reply.
  The buttons then left the tree, and the answered question stayed as text.
- The message box is an `Edit` named "Prompt", with ValuePattern and TextPattern, and it takes keyboard focus when the app switches sessions.
  `SetValue` filled it, and a `Send` button appeared that isn't in the tree while the box is empty; `Invoke` on it sent the text as a `user` record with `origin.kind` `human`.
- The app calls `focus()` on its main window when a link reaches it (read in the Claude desktop app 5.11492.5.0, `app.asar`, its `second-instance` handler), and the app was already in front in every run, so whether it comes in front of another app is unmeasured.

**Changed:** clicking one of the wrap-up's choices on a hover card opens the session and presses the app's button for that choice.

**Affects:** [02-tiles](../rack/02-tiles.md#the-hover-card) and [01-claude-code-hooks](01-claude-code-hooks.md#answering-a-wrap-up).

## 2026-11-15: The app's file for a sub-session names the session that started it

**Asked:** can Plates draw a sub-session, a session that another session started, under the starting session's tile?

**Expected:** a field in the app's file for each session, since no hook event names another session.

**Found:** we read the app's file for a session this repository's own session had started with the app's `mcp__ccd_session__start_session` tool, and counted the app's files that hold the same field (measured on Windows 11 25H2, 2026-11-15, Claude desktop app 5.11492.5.0, [results/2026-11-15-spawned-from.txt](results/2026-11-15-spawned-from.txt)).

- The file holds `spawnedFrom`, an object with `sessionId`, `taskId`, `title`, `sideSession` of true, and `name`.
  Its `sessionId` is the starting session's own app ID, such as `local_e4c2b1b0-16ce-4c53-be73-25a5e5fc8bd2`.
- 42 of the app's 354 session files on our PC hold a `spawnedFrom`, and every one has a `sessionId` and a `taskId`, while 36 of them mark `sideSession` true.
- The hook events we captured, none of them from a sub-session, carry no field naming another session ([results/2026-11-15-desktop-session-events.txt](results/2026-11-15-desktop-session-events.txt)); whether a sub-session's own events do is untested.
- Whether the app writes the new session's file before its first hook event is unverified, since we read the file after the fact.

**Changed:** Plates reads `spawnedFrom.sessionId` with the session's title, and draws a sub-session's tile under its parent's.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#titles-and-summaries), [02-tiles](../rack/02-tiles.md#layout), and [01-core](../core/01-core.md#tiles).

## 2026-11-15: Stop lists the tasks still running, and a background agent's report starts a new turn

**Asked:** a session's tile showed your turn while the session waited on two background agents it had started; can Plates tell such a turn from one that's yours?

**Expected:** no, since no hook event fires as a background task starts or finishes.

**Found:** we read the docs, and watched a session in the Claude desktop app (documented, [hooks](https://code.claude.com/docs/en/hooks.md#stop-input) and [tools reference](https://code.claude.com/docs/en/tools-reference.md#background-commands), as of 2026-11-15; measured on Windows 11 25H2, 2026-11-15, Claude Code 2.1.284).

- `Stop`'s `background_tasks` lists each task "in flight" as the turn ends, with an `id`, a `type`, a `status`, and a `description`.
  The docs give `shell`, `subagent`, `monitor`, `workflow`, `teammate`, `cloud session`, and `MCP task` as examples of `type`, which "Falls back to the raw discriminant for unrecognized types", and they don't list the `status` values.
- In this repository's own desktop session, a turn ended with two background agents running, and each agent's report later started a turn of its own, with no prompt from us.
  We haven't captured a `Stop` body whose `background_tasks` holds entries.
- `UserPromptSubmit` fires for a turn a background agent's report starts (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
- A workflow's result arrives as a task notification when it completes (read in the Workflow tool's own description, Claude Code 2.1.284).
  Nothing we read says how a teammate reports back.
- A background command in a local session, a desktop one included, has no time limit, and runs until it exits or someone stops it.
- The docs don't say whether a background command that exits starts a turn in an interactive session.
- The Monitor tool's watch has a deadline of 5 minutes by default and 30 minutes at most, and Claude takes up each line it prints as it arrives; the docs also give a `monitor` entry `server` and `tool` fields, so whether that `type` is the Monitor tool's watch is unclear.
- No hook event Plates installs says that a background command or agent has finished.
  Whether `SubagentStop` does, for a background agent, is unverified ([01-claude-code-hooks](01-claude-code-hooks.md#events)), and `TaskCreated` and `TaskCompleted` follow the task list Claude keeps with its `TaskCreate` tool.

**Changed:** a turn that ends with a `subagent`, `workflow`, or `teammate` still running, and whose wrap-up asks you nothing, keeps its tile working, with "Waiting on 2 background tasks" as its summary line.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#background-tasks) and [02-tiles](../rack/02-tiles.md#session-tiles).

## 2026-11-15: No supported channel puts a message into a Claude desktop app session

**Asked:** can the hover card send a session a message, or answer the question a turn's wrap-up asks, the way the wrap-up's buttons do in the app?

**Expected:** a link that carries text, or the app's own tool for one session to message another.

**Found:** we read the app's code and Claude Code's docs, and printed the environment a Bash command gets in a desktop session (read in the Claude desktop app 5.11492.5.0, app.asar and ion-dist.asar; documented, [cross-session messaging](https://code.claude.com/docs/en/cross-session-messaging.md), [channels](https://code.claude.com/docs/en/channels.md), and [remote control](https://code.claude.com/docs/en/remote-control.md), as of 2026-11-15; measured on Windows 11 25H2, 2026-11-15, Claude Code 2.1.284; [results/2026-11-15-input-channels.txt](results/2026-11-15-input-channels.txt)).

- Of the app's `claude://code/` links, only `code/new` takes text, in `q` or `prompt`, and it fills the message box of a new session without sending it.
  `code/continue` takes only `session` and `source`.
- The app runs each session's Claude Code through the Agent SDK, writing your messages to its stdin, a pipe only the app holds.
- The app's `send_message` tool, which lets one session message another, is an MCP server inside the app's own process, with no port or pipe another program could reach.
- A click on one of the wrap-up's buttons sends its label as an ordinary message from you, and a few app actions, such as `commit`, send a fixed prompt instead.
- The app's local listeners serve sign-ins, artifact pages, and SSH passwords, and none takes a message for a session.
- Claude Code gives each desktop session an inbox for messages from other sessions, on Windows a named pipe such as `\\.\pipe\LOCAL\cc-msg-<32 hex digits>`, whose name and token reach hooks and Bash commands as `CLAUDE_CODE_MESSAGING_SOCKET` and `CLAUDE_CODE_MESSAGING_TOKEN`.
  The docs say a message arriving there is labeled as coming from another session and can't approve a permission, so it can't stand in for an answer of yours.
- An MCP server can push messages into a session only if Claude Code started with the `--channels` flag, a research preview that the app gives us no way to add.
- Remote Control reaches a session through the Anthropic API, with no local API.

**Changed:** the hover card names the wrap-up's choices without acting on them, and clicking one opens the session.
Whether Windows UI Automation can press the app's own button for you is untested.

**Affects:** [02-tiles](../rack/02-tiles.md#the-hover-card).

## 2026-11-15: A transcript shows when a turn ended, through Claude's end_turn record

**Asked:** a session's tile vanished when Plates restarted after the session's turn ended, since no event came to bring it back; can Plates tell from the transcript how a turn stands, to restore and catch up its tiles?

**Expected:** no record of a turn's end, since `Stop` is a hook event.

**Found:** we read the end of the session's transcript, titled "Controller/HOTAS support for Cyberpunk" in the app (measured on Windows 11 25H2, 2026-11-15).

- The turn's last two Claude records, a `thinking` part and a `text` part, both carried a `message.stop_reason` of `end_turn`, and a `system` record with a `subtype` of `stop_hook_summary` came 8s after the text.
- The app's file for the session gave `lastTurnEdgeAt` the time of that `system` record, and Plates had restarted 66s later.
- Records of other types, such as `attachment` and `api-request`, sit among the conversation's, and some carry no `timestamp`.
- `user` records, the tool results included, carry a `promptId`, and Claude's records don't, so we keyed a turn on its records' `timestamp` instead.

**Changed:** Plates saves session tiles to `sessions.json`, restores them on starting, and moves each to what its transcript's newest turn shows, when that turn's record is later than the tile's last change.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#titles-and-summaries), [02-tiles](../rack/02-tiles.md#saving), and [01-core](../core/01-core.md#saving).

## 2026-11-15: The app records when you open a session, but not whether a turn asked you anything

**Asked:** after a finished turn showed as done before the user had read it, can Plates tell a turn you've read from one you haven't, and one that asks you something from one that doesn't?

**Expected:** an unread flag in the app's file for each session.

**Found:** we read the fields of the app's session files and the app code that writes them (read in the Claude desktop app 5.11492.5.0, app.asar and ion-dist.asar; measured on Windows 11 25H2, 2026-11-15).

- The app's file holds `lastFocusedAt` and `lastTurnEdgeAt`, both in milliseconds since 1970, and no unread flag.
- The app sets `lastFocusedAt` to the current time and saves the file when a session becomes visible, in the handler that logs `desktop_ccd_session_visibility_changed`, and not while the session stays visible.
- In the file of the session titled "Plates", `lastFocusedAt` came 22.7s after `lastTurnEdgeAt`, when the user came back to read the turn.
- 29 of the 40 newest files hold `turnWrapUp`, whose only field, `description`, is one sentence about the turn, so the file doesn't say whether the turn asked anything.
- The app's session list picks its own marks, such as "unread", "idle", and `needs_attention`, from state it keeps in memory, in a function in ion-dist.asar that returns "unread" for a session with `hasUnread`.

**Changed:** a turn whose wrap-up asked nothing is unread until the session's `lastFocusedAt` passes the turn's end, or you click its tile, and then it's done.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#events) and [02-tiles](../rack/02-tiles.md#session-tiles).

## 2026-11-15: Claude Code documents 33 hook events, and the app's status line shows 8 states

**Asked:** after a working tile kept showing Claude's last tool while Claude thought, and showed nothing of a `/compact`, which other states does Plates miss?

**Expected:** a few more events beside the 10 Plates installed.

**Found:** we listed every documented event and every state the Claude desktop app's status line draws (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15; read in the Claude desktop app 5.11492.5.0, ion-dist.asar; [results/2026-11-15-events-and-app-states.txt](results/2026-11-15-events-and-app-states.txt)).

- Claude Code documents 33 hook events, and 14 `notification_type` values for `Notification`.
- Of the 23 events Plates didn't install, two pairs change what a tile shows: `PreCompact` and `PostCompact` bracket a compaction, and `Elicitation` and `ElicitationResult` bracket an MCP server waiting for your answer.
- The app's status line has 8 states: "Stopping…", "Compacting session…", a retry label, "Thinking…", a waiting label, a tool's label, "Working…", and a count of background tasks.
  One waiting label is for a message being sent.
- Thinking, retrying, waiting, and stopping have no hook event, and background tasks reach a hook only as `Stop`'s and `SubagentStop`'s `background_tasks`, when a turn or a subagent ends.
- After a `/compact` the user typed, the app showed "Compacted session · saved 920.4k tokens" and no turn followed (user's screenshot, 2026-11-15).

The gaps came from how the hooks design started: it took the events that move a tile between its states and never listed the rest.

**Changed:** Plates installs hooks for `PreCompact`, `PostCompact`, `Elicitation`, and `ElicitationResult`, and the hooks design lists every documented event with what Plates does with it or why it doesn't need it.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#events).

## 2026-11-15: The Claude desktop app opens one session from a continue link with its own ID

**Asked:** can a link open one session in the Claude desktop app, for [the plan's step 6](../product/plan.md#6-opening-a-session-from-its-tile)?

**Expected:** no, since the app documents only `claude://code/new`.

**Found:** we read the app's link handler in its code and then tried the link (read in the Claude desktop app 5.11492.5.0, `app.asar`, the code that logs `claudeURLHandler: code entry link invalid ?session`; measured on Windows 11 25H2, 2026-11-15).

- The app registers the `claude` scheme in its package manifest, and passes each link to its own handler.
- For `claude://code/continue`, the handler takes `session=last`, or a `session` that matches `^local_[A-Za-z0-9-]{1,64}$`, the app's own ID for a session, which its file for the session holds as `sessionId`.
  It looks the ID up among the sessions that aren't archived and shows that session, or the Code page when none matches.
- With the session titled "Stage 18" showing, running `Start-Process 'claude://code/continue?session=local_<id>'` in Windows PowerShell 5.1 with the ID of the session titled "Plates" switched the app to that session, and the same command with Stage 18's ID switched it back.
- The app's code also names `claude://code/continue?session=last` "Continue Last Claude Code Session", and `claude://code/needs-input` "Sessions Waiting for You", as actions in its Linux `.desktop` file, which Windows and macOS don't show.
- None of this is documented, so an update to the app can change it without notice.

**Changed:** clicking a session tile opens that link, with the ID from the app's file for the session.
We haven't yet measured whether the app comes to the front when Plates opens the link while another window covers it.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#opening-a-session) and [02-tiles](../rack/02-tiles.md#session-tiles).

## 2026-11-15: An interrupt and a denial each write a marker record to the transcript

**Asked:** since no hook event fires after you interrupt Claude or deny a tool, does the transcript show either at once?

**Expected:** a `tool_result` with `is_error` for a denial, and nothing for an interrupt.

**Found:** we searched every transcript on our PC, all from the Claude desktop app (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-interrupt-markers.txt](results/2026-11-15-interrupt-markers.txt)).

- An interrupt writes a `user` record whose text is `[Request interrupted by user]`.
- A denial writes the tool's result with `is_error` true and a top-level `toolDenialKind` of `user-rejected`, then a `user` record whose text is `[Request interrupted by user for tool use]`.
  There are 133 `user-rejected` results and 133 transcript lines containing that text, and in the three denials we read in order, the marker came right after the result.
- `toolDenialKind` takes six other values: `permission-rule` (75), `automode-blocked` (46), `automode-unavailable` (33), `interrupted` (32), `cancelled` (31), and `automode-parsing-error` (11).
  We haven't read what follows them, so whether Claude goes on with its turn after each is unverified, and reading the records after a few of each would settle it.
- We haven't timed how soon after an interrupt the record reaches the file.
- Whether Claude Code documents these records is unverified, and searching its documentation for `toolDenialKind` and the marker text would settle it.
  If it doesn't, an update can change them without notice, and a tile would then wait for the session's next event again.

**Changed:** Plates reads what the transcript of each session in a turn gains, once a second, and a marker record moves the tile to your turn.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#events), [02-tiles](../rack/02-tiles.md#session-tiles), and [01-core](../core/01-core.md#events-and-time).

## 2026-11-15: The Claude desktop app keeps a file per session with its title and its Claude Code session IDs

**Asked:** where does the title the Claude desktop app shows for a session come from, since `session_title` carries only a custom title?

**Expected:** nowhere Plates could read it.

**Found:** we searched the app's data folder for this repository's session ID (measured on Windows 11 25H2, 2026-11-15, with the app installed as a Windows package).

- The app keeps one JSON file per desktop session, `local_<id>.json`, under `%LOCALAPPDATA%\Claude-Data\claude_<package id>\claude-code-sessions\<two IDs>\`, 332 of them on our PC.
- Each file holds the session's `title` and its `titleSource`: `auto` for a title the app generated, in 277 files, `user` in 20, `tool` in 25, and none in 10.
- Each file also holds `cliSessionId`, the `session_id` the hooks carry, and `priorCliSessionIds`, which looks like where the app keeps a session's earlier IDs, such as after a `/clear` (unverified).
- It holds `sessionId`, the app's own `local_<id>`, `isArchived`, and `interruptedAwaitingPerson`, which was null in every file (unverified what sets it).
- None of this is documented, so an update to the app can change it without notice.
- Reading all 332 files took about 205ms on our PC, and the file of this repository's session, which had just fired an event, was the second newest by modified time.

**Changed:** a session tile takes the title the app shows from the session's file, ahead of `session_title` and the first prompt, and Plates searches the files newest first by modified time.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#titles-and-summaries).

## 2026-11-15: In a desktop session, a denial and an interrupt fire nothing until the next prompt, and /clear starts a new session

**Asked:** the questions the hooks design left to the plan's step 3: whether `PermissionRequest` fires in a desktop session, what arrives after you allow or deny a tool, what an interrupt fires, whether a session keeps its `session_id` across a resume and a `/clear`, and whether `UserPromptSubmit`'s `prompt` holds only what you typed.

**Expected:** `PermissionRequest` and the `permission_prompt` notification, a `PostToolUse` after an allowed tool, some event after a denial, nothing for an interrupt, the same `session_id` after a `/clear`, with `SessionEnd`'s `reason` `clear`, and prompts from typing only.

**Found:** the user took a session through allowing and denying a command, an interrupt, `/clear`, and closing it, while `plates listen` received its events (measured on Windows 11 25H2, Claude Code 2.1.325 in the Claude desktop app, 2026-11-15, [results/2026-11-15-desktop-session-events.txt](results/2026-11-15-desktop-session-events.txt)).

- `PermissionRequest` fired as Claude asked, with `tool_name`, `tool_input`, and `permission_suggestions`, and the `permission_prompt` notification followed while the approval waited.
- After the command the user allowed, the next event was its `PostToolUse`, once the 10s command had finished.
- After the command the user denied, no event arrived until the user's next prompt, so the tile showed needs approval until then.
- After the prompt the user interrupted, no event arrived until the session's next one, and no `PostToolUseFailure` with `is_interrupt` came.
- `/clear` sent `SessionEnd` with a `reason` of `other`, not `clear`, and the session went on under a new `session_id`, whose `SessionStart` had a `source` of `startup`.
  Closing the session sent `SessionEnd` with a `reason` of `other` too.
- Opening two older sessions sent `SessionStart` with a `source` of `resume`, and each kept its `session_id`, since its transcript, named for that ID, was first written days earlier.
- After the user renamed the session, its next `UserPromptSubmit` carried the name in `session_title`.
- In this repository's own session, `UserPromptSubmit` also fired for a subagent's report and for a task notification, whose `prompt` started with a tag block, `<agent-message` and `<task-notification`.
- Every event also carried `scratchpad_dir` and `prompt_id`, and tool events carried `effort`, which the design doesn't use.
- With nothing listening on port 47231, this session's tool calls brought no hook error into its conversation, and the user saw no hook error in a session in the app (user, 2026-11-15).

**Changed:**

- A `/clear` ends the session's tile and starts a new one, since the `session_id` changes.
- After you deny a tool or interrupt Claude, the tile keeps its state until the session's next event, and how to tell sooner is left open.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#events).

## 2026-11-15: Desktop sessions fire the hooks, and an http hook would stall 2s per event while Plates is closed

**Asked:** do the hooks `plates hooks install` writes reach the receiver from sessions in the Claude desktop app, and what do they cost a session while Plates is closed?

**Expected:** events from every desktop session, and a connection to a closed port refused at once, so an `http` hook would cost a session nothing while Plates is closed.

**Found:** we installed the hooks into our user settings and ran `plates listen` (measured on Windows 11 25H2, 2026-11-15, timings in [results/2026-11-15-hook-timings.txt](results/2026-11-15-hook-timings.txt)).

- This repository's own desktop session sent `PreToolUse`, `PostToolUse`, and `PostToolUseFailure` from its next tool call after the install, without a restart, and another desktop session sent `UserPromptSubmit` with its prompt and title.
- A TCP connection to a port on `127.0.0.1` that nothing listens on was refused after 2,038ms to 2,053ms, in 3 tries.
  So while Plates is closed, each `http` hook would hold Claude Code for about 2s, two of them on every tool call, against the 100ms [the plan's step 3](../product/plan.md#3-claude-code-hookup) allows.
- Claude Code starts an async command hook and goes on without waiting for it, and gives it the event's JSON on stdin (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
  A command hook with an `args` field runs its program directly, with no shell (documented, same page).
- `plates-hook`, which forwards an event from stdin to the receiver, exited 10ms to 21ms after it started while `plates listen` was running, and 320ms to 331ms after while nothing listened, with exit code 0 both times.
- A desktop session's first prompt can start with an `<artifact-view-context>` block that the app adds before the typed text, and it named the session until titles skipped the tag blocks at the start of a prompt.

**Changed:** each hook is an async command hook that runs `plates-hook`, and a prompt's title skips the tag blocks at its start.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#installing).

## 2026-11-15: A desktop session's transcript marks a typed prompt with origin.kind human, after a system reminder

**Asked:** how does the transcript of a session in the Claude desktop app hold the prompts you type, so the transcript reader can name a session Plates first saw after its first prompt?

**Expected:** JSON Lines, with each typed prompt as a record whose `message.content` is the prompt's text.

**Found:** we read the structure of one of this repository's session transcripts, the record types and keys only, written by Claude Code 2.1.325 in the Claude desktop app (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-transcript-structure.txt](results/2026-11-15-transcript-structure.txt)).

- The transcript is JSON Lines, with records of `type` `queue-operation`, `user`, `attachment`, `last-prompt`, `file-history-snapshot`, and others.
- A prompt a person typed is a `user` record with `origin.kind` `human` and `isSidechain` false.
  Its `message.content` is either a string or a list of `text` parts, and the first prompt's list started with a `<system-reminder>` part of context before the typed text.
- A tool's result is also a `user` record, with a `tool_result` part and no `origin`, and a subagent's records carry `isSidechain` true.
- `UserPromptSubmit` carries the prompt's text as `prompt` (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15), but it also fires for a scheduled task, a subagent's report, and another session's message, so the first `prompt` Plates sees isn't always one you typed.

**Changed:** the transcript reader takes a session's first prompt from the first `user` record with `origin.kind` `human`, outside a subagent, leaving out `<system-reminder>` parts.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#titles-and-summaries).

## 2026-11-15: Claude Code documents the final message on Stop, failure events, and the session title on every prompt

**Asked:** does the hooks design handle every event that changes what a session is doing, and does it read each value from where the documentation puts it?

**Expected:** the seven events the design listed, covering every change of state.

**Found:** two more reads of Claude Code's documentation, for two reviews of the hooks design.
Nothing here has been measured yet.

- `Stop` carries `last_assistant_message`, the text of Claude's final response, and the page warns that the transcript file may not hold that message yet when `Stop` fires (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
- `Stop` doesn't fire when you interrupt Claude, and canceling a running tool fires no `PostToolUseFailure` (documented, same page).
  That no event at all ends an interrupted turn is our inference (unverified), and `PostToolUseFailure`'s `is_interrupt`, true for a failure that reached Claude Code as an abort, may mark some interrupts (documented, same page).
- When an API error ends a turn, `StopFailure` fires in place of `Stop`, with an `error` such as `rate_limit` and, optionally, the error's text in `last_assistant_message` (documented, same page).
- `PostToolUseFailure` fires after a tool that started fails.
  A permission you deny by hand fires no event of its own: `PostToolUseFailure` doesn't fire for a permission denial, and `PermissionDenied` fires only for auto mode's denials (documented, same page).
- `PermissionRequest` fires when Claude Code is about to ask you for permission to use a tool (documented, same page).
  In a session hosted through the Agent SDK's `canUseTool` callback, as the Claude desktop app hosts Claude Code, `Notification` with `permission_prompt` arrives about 6s after Claude asks, and not at all if you or a `PermissionRequest` hook answer sooner (documented, same page).
- `SessionStart` fires with a `source` of `startup` for a new session, and also of `resume`, `clear`, `compact`, or `fork` (documented, same page).
- `SessionEnd` fires when a session exits, and also on `/clear` and on switching sessions with `/resume`, with a `reason` of `clear` or `resume` (documented, same page).
- `UserPromptSubmit` fires for a prompt you send, and also when a scheduled task fires, when a background subagent reports back, and when another session sends a message to the conversation (documented, same page).
- `session_title` is present whenever the session has a custom title, whether from `--name`, `/rename`, a hook, or the Agent SDK's `renameSession()`, and absent for a title the app generates.
  `UserPromptSubmit` carries it too (documented, same page).
- An `http` hook whose connection fails, or whose response isn't a 2xx status, ends in a non-blocking error, and the action goes ahead (documented, same page).
- An `http` hook's default timeout is 600s, except 30s on `UserPromptSubmit`.
  A session's `SessionEnd` hooks share a budget of 1.5s, which Claude Code raises to the longest per-hook `timeout` in the settings files, up to 60s (documented, same page).
- `claude --desktop --resume <session-id>` opens a CLI session in the Claude desktop app by its ID, a session's name doesn't work in place of the ID, and it doesn't move a session that's open in another terminal or still running in the background (documented, [desktop](https://code.claude.com/docs/en/desktop.md), as of 2026-11-15).

**Changed:**

- The your-turn summary comes from `last_assistant_message`.
- `PostToolUseFailure` and `StopFailure` move tiles as `PostToolUse` and `Stop` do.
- `PermissionRequest` moves a tile to needs approval, without the 6s wait of `permission_prompt`.
- `SessionStart` brings back an ended tile and leaves any other tile it already shows as it is.
- An event from a session Plates doesn't show adds its tile.
- The receiver answers each request before acting on it.
- Step 6 tries `claude --desktop --resume` on a desktop session.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md) and [the plan](../product/plan.md).

## 2026-11-15: Claude Code documents hooks in desktop sessions and an HTTP hook type, but no link that opens one desktop session

**Asked:** do the hooks in Claude Code's user settings file fire for sessions in the Claude desktop app, what does each event carry, can a hook reach Plates without a helper program, and is there a link that opens one desktop session?

**Expected:** a small helper program for each hook to run, which would forward the event to Plates, and possibly no hooks at all in desktop sessions.

**Found:** an agent read Claude Code's documentation for us, and we read the Windows registry on our PC.
Apart from that look at the registry, nothing here has been measured yet.

- "Claude Code fires the same hook events wherever it runs: sessions in the terminal, IDE extensions, the Desktop app, and cloud sessions" (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
  Cloud sessions don't read the settings file on our machine (documented, same page).
- Every event carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, and `permission_mode` (documented, same page).
- `Notification` adds `message`, an optional `title`, and `notification_type`: `permission_prompt` once Claude has waited about 6s for an approval, and `idle_prompt` about 60s after Claude finishes, if nothing has been typed and no background agent runs (documented, same page).
  A `Notification` hook can't block.
- `SessionStart` carries `session_title` when the session has a custom title, such as one set with `--name` or `/rename`, and leaves it out for a title the app generates (documented, same page).
- A hook can be of type `http`, which POSTs the event's JSON to a URL, with a default timeout of 600s (documented, same page).
- The documented deep link, `claude-cli://open`, opens the terminal CLI and takes `q`, `cwd`, and `repo`, with no session ID (documented, [deep links](https://code.claude.com/docs/en/deep-links.md)).
  The desktop app's one documented `claude://` link, `claude://code/new`, opens its new-session page, optionally on an SSH connection, and takes no session ID (documented, [desktop](https://code.claude.com/docs/en/desktop.md)).
- On our Windows PC, the `claude` URL protocol is registered to the Claude desktop app's executable, under `HKCU\Software\Classes\claude` (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-claude-protocol.txt](results/2026-11-15-claude-protocol.txt)).

**Changed:** the receiver takes `http` hooks directly, with no helper program; a session tile's title falls back to the session's first prompt; and clicking a session tile brings the Claude app forward until a link to one session turns up.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md), [01-aims](../product/01-aims.md), and [02-tiles](../rack/02-tiles.md).
