# Tasks: the task list

```text
Purpose:     Each session's list of planned work, which Claude keeps and the session's tile shows.
Depends on:  02-tiles (../rack/02-tiles.md), for the tile the list sits in; 01-window
             (../rack/01-window.md), for clicks and focus; 01-claude-code-hooks
             (../hooks/01-claude-code-hooks.md), for the session IDs a tile holds.
Decides:     When Claude starts and changes a list and how it writes one, the server that keeps
             it, where it's saved, and how a tile shows it.
```

A Claude Code session, in the Claude desktop app or in a terminal, can keep a task list: the planned work that runs past the request at hand.
Claude keeps it through `plates-tasks`, a Model Context Protocol (MCP) server that Claude Code starts for the session.
The session's tile shows the list in a section under the tile's 60px top: a bar that counts the open tasks by who holds each one, and that opens the list below it.
The list is for getting back into a session's work at a glance when you switch to it: what waits on you, what Claude is doing, what waits on someone else, and what nobody has started (user, 2026-11-20).

The steps of a single request don't belong on the list (user, 2026-11-20), since the Claude desktop app shows those itself, as the plan under Claude's latest message (seen in the Claude desktop app 5.11492.5 on macOS 26.5.2, 2026-11-20).
That plan is a different list from this one, and so is the task list Claude Code keeps with its own tools, which [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events) mentions.

## A task

| Field | Holds |
| --- | --- |
| `id` | a number the server gives the task, unique within its list |
| `title` | the piece of work in a few words, at most 30 characters |
| `state` | `todo`, `doing`, `waiting`, `done`, or `dropped` |
| `waiting_on` | who or what a waiting task waits on, at most 20 characters: `you` for you, or a name |
| `status` | what's new about the task, at most 40 characters, which a new `status` replaces rather than adds to |
| `updated` | when the task last changed, which the server stamps |

The tile groups tasks by who holds each one:

| Group | Tasks |
| --- | --- |
| On you | `waiting`, with a `waiting_on` of "you", "user", or "the user", in any case |
| Doing | `doing` |
| Waiting on others | `waiting` on anyone or anything else, or on no one named |
| To do | `todo` |
| Done and dropped | `done` and `dropped`, folded away |

## How Claude keeps a list

