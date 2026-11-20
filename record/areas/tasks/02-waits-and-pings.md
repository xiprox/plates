# Tasks: waits and pings

```text
Purpose:     Letting Claude end a turn as waiting on something outside the session, and ring you when
             something you requested a ping for happens.
Depends on:  01-task-list (01-task-list.md), for the server; 01-core (../core/01-core.md), for the
             session states and saving; 02-tiles (../rack/02-tiles.md), for the tile's states, its
             timer, and its tone; 01-claude-code-hooks (../hooks/01-claude-code-hooks.md), for the
             events that move a tile; 03-holding-pings (../rack/03-holding-pings.md), for when a
             tone waits.
Decides:     The two tools and what Claude is told about them, how Plates learns of a call, where a
             turn's end moves the tile, the waiting state and when it ends, and what a ping does to
             a tile.
```

You can ask a session to tell you when something outside it happens, such as "once the PR is up and checks pass, ping me on Plates so I can merge it" (user, 2026-11-20).
Claude watches for it itself with a background command, whose exit starts Claude's next turn ([Watching for the event](#watching-for-the-event)).
Two tools on the `plates-tasks` server, `wait` and `ping`, tell Plates what the session's tile shows meanwhile and when the thing happens.
Plates watches nothing outside the session, so it holds no account or token for GitHub or any CI service.

