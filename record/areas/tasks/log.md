# Tasks: log

What we found about keeping a task list for each session, for Plates to show in its tile, newest entry first.

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
