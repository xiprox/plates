# Rack: holding pings while you type

```text
Purpose:     Keeping a timer's tone and a tile's call for attention from cutting into what you're typing.
Depends on:  02-tiles (02-tiles.md), for the tone and the brighten; 01-window (01-window.md), for the docked strip's nudge; 01-claude-code-hooks (../hooks/01-claude-code-hooks.md), for the events that say you've sent a prompt or answered Claude.
Decides:     What counts as typing, which pings wait while you type, what lets them through, and how long at most they wait.
```

On Windows, while you type in any app, Plates holds back its pings: the tone a timer rings as it runs out, a tile's brighten as it starts needing you or as you come back to the PC, and the docked strip's nudge.
They come together as soon as you stop typing for 30s, send Claude a prompt you typed, or answer Claude, and none waits longer than 5 minutes (user, 2026-11-17).
The tile doesn't wait: a timer that runs out while you type turns its tile time-up at once, with its tint, and only the brighten waits.

## What counts as typing

You count as typing from a typing key until 30s pass with no other, so a pause to think in the middle of a message doesn't let a ping through (user, 2026-11-17).
A typing key is a letter, a digit, punctuation, a number-pad key, Space, Backspace, Enter, Tab, or Delete, in any app.
Scrolling doesn't count, with the wheel or with the arrow, page, Home, and End keys.
Clicks, mouse movement, and Shift, Ctrl, Alt, or the Windows key pressed alone don't count either.
A key pressed while Ctrl, the left Alt key, or a Windows key is held doesn't count, since it's a shortcut, such as Ctrl+Tab.
Windows sends AltGr, which types characters such as @ on the German layout, as Ctrl with the right Alt key, so a key pressed with those two held still counts (unverified; typing @ with AltGr on the German layout while a timer runs out would settle it).
Space counts even when it pages down a web page, and how often browsing presses it is unmeasured.
`types` in `src/plates/src/native/win.rs` holds the same list of keys, so a change to the list changes both.

In the [rack log](log.md#2026-11-17-on-windows-raw-input-sees-typing-keys-only-while-you-type-in-any-app-and-clicks-while-you-browse-too)'s probe, typing keys came only while the user typed, and 1 to 3 clicks came in each 10s to 20s part of resting and scrolling, so clicks couldn't tell typing from browsing (measured on Windows 11 Pro build 26200, 2026-11-17).
In its second run, the only one that timed pauses, the longest pause in 20s of typing was 3.1s.
How long you pause to think while writing a real message is unmeasured, and timing the pauses in a working session's real typing would settle whether 30s outlasts them.

On Windows, Plates registers for raw keyboard input with `RIDEV_INPUTSINK`, which hands a window in the background the key presses of every app ([RAWINPUTDEVICE](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice)).
Plates keeps only the time of the last typing key, never which key it was.
On macOS, how Plates would see typing is left open, so no ping waits there ([Left open](#left-open)).

## What lets the pings through

Held pings come together on the first of these:

- 30s with no typing key.
- A prompt you typed to Claude, in any session: a `UserPromptSubmit` whose `prompt` holds text after the blocks of context the Claude desktop app puts before a prompt.
  A subagent's report or a task notification is nothing but such blocks ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#titles-and-summaries)), so it lets nothing through.
- An answer to Claude, one of these:
  - a question you answered: Claude Code sends the `AskUserQuestion` tool's `PostToolUse` as you answer ([hooks log](../hooks/log.md#2026-11-16-the-claude-desktop-app-asks-for-askuserquestion-through-permissionrequest)), and Plates counts a `PostToolUseFailure` for it too
  - an MCP server's form or page you answered: Claude Code sends `ElicitationResult`
  - an interrupt or a denial: Claude Code writes a marker to the transcript, which Plates reads once a second, and how soon after you interrupt the marker reaches the file is unverified ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events))
- A press on the rack, or a click in its hover card or menus that marks a tile unread or brings one back from the snoozed list or the archive.
- 5 minutes since the first ping started waiting, so a long stretch of writing can't hold a ping for good.

A prompt or an answer means you've finished what you were doing in Claude and are free again, so it ends the typing before it, and the pings come then rather than 30s later (user, 2026-11-17).
A press on the rack means you're at the rack, so what you just did there, such as an Undo, brightens at once even if you typed elsewhere a moment before.
A ping that starts afterward waits only if you press a typing key again.
A tool you approved doesn't count: Claude Code sends nothing after you approve a tool until its `PostToolUse`, once the tool finishes ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events)), so a long `cargo test` you approved would let the pings through when it ends, while you may be typing elsewhere.

Each ping comes once: the tone rings once however many timers ran out while the pings waited, every tile that started needing you while they waited brightens together, and the docked strip nudges once.
The tone rings only if it's on as the pings come and a timer that ran out while they waited still has its tile time-up, so a timer you dealt with meanwhile, such as by answering its session, rings nothing.
A ping that starts as the held ones come is given with them, not after them.

## Left open

- Whether typing in an app that runs as administrator reaches Plates' raw input is unverified.
  Windows' User Interface Privilege Isolation lets a program inject input only into programs with the same or fewer rights ([SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)), and whether it also keeps the program from reading their keys is unmeasured, so typing there may hold nothing.
  Typing into an app run as administrator while a timer runs out would settle it.
- Whether a number-pad digit reaches Plates as a number-pad key: raw input may report it with Num Lock on as Insert, End, and the like (unverified), and then it wouldn't count, and the number pad's decimal point would count as Delete.
  Logging the key codes of number-pad digits with Num Lock on and off would settle it.
- macOS: `CGEventSourceSecondsSinceLastEventType`, which Plates already reads for how long you've been away, takes an event type ([CGEventSource](https://developer.apple.com/documentation/coregraphics/cgeventsource/secondssincelasteventtype(_:eventtype:))), so asking it for key-downs would tell key presses from the wheel.
  It gives only when the last key-down was, not which key, so the arrow and page keys would count as typing on macOS.
  Whether it reports key-downs without the Input Monitoring permission is unverified, and the Mac session in [plan step 7](../product/plan.md#7-macos) would measure it.
  If it needs the permission, Plates would need a key-press watch that asks for it.