Claude follows the instructions the server sends in its `initialize` reply, which Claude Code passes on to Claude (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
They give these rules, all chosen by the user on 2026-11-20:

- Claude starts a list without being asked once it and you agree on work that runs past the current request: at least two pieces of work, or one piece that's waiting on someone.
  The steps of the request it's working on don't count.
- When it starts a list, Claude says so in one line, such as "Started a Plates task list: 3 tasks."
- If you say not to keep a list, Claude calls `tasks_decline`.
  From then on the server refuses `task_add` and `tasks_replace` with an error Claude reads, while `task_update` still changes the tasks the list already has.
- A list holds big pieces of work, about 3 to 10.
- A title names the piece in a few words, such as "Windows probe".
- A status says only what's new about the piece: where it stands, what's next, or what it waits for, such as "Needs a session on the PC".
  It never repeats the title, and it stays empty when the title says enough.
- A task waiting on you has a `waiting_on` of "you", and one waiting on someone else names them there, such as "Dana", rather than in its status.
- Claude updates a task when its state changes or there's news on it, and not at every step, and it marks a task dropped rather than leaving it stale.
- Claude replaces the whole list only when you ask it to start over or to replace the list.
- Claude calls `tasks_get` before changing a list it hasn't seen in this conversation.
- Claude keeps no other copy of the list, such as in its memory, because the probe's Claude copied the list into its memory at every change (same entry).

The limits on a title and a status are what fits on one line of a tile at the rack's 304px width ([01-window](../rack/01-window.md)).
`plates-core` refuses a longer title, status, or `waiting_on`, and one with a control character, such as a line break or a tab, with an error that tells Claude how to rewrite it.
The instructions give the rule for starting a list without a worked example, which would cost words in every session's context, and an example goes in only if Claude misjudges the rule in use.

## The server

`plates-tasks` is a binary beside `plates-hook` that speaks MCP over stdin and stdout.
Claude Code starts one for each session, and keeps the same one through a `/compact` (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
Every session keeps one running, so it's a binary of its own rather than a mode of the `plates` program, which links GPUI (user, 2026-11-20).
In debug builds, each server held 0.7MB to 1.4MB of resident memory, and the running rack held 90MB (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-a-running-task-server-holds-about-1mb-of-memory)).
A `plates` started only as a server would open no window, and we haven't measured what it would hold.

| Tool | What it does |
| --- | --- |
| `tasks_get` | returns the session's list, and the list `tasks_replace` last replaced, with when |
| `task_add(title, state?, status?, waiting_on?)` | adds a task, as `todo` unless it gives a state, starting the list if there's none |
| `task_update(id, title?, state?, status?, waiting_on?)` | changes the fields given and keeps the rest |
| `tasks_replace(tasks)` | replaces the whole list, and an empty list wipes it |
| `tasks_decline` | records that you don't want a list in this session |

Since `tasks_get` returns the list `tasks_replace` last replaced, Claude can put it back with `tasks_replace` when you ask.
When `tasks_replace` replaces a list that's already empty, as after a wipe, the server keeps the list the wipe replaced, so a list wiped by mistake survives a new list put in its place.
A state outside the five, an `id` with no task, or text past its limit gets an error that Claude reads, and the list stays as it was.

### Finding the session

A server started for an app session finds `CLAUDE_CODE_HOST_SESSION_ID` in its environment, set to the Claude desktop app's `sessionId` for the session, the `local_<id>` that a session tile holds ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session)), and the server takes its session from it (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
A rewind moves the app's session to a new Claude Code session, and Claude Code starts a new server for it with the same `CLAUDE_CODE_HOST_SESSION_ID`, so the list carries on (same entry).
A rewind leaves the list as it is, because a rewind takes back the conversation and not the work the list tracks, such as a pull request you opened (user, 2026-11-20).

A session in a terminal has no app ID, so its server falls back on `CLAUDE_CODE_SESSION_ID` (unverified: the probe ran only app sessions).
If a rewind in a terminal starts a new Claude Code session, that session's list doesn't carry on, as [Left open](#left-open) says.

### Saving

The server saves each list as `tasks/<id>.json` in Plates' data folder ([02-tiles](../rack/02-tiles.md#saving)), where `<id>` is the ID it took its session from, such as `tasks/local_fb415057-61af-40ba-a31c-ad4367a426e1.json`.
It writes a new file beside the list and renames it over the list, so Plates never reads half a list (unverified on Windows while Plates holds the list open, as [Left open](#left-open) says).
The file holds the tasks, the next `id`, whether you declined a list, and the last list replaced, with when.

Plates reads the `tasks` folder once a second, and rereads only the files whose modified time or length changed.
It shows each list on the tile that holds the file's ID: the app's `sessionId`, or for a session in a terminal, its Claude Code session ID.
So a change Claude makes shows on the tile within about 1s.

Plates deletes a list's file when the session's tile goes for good, as [02-tiles](../rack/02-tiles.md#the-archive) defines it: an ended tile that's cleared, by its × or after 30 minutes, and a dismissed or snoozed session that ends.
Dismissing a session keeps its file, and so does forgetting it in the archive with Clear all or its row's ×, since the session comes back once it needs you.

### Installing

`plates hooks install` registers the server with Claude Code for every project, and `plates hooks uninstall` removes it (user, 2026-11-20).
Claude Code keeps the servers registered for every project in `~/.claude.json`, and the app's sessions load them (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
How the install writes that file is open, as [Left open](#left-open) says.

## On the tile

```text
┌─────────────────────────────────────────┐
│ ● V2 landing redesign          [timer]  │  the tile's 60px top
│   Footnote: A, B, or C?                 │
├─────────────────────────────────────────┤  divider
│ ● 2 on you  ‖ 2 waiting  ○ 2 to do   ⌃  │  the bar, 28px
├─────────────────────────────────────────┤  a second divider, while the list is open
│ On you                                  │
│ ● Category filters                      │
│   Your take on the 8 categories         │
│ ● Icon set  new                         │
│   Where the originals live decides      │
│ …                                       │  the list scrolls past 232px
│ 1 done · 1 dropped ⌄                    │
└─────────────────────────────────────────┘
```

### The bar

A session tile whose list has at least one task has a section under its 60px top: a 1px divider in 6% white, then a 28px bar.

- The bar counts the open tasks of each group that has any, in the groups' order, each after the group's mark: "2 on you", "1 doing", "2 waiting", "2 to do".
  The counts are 11.5px in the muted color, and "on you" takes the your-turn text color ([02-tiles](../rack/02-tiles.md#colors-and-type)).
- Once no task is open, the bar counts the done and the dropped ones instead.
- A chevron at the bar's right end points down while the list is closed and up while it's open.
  The counts sit in a box that cuts off at its edge, so the chevron always shows, and with all four groups and a two-digit count, the last count gives way.
  Four single-digit counts fit whole (seen on macOS 26.5.2 in a capture of `plates showcase --tasks` with a `doing` task added for the check, 2026-11-20).
- Hovering the bar lights it with 3.5% white, and hovering the tile's top lights the top alone, since a click there opens the session.

Clicking the bar opens the list below it, and since the bar stays where it is, a second click in the same place closes it (user, 2026-11-20).
The bar keeps its counts while the list is open, so a click changes only its chevron (user, 2026-11-20).
One list is open at a time, so opening another closes the first.

The bar follows the rack's rules for clicks and swipes:

- A click that ends a drag opens nothing, as on the rack's other buttons.
- A click on the bar treats keyboard focus as any other click on the rack does: on Windows, focus stays in the app you're in, and on macOS, it goes to the rack ([01-window](../rack/01-window.md#focus)).
- A two-finger swipe dismisses a tile only when it starts on the tile's top, so swiping across a list, or scrolling it, leaves the tile alone.

### The open list

The list is read-only, and you change it by telling Claude.
It opens under a second divider, with its tasks in the groups' order, each group under an 11px heading.
Within a group, tasks keep the order Claude added them in.
Each task's row holds its mark, its title at 12.5px, and under the title its status line at 11.5px in the secondary text color, each on one line, cut off at the tile's edge.
A task waiting on someone else ends its status line with who, as in "Asked on Tuesday · Dana".

| Group | Mark |
| --- | --- |
| On you | a dot in the your-turn color |
| Doing | a dot in the working color |
| Waiting on others | a pause mark in the done color |
| To do | a ring in the done color |
| Done | a check in the done color, with the title in the secondary text color |
| Dropped | a dash in the done color, with the title struck through in the muted color |

The colors are the tile states' own ([02-tiles](../rack/02-tiles.md#colors-and-type)).
A task that changed since you last opened the session says "new" after its title, in the working color.
The time you last opened it is the session's `lastFocusedAt` in the app's file for it ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events)), so a session in a terminal, which has no such file, shows no "new".

The done and dropped tasks fold into one row, such as "1 done · 1 dropped".
Clicking it unfolds them under it, and scrolls the list until the row sits at the list's top, or as far as the list scrolls.

Every row has a fixed height, so Plates sizes the rack's window from the same numbers it draws at:

- a heading, 22px
- a task, 22px, and 15px more with a status line
- the fold, 24px
- 4px above and below the list

An open list grows to at most 232px, about three and a half tiles, and scrolls past that.

### Opening and closing

The section grows and shrinks over 220ms, easing out, as a tile grows in ([02-tiles](../rack/02-tiles.md#layout)), and the rack's window changes height with it on every frame.
On macOS, Plates pins the window's drawing to its top-left corner, so a frame drawn for the height before shows unscaled rather than stretched ([rack log](../rack/log.md#2026-11-20-on-macos-a-frame-drawn-for-the-windows-old-height-can-show-stretched-to-its-new-one)).
A list that's closing keeps its scroll and its unfolded tasks until its section has shrunk to the bar.

When an open list would reach past the bottom of the screen's usable area, the window shows higher by as much, up to the height the list opened by, and never above the area's top.
The rack's place stays where it was, so the window comes back down as the list closes.
A drag moves the rack from where it shows, and then the rack is where you put it.

- Docking the rack closes an open list.
- A tile that leaves the rack keeps its section's height while it shrinks out.

## Known limits

- A session's first turn may run before Claude Code has connected to the server, so Claude sees neither the instructions nor the tools until it searches for them (measured on macOS 26.5.2, 2026-11-20, [tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
  A list starts only once you and Claude agree on a plan, which is rarely the first turn.
- A session that started before the server was registered doesn't load it until its Claude Code process starts again, as after a rewind or after you quit and reopen the Claude desktop app (unverified: the probe registered the server before its sessions started, and registering it while a session runs, then rewinding that session, would settle it).

## Left open

- Whether a server started for an app session on Windows finds `CLAUDE_CODE_HOST_SESSION_ID` in its environment.
  Registering the probe server, `experiments/areas/tasks/mcp-server/probe-server.py`, on Windows and reading its log would settle it.
- How `plates hooks install` registers the server: by writing `~/.claude.json`, which Claude Code may rewrite while it runs (unverified), or by running `claude mcp add`, which needs Claude Code's command on the PATH.
  Reading `~/.claude.json` before and after a session's turn would settle whether Claude Code rewrites it.
- Whether Claude Code asks you to approve the server's tools in each permission mode, and so whether the install needs to allow them.
  In the probe, the app didn't ask before `tasks_get` in the one session where we noted it, and we didn't record that session's permission mode.
  A session in each mode with the server registered would settle it.
- Whether Claude starts a list when it should in a project whose memory doesn't mention the task list, as the probe's second session's memory did.
  Sending [the second session's prompts](../../../experiments/areas/tasks/mcp-server/results/session-2-prompts.txt) in such a project would settle it.
- Whether Claude stops copying the list into its memory, since that instruction went in after the probe's run ([tasks log](log.md#2026-11-20-each-claude-app-session-starts-its-own-mcp-server-which-knows-the-apps-session-and-claude-keeps-a-task-list-by-its-instructions)).
  Changing a list a few times in a session, then reading the project's memory, would settle it.
- How you take back a decline, since no tool undoes it.
- What a server started for a session in a terminal finds in its environment, and whether a rewind in a terminal starts a new Claude Code session, which decides whether a terminal session's list carries on through one.
  Starting `claude` in a terminal with the probe registered, rewinding it, and reading the probe's log would settle both.
- Whether renaming a new file over a list succeeds on Windows while Plates reads the list.
  Renaming over a file another process holds open would settle it.
- Whether Windows stretches a frame drawn for the window's old height, as macOS did.
  A screen recording of a list opening and closing on Windows, read as [the macOS one](../rack/results/2026-11-20-resize-stretch.txt) was, would settle it.
