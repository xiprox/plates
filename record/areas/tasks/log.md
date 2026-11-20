# Tasks: log

What we found about keeping a task list for each session, for Plates to show in its tile, newest entry first.

## 2026-11-20: A running session didn't get its restarted server's new instructions

**Asked:** does a running session's Claude get the server's new instructions once the session's server restarts?

**Expected:** the new lines, since a server sends its instructions in its reply to `initialize`, and Claude Code starts a new server when the old one is gone.

**Found:** we restarted the server of the session in the [entry below](#2026-11-20-claude-updated-a-task-when-it-handed-the-task-back-to-the-user-but-not-when-the-users-answer-set-it-moving), after the server's instructions gained two lines (measured on macOS 26.5.2, Claude desktop app 5.11492.5 with Claude Code 2.1.325, 2026-11-20, [results](results/2026-11-20-missed-update.txt)).

- The session's Claude didn't get the new lines when its server restarted.
  We stopped the session's server at 16:32Z, Claude Code started a new one at 16:33:15Z when Claude next called `tasks_get`, and up to 16:34:54Z the transcript held no `mcp_instructions_delta` event.
- In that transcript, Claude Code wrote that event only when a server was added or removed, and after each compaction, when it sent every server's instructions again.

So the new lines reach a running session at its next compaction (unverified: compacting this session, then asking Claude to quote the two lines, would settle it), and a new session from its start ([the server entry](#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).

**Changed:** nothing in Plates.

**Affects:** [01-task-list](01-task-list.md#left-open), whose test of the new lines needs a session that has them: a new one, or one compacted since the change.

## 2026-11-20: Claude updated a task when it handed the task back to the user, but not when the user's answer set it moving

**Asked:** why did a session's tile keep showing a task as on the user for 18 minutes after the user answered?

**Expected:** Claude to set the task to doing once the user answered, since the server's instructions say "Update a task when its state changes or there's news on it, not at every step."

**Found:** we read the transcript of a Claude desktop app session on another of the user's projects (measured on macOS 26.5.2, Claude desktop app 5.11492.5 with Claude Code 2.1.325, 2026-11-20, [results](results/2026-11-20-missed-update.txt)).

- Claude called `task_update` as it ended two turns.
  At 15:21Z it set the task to wait on the user, with the status "Strategy proposed; your call".
  At 15:36Z it rewrote the status twice, ending on "New idea list, waiting on your picks", after the server refused the first status as 41 characters, one over the 40-character limit.
- The user's message at 15:42Z gave the picks.
  Claude's first tool calls were a command and the app's own plan, `mcp__ccd_turn__update_plan`, which the app tells Claude to send with a turn's first tool call for a request that edits files or runs more than one command (read in the system prompt the app gives Claude, Claude desktop app 5.11492.5 on macOS 26.5.2, as of 2026-11-20).
  It worked on the picks from then to at least 16:00:51Z, across three turns, with three more `update_plan` calls and no call to a Plates task tool.
- When the first of those turns ended at 15:47:44Z, after Claude had sent the user a file, the task still had the status "New idea list, waiting on your picks".
- The session's server had sent the instructions quoted above at 14:24Z, before any of these turns.

Our reading, unverified because a transcript shows what Claude did and not why: the instructions named no moment to update a task once the user's answer moves it, and at that moment the app has Claude send its own plan instead.
Nor did they have Claude bring its tasks up to date before ending a turn.

**Changed:** the server's instructions, in `instructions()` in `src/plates-tasks/src/main.rs`, gained three lines.
One has Claude update a task in the same message as the first tool call of its work on the task, once the user's answer or Claude's own start sets that work going.
Another has it update each task it worked on that the list shows wrong before it ends a turn.
The third has Claude, when an update is due before it has seen the list, call `tasks_get` in the message the update was due in and send the update in its next message, since `task_update` needs a task's ID from the list.
The fix is in the instructions because no Plates hook adds to the prompt the user sends: each is an async command hook, which Claude Code starts and doesn't wait for (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15, [hooks log](../hooks/log.md#2026-11-15-desktop-sessions-fire-the-hooks-and-an-http-hook-would-stall-2s-per-event-while-plates-is-closed)).
Whether Claude follows the new lines is unmeasured: a session with them, in which the user answers a task waiting on them, would settle it.
A running session doesn't get the new lines when its server restarts ([the restart entry](#2026-11-20-a-running-session-didnt-get-its-restarted-servers-new-instructions)).

**Affects:** [01-task-list](01-task-list.md#how-claude-keeps-a-list).

## 2026-11-20: A running task server holds about 1MB of memory

**Asked:** what does each session's `plates-tasks` server cost to keep running, beside the rack?

**Expected:** little, since the server only reads and writes one small JSON file and loads no GPUI.

**Found:** with three app sessions running, their three servers held 0.7MB, 1.4MB, and 1.4MB of resident memory, and the running rack, the `plates` program, held 90MB (measured with `ps -o rss` on macOS 26.5.2, 2026-11-20, [results](results/2026-11-20-server-memory.txt)).
Both were debug builds, as `cargo build` makes them.

**Changed:** nothing; it backs the server being a binary of its own.

**Affects:** [01-task-list](01-task-list.md#the-server).

## 2026-11-20: Each Claude app session starts its own MCP server, which knows the app's session, and Claude keeps a task list by its instructions

**Asked:** can each session's task list come from a Model Context Protocol (MCP) server that Claude Code starts for the session?
That takes three things: the server learns which of the Claude desktop app's sessions it serves, its instructions reach Claude, and Claude keeps a list by them.

**Expected:** Claude Code starts a stdio server for each session and passes it the session's environment.
That environment includes `CLAUDE_CODE_HOST_SESSION_ID`, the app's ID for the session, which stays the same when the app moves the session to a new Claude Code session ([hooks log](../hooks/log.md#2026-11-20-on-macos-a-rewind-splits-a-sessions-tile-for-about-6s-before-it-merges-back)).
We had seen the variable only in the commands a session runs.

**Found:** the server knows the app's session, survives a compaction, and follows a rewind (measured on macOS 26.5.2, Claude desktop app 5.11492.5 with Claude Code 2.1.325, 2026-11-20, [results](../../../experiments/areas/tasks/mcp-server/results/server-log.jsonl)).
The probe, [probe-server.py](../../../experiments/areas/tasks/mcp-server/probe-server.py), was registered for the repository's folder with `claude mcp add --scope local`, and the user ran two app sessions in that folder:

- Each session started its own server process, with `CLAUDE_CODE_HOST_SESSION_ID` and `CLAUDE_CODE_SESSION_ID` in its environment.
  For the second session, `CLAUDE_CODE_HOST_SESSION_ID` was the app's `sessionId` for it, `local_fb415057-61af-40ba-a31c-ad4367a426e1`, and its two `CLAUDE_CODE_SESSION_ID`s, before and after the rewind below, were the `priorCliSessionIds` and the `cliSessionId` in the app's file for it (the file read again on 2026-11-20).
- A rewind started a new server process with the new Claude Code session's ID and the same `CLAUDE_CODE_HOST_SESSION_ID`, so the list carried over.
  A `/compact` kept the same process.
- Claude Code sent `server/discover` before `initialize`, and went on after the probe answered that it didn't know the method.
  It asked for protocol version 2025-11-25.
- The server's instructions and tools reached Claude, with the tools deferred until Claude loaded them through ToolSearch: asked in the first session, Claude quoted the instructions' marker, "Probe marker: lantern-47", and named the five tools ([its replies](../../../experiments/areas/tasks/mcp-server/results/session-1-reply.txt)).
  On the first session's first turn, the server was still connecting when the prompt arrived, so Claude saw neither, and saw both once ToolSearch waited for the server.
- The app didn't ask the user to approve `tasks_get`, in a session whose permission mode we didn't record.
- App sessions also load the servers registered in `~/.claude.json` for every project: this repository's sessions had the tools of the two servers registered there.
- The first two server processes in the log came from `claude mcp get`, run in another session to check the registration, so they carry that session's IDs.

In the second session, the user sent [a script of prompts](../../../experiments/areas/tasks/mcp-server/results/session-2-prompts.txt), and Claude followed the instructions at each one:

- A request with several steps started no list.
- Agreeing on a plan with several pieces of work, one of them waiting on someone, started a list of 6 tasks, and Claude said "Started a Plates task list: 6 tasks."
- News that the wait was over marked that task done.
- After a `/compact`, a change of plan marked one task dropped and added another.
- "Start the list over" called `tasks_replace`, and "Don't keep a task list" called `tasks_decline`.

Two results the design has to answer:

- **A rewind doesn't roll the list back.**
  The user renamed a task, then rewound to before that prompt, and the list kept the new name.
  Claude, reading the list in the rewound conversation, reported the rename as a change made outside the session.
- **Claude copied the list into its memory.**
  At each change to the list, Claude also wrote the plan into its memory for the project, and recorded the script's made-up lines as the user's decisions.

The second session wasn't a clean test of when Claude starts a list, because its memory for the project already described the task list's design.

**Changed:** nothing in Plates.
A rewind leaves the list as it is, unless using it shows otherwise (user, 2026-11-20), since the list tracks work that a rewind doesn't undo.
After the run, we added a line to the probe's instructions telling Claude that the list is the only copy of the plan's tasks, and not to copy it into memory or other notes.
Still unmeasured: the server on Windows.

**Affects:** [01-task-list](01-task-list.md), which rests on this entry.
