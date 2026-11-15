# Hooks: log

What we found about Claude Code's hooks and the Claude desktop app, newest entry first.

## 2026-11-15: Claude Code documents 33 hook events, and the app's status line shows 8 states

**Asked:** after a working tile kept showing Claude's last tool while Claude thought, and showed nothing of a `/compact`, which other states does Plates miss?

**Expected:** a few more events beside the 10 Plates installed.

**Found:** we listed every documented event and every state the Claude desktop app's status line draws (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15; read in the Claude desktop app 5.11492.5.0, ion-dist.asar; [results/2026-11-15-events-and-app-states.txt](results/2026-11-15-events-and-app-states.txt)).

- Claude Code documents 33 hook events, and 14 `notification_type` values for `Notification`.
- Of the 23 events Plates didn't install, two pairs change what a tile shows: `PreCompact` and `PostCompact` bracket a compaction, and `Elicitation` and `ElicitationResult` bracket an MCP server waiting for your answer.
- The app's status line has 8 states: "Stopping…", "Compacting session…", a retry label, "Thinking…", a waiting label, a tool's label, "Working…", and a count of background tasks.
  One waiting label is for a message being sent.
- Thinking, retrying, waiting, and stopping have no hook event, and background tasks reach a hook only as `Stop`'s and `SubagentStop`'s `background_tasks`, at a turn's end.
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
- With the Stage 18 session showing, running `Start-Process 'claude://code/continue?session=local_<id>'` in Windows PowerShell 5.1 with the Plates session's ID switched the app to the Plates session, and the same command with Stage 18's ID switched it back.
- The app's code also names `claude://code/continue?session=last` "Continue Last Claude Code Session", and `claude://code/needs-input` "Sessions Waiting for You", as actions for its desktop entry.
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