Without them, a turn that ends with a background command running shows as unread or your turn ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#background-tasks)), so the tile asks for you while nothing needs you.
A background command can't keep the tile working instead, since a dev server's tile would then work for good.
And a tile at your turn rings only when its timer runs out ([02-tiles](../rack/02-tiles.md#the-timer)), not when the thing you're waiting to hear about happens.

## The tools

| Tool | What it says |
| --- | --- |
| `wait(on)` | the turn ends waiting on `on`, such as "CI checks", and not on you |
| `ping(message)` | the turn ends with news you requested a ping for, such as "Checks passed; ready to merge #12" |

- `on` takes the length limit of a task's `waiting_on`, and `message` that of a task's `status`, with the same checks in `plates-core`, since each shows on one line of the tile ([01-task-list](01-task-list.md#how-claude-keeps-a-list)).
- The server keeps nothing for either: it checks the text and answers, and Plates learns of the call from the hooks ([How Plates learns of a call](#how-plates-learns-of-a-call)).
- Each applies to the turn that calls it, and the last call of each in a turn wins.
- `wait` and `ping` work with or without a task list, since a session can watch a pull request without one.

## What Claude is told

The server's instructions also give these two rules (user, 2026-11-20):

- Before Claude ends a turn that waits on a background command whose result it will act on, such as tests, a build, an install, or a watch on a pull request's checks, it calls `wait` with what the turn waits on, such as "CI checks", so the tile doesn't show the turn as needing you.
  If Claude's closing message says it's waiting on something, Claude calls `wait` before sending it, and a command left running for you, such as a dev server, gets no `wait`.
- When you've requested a ping for something, Claude calls `ping` in the turn it happens, with what happened, so the tile rings at once.

Claude pings only on your request, since a ping at the end of every turn would stop meaning anything.
The rule for `wait` says to call it before the closing message, and names tests, builds, and installs.
Under a rule that said only "when you end a turn", with a watch on a pull request's checks as its one example, a session ended a turn saying it was waiting on two background commands and didn't call `wait` ([tasks log](log.md#2026-11-20-a-session-ended-a-turn-waiting-on-two-background-commands-without-calling-wait)).
Plates doesn't infer a wait from a background command still running at `Stop`, since a dev server is one, and its tile would then show waiting for as long as the server runs instead of asking for you (user, 2026-11-20).

## How Plates learns of a call

Plates reads the calls from the hooks, as it reads the app's wrap-up ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events)).
`PreToolUse` for `mcp__plates__wait` or `mcp__plates__ping` keeps the call's text for the turn, and the turn's `Stop` decides the tile's next state, as it does for every turn.
Claude Code names an MCP server's tools `mcp__<server>__<tool>`, after the name the server is registered under, as the probe, registered as `plates-tasks`, served `mcp__plates-tasks__tasks_get` (measured on macOS 26.5.2, 2026-11-20, [results](../../../experiments/areas/tasks/mcp-server/results/session-1-reply.txt)).
The names hold because `plates hooks install` registers the server as `plates` ([01-task-list](01-task-list.md#installing)).

`PreToolUse` fires before the server answers, so Plates runs the same `plates-core` checks the server runs and keeps only a call that passes them.
A call that passes but fails to reach the server still counts.
A call you deny ends the turn as an interrupt, which drops it ([Where a turn's end moves the tile](#where-a-turns-end-moves-the-tile)).

## Where a turn's end moves the tile

At `Stop`, Plates moves the tile by the first of these that holds (user, 2026-11-15 and 2026-11-20):

1. If the turn called `ping`, to your turn, as [What a ping does](#what-a-ping-does) says.
2. If the turn's last wrap-up asked you something, to your turn.
3. If a background task that counts still runs, to working, where it stays ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#background-tasks)).
4. If the turn called `wait`, to waiting.
5. If the turn had a wrap-up, to unread, and otherwise to your turn, such as in a terminal.

Every move but item 3's starts the state over, even when the tile is already in it: your turn starts a new timer and brightens, unread brightens, and waiting counts from 0 again.
A turn that begins with no `UserPromptSubmit` finds the tile where the last turn left it, and its own end still asks for you.
`StopFailure`, an interrupt, and `SessionEnd` drop the turn's calls, and move the tile as they do for any turn.

## The waiting state

A waiting tile shows:

- its marker: the pause mark (‖) that a task waiting on someone else shows in a list ([01-task-list](01-task-list.md#the-open-list)), drawn in the muted color rather than the list's done color
- its summary line: "Waiting on " and `on`, then how long it has waited, such as "Waiting on CI checks · 4m", in the secondary text color
- no timer, and no tint, brighten, or tone, since a waiting tile doesn't need you ([01-core](../core/01-core.md#tiles)) (user, 2026-11-20)

Its hover card's state line is its summary line, such as "Waiting on CI checks · 4m", and its scrolling part shows Claude's last message, as an unread tile's does ([02-tiles](../rack/02-tiles.md#the-hover-card)).
The waiting time counts up, so a wait that no turn ends shows how long it has gone unanswered.

Waiting ends when the session's next turn begins:

- at `UserPromptSubmit`, which moves the tile to working
- at the turn's first `PreToolUse`, which moves the tile to working, in case the turn fires no `UserPromptSubmit` ([Left open](#left-open))
- at a `PreCompact` for a `/compact` you typed, which moves the tile to working, and at an MCP server's `Elicitation`, which moves it to needs approval
- at `Stop`, for a turn that fired none of these, which moves the tile as [Where a turn's end moves the tile](#where-a-turns-end-moves-the-tile) says
- at `SessionEnd`, which moves the tile to ended

Waiting is a session state of its own in `plates-core`, beside working and unread ([01-core](../core/01-core.md#tiles)), whose summary is what the session waits on.
`sessions.json` saves it as it saves the other states, so a wait survives Plates restarting, and an earlier Plates that loads a `sessions.json` holding one renames it `sessions.json.broken` and starts with no session tiles ([01-core](../core/01-core.md#saving)).

## What a ping does

A turn that called `ping` moves the tile to your turn (user, 2026-11-20):

- the summary line is the ping's `message`, in place of the first line of Claude's last message
- the tile starts its timer, brightens, and takes your turn's amber, as any tile does at your turn ([02-tiles](../rack/02-tiles.md#session-tiles))
- Plates rings the timer's tone at once, and again when the timer runs out, as at any tile at your turn

On Windows, the tone and the brighten wait while you're busy, as a timer's tone does, and [03-holding-pings](../rack/03-holding-pings.md#what-lets-the-pings-through) says when a held one still rings.

## Watching for the event

Claude watches for the event with a background command, such as `gh pr checks 12 --watch`, which exits once the pull request's checks finish (documented, [gh pr checks](https://cli.github.com/manual/gh_pr_checks), as of 2026-11-20).
In a Claude desktop app session, the command's exit starts a new turn, even after the turn that started it has ended (measured on macOS 26.5.2, 2026-11-20, [hooks log](../hooks/log.md#2026-11-20-on-macos-a-background-commands-exit-starts-a-new-turn-in-a-desktop-session), and on Windows 11 25H2, 2026-11-20, [tasks log](log.md#2026-11-20-a-session-ended-a-turn-waiting-on-two-background-commands-without-calling-wait)).
In that turn, Claude reads the command's result, and calls `ping` with it if you requested one.

The Claude desktop app also watches a pull request a session opened, and starts a new turn in the session for "CI failures, merge conflicts and review comments" (read in the system prompt the app gives Claude, Claude desktop app 5.11492.5 on macOS 26.5.2, as of 2026-11-20).
That list names no new turn for checks that pass, so Claude watches for those with a command.

## Known limits

- If the Claude desktop app quits without sending `SessionEnd`, the tile stays waiting, its time counting up (unverified: closing a session sent `SessionEnd` on Windows ([hooks log](../hooks/log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)), and quitting the app with a session waiting would settle it).
- If Claude ends a turn with a background command running and doesn't call `wait`, the tile shows unread or your turn, and if it doesn't call `ping`, the news arrives with no tone.
- A turn that ended while Plates was closed reaches the tile from the session's transcript, which Plates reads for the turn's last message and wrap-up but not for `wait` or `ping` (read in `src/plates-core/src/hooks.rs`, `catch_up`), so that turn moves the tile to unread or your turn.

## Left open

- Whether a task notification that arrives after its turn has ended fires `UserPromptSubmit`.
  On Windows a task notification fired `UserPromptSubmit` ([hooks log](../hooks/log.md#2026-11-15-in-a-desktop-session-a-denial-and-an-interrupt-fire-nothing-until-the-next-prompt-and-clear-starts-a-new-session)), but we haven't seen one that arrived after its turn had ended.
  Running `plates listen --log` while a background command exits after its turn would settle it.
- Whether a watching command that fails or is stopped starts a turn, since the one we measured completed.
  Ending a turn with a background command that fails a minute later, and reading the transcript, would settle it.
- Whether Claude calls `wait` and `ping` when it should.
  Asking a session in a pull request's branch to ping you once the checks pass, then watching its tile, would settle it.
- Whether the Claude desktop app starts a turn in a session when its pull request's checks pass.
  Opening a pull request from a session, ending the turn with nothing watching, and waiting for the checks to pass would settle it.
- Whether a background command's exit starts a new turn in a terminal session.
  Running `claude` in a terminal and ending a turn with a background command that exits a minute later would settle it.
