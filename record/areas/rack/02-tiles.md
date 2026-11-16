# Rack: tiles

```text
Purpose:     What each tile in the rack shows, and how you act on it.
Depends on:  01-window (01-window.md), and 01-claude-code-hooks (../hooks/01-claude-code-hooks.md) for where a session's state comes from.
Decides:     The tiles' layout and states, marking a turn read or unread, the hover card, the timer and snoozing, the archive, how a tile asks for attention, adding, renaming, and removing tiles, the hotkey, what Plates saves, and the tiles' colors and type.
```

A tile is one 60px row of the rack: a session tile for each Claude Code session, and your own tiles for anything else you want kept in view.
On every tile, the marker on the left is its status, and the timer slot on the right shows an hourglass, which fills with yellow while a timer runs, or a violet moon while it's set for tomorrow (user, 2026-11-15 and 2026-11-16).
[The mockups](https://claude.ai/artifact/A81TcK3BfzDVyQ2qaCqgNd) draw every state on this page.

## Layout

- Tiles never change places, so a change of state never moves a tile under your cursor, and only adding or clearing a tile moves the tiles below it.
- A tile that arrives grows from nothing to its full 60px over 220ms, and one that goes shrinks out over 180ms, fading in or out with its height (user, 2026-11-15).
  The 6px gap below a tile grows and shrinks with it, so the tiles below slide rather than jump, and the rack's window grows and shrinks with them ([rack log](log.md#2026-11-15-the-racks-window-can-change-height-on-every-frame)).
  The Undo button grows in and shrinks out the same way.
  A change of state, such as a new summary line or a new color, shows at once, so a fast change can still be read (user, 2026-11-15).
- Session tiles come first, then your own tiles, with no divider or gap between the groups (user, 2026-11-15).
- On Windows, a sub-session, a session that another session started in the Claude desktop app, sits under its parent's tile, indented 26px, with a line that comes down from under the parent's marker and turns into the tile (user, 2026-11-15).
  The app's sidebar draws a session's sub-sessions the same way (user's screenshot, 2026-11-15).
  A sub-session's tile keeps its right edge flush with the other tiles', so it's 26px narrower.
  A sub-session's own sub-sessions sit at the same indent, so the rack nests one level deep.
  [01-core](../core/01-core.md#tiles) says where a sub-session's tile goes.
  A sub-session's tile is indented only while its parent's tile is above it with nothing but indented tiles between them, so one that sits anywhere else, such as after you dismiss its parent, keeps its place without the indent.
- A session tile's marker is a circle, and your own tile's is a square.
- A title wraps to two lines when the tile has no summary line, and truncates after the second.
- Hovering a tile lays 3.5% white over its background, so a tinted tile keeps its tint.

## Session tiles

| State | Starts when | The tile shows |
| --- | --- | --- |
| New | the session starts | the tile grows in, and its background then brightens blue once |
| Working | Claude starts a turn, such as when you send a prompt | a blue dot that breathes on a 2.4s cycle, and a blue summary line of what Claude last wrote or is doing, such as "Reading plan.md" ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#titles-and-summaries)) |
| Your turn | Claude finishes a turn that asks you something, or you interrupt Claude or deny a tool | an amber dot, an amber summary line, and a timer that starts on its own; the summary line is the first line of Claude's last message, or "Interrupted", or "Denied: " and what you denied, such as "Denied: rm -rf target"; once you've read the turn, by opening the session in the Claude desktop app, clicking its tile, or marking it read, the dot becomes an amber ring, the summary line takes the secondary text color, and the timer stops, since the reply is still yours but the turn no longer needs your attention (user, 2026-11-15 and 2026-11-16) |
| Unread | Claude finishes a turn that asks you nothing, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events) tells, or you mark a done tile unread | a green dot, the first line of Claude's last message in the text color, as a new message would be, and a background that brightens green once, with no timer, since the turn wants reading but no response (user, 2026-11-15 and 2026-11-16) |
| Done | you open an unread session in the Claude desktop app, click its tile, or mark it read | a gray dot, and the first line of Claude's last message in the secondary text color, with no timer and no tint, since the session doesn't need you (user, 2026-11-15 and 2026-11-16) |
| Needs approval | Claude asks you to approve a tool, or asks you a question, which the hover card calls "Needs your input" | an orange dot, what Claude wants to do or asks as an orange summary line, such as "Wants to run: cargo test -p collector" or "Asks: Which design?", and a timer that starts on its own |
| Ended | the session ends | a dimmed ring for its marker, "Ended · clears in 27m" as its summary line in the muted color, and an × on hover that clears the tile (user, 2026-11-15) |

[01-claude-code-hooks](../hooks/01-claude-code-hooks.md#events) says which hook event starts each state.

- A turn that ends with background agents still running keeps its tile working, unless its wrap-up, the `mcp__ccd_turn__wrap_up` call the Claude desktop app has Claude make at a turn's end, asks you something (user, 2026-11-15).
  It stays working because an agent's report starts a new turn ([hooks log](../hooks/log.md#2026-11-15-stop-lists-the-tasks-still-running-and-a-background-agents-report-starts-a-new-turn)), and its summary line counts the agents, as in "Waiting on 2 background tasks".
  [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#background-tasks) says which tasks count.
- On Windows, clicking a session tile opens that session in the Claude desktop app, and clicking the tile of a session the app doesn't keep, such as one in a terminal, does nothing ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session)).
- Right-clicking the text of a tile at your turn, unread, or done opens a menu with one item, in a popup at the pointer like the timer's menu (user, 2026-11-16).
  On a turn you haven't read, the item is "Mark as read", which marks the turn read as opening the session would, and leaves the session closed.
  On a turn you've read, it's "Mark as unread": a done tile goes back to unread, a your-turn tile's amber ring goes back to an amber dot, and the tile brightens once, as a tile that needs you does, and starts no timer.
  Right-clicking the text of any other tile opens nothing, and right-clicking the timer slot opens the timer's menu on every tile.
- A summary line draws text between backticks in a monospace font on a background of 8% white, and text between double asterisks in bold, and leaves out the backticks and asterisks (user, 2026-11-15).
  It draws a markdown link, `[text](target)`, as its text with a 1px underline in the line's color at 45% opacity, and the link can't be clicked (user, 2026-11-15).
- Session tiles can't be renamed ([01-aims](../product/01-aims.md#non-goals)).
- An ended tile clears itself 30 minutes after the session ends, and its × clears it at once.
  A timer holds it past the 30 minutes while the timer is set for tomorrow, and once a timer has run out, until you click the tile (user, 2026-11-16).
  While a timer holds it, its summary line says only "Ended".
- Every other session tile shows an × on hover too, which dismisses the session: its tile shrinks out, with an Undo button for 5s, and the session is read and its timer stops (user, 2026-11-15).
  A dismissed session stays off the rack while it works, compacts, or is opened, and comes back once it needs you again, with an approval, a question, an unread turn, or an MCP server's request, or once you send it a new prompt (user, 2026-11-15).
  It comes back after the last session tile, or for a sub-session where [01-core](../core/01-core.md#tiles) says.
  A dismissed session that ends doesn't come back.

## The hover card

Resting the pointer on a session tile's text for 400ms opens its hover card: a popup window 360px wide that grows out of the tile over the rack, like the menus in [01-window](01-window.md#launching-and-quitting), and shows what the tile cuts short (user, 2026-11-15).

- Each move of the pointer starts the wait again, so a pointer crossing the rack opens no card.
  The × and the timer slot sit beside the tile's text, so a pointer on its way to them, or resting on them, opens none either.
- Clicking a tile, or its card, opens the session the card would show, so that tile's card doesn't open again until the pointer has left the tile (user, 2026-11-16).
- The card grows from the tile's own rectangle to its full size over 180ms, easing out, and its text shows once it's full size, without a fade (user, 2026-11-15).
  It grows right and down from the tile's top-left corner, left from the tile's right edge where the monitor's work area leaves no room on the right, and up from the tile's bottom edge where it leaves no room below.
- It can reach past the rack's edges, and it closes as soon as the pointer is off both the card and the tile.
- Clicking the card's title, state, scrolling part, or wrap-up's box outside its choices does what clicking the tile does, so the card has no Open button (user, 2026-11-16).

From the top, the card shows:

1. The tile's marker, its title on up to two lines, and the session's state in words with how long it has been in it, such as "Your turn · waiting 12m".
2. What the session's state calls for, as the table below gives it, in a part that scrolls once it's taller than 300px, so a long message can be read whole while the rest of the card stays in place (user, 2026-11-15).
3. For a tile at your turn, unread, done, or ended whose turn called the wrap-up, the wrap-up's box, which the bullets below describe.
4. For a sub-session whose parent's tile is on the rack, "Started from" and the parent's title in quotation marks.
5. The session's folder and its branch, such as "plates · main".
6. The timer's controls, except on an ended tile, in two rows as the timer menu's two parts are: after the hourglass, a chip for each length to remind you in, and after a gray moon, a chip for each length to snooze for and Tomorrow (user, 2026-11-16).
   A divider runs above items 4 and 5, and another above the timer's controls, since under one divider the folder line read as part of the timer (user, 2026-11-16).
   While a timer runs, they're the time left, such as "Reminder in 4m", and a Cancel chip.
   The menu's hours aren't chips too, since ten chips crowd the card (user, 2026-11-16).

| State | The scrolling part shows |
| --- | --- |
| Working | the turn's newest four tools in words, oldest first, in the muted color, each after a check, or a cross for one that failed, and then the summary line in the working text color, after three dots that light in turn every 400ms (user, 2026-11-16) |
| Working, waiting on background agents | Claude's last message, the agents it waits on, and then the summary line |
| Needs approval | the summary line, and below it the whole command, for a shell command |
| Your turn, unread, done, or ended | Claude's last message, or for a turn with neither a message nor a wrap-up, the summary line |

- The card shows at most 12,000 characters of Claude's last message, cut at a space, with "…" where it stops, and draws it as paragraphs: a heading loses its #s and is 12.5px semibold, and a fenced code block is in the monospace font.
- A list item's marker, a • for a `-`, `*`, or `+` item, or the item's number, such as "2.", sits in a gutter of its own, so the item's wrapped lines line up with its text, as the Claude desktop app draws a list (user, 2026-11-16).
  Each two spaces of indent nest an item 14px further in, up to 28px, and an item indented further sits at 28px.
- The card runs in the order the Claude desktop app draws a session, with what came first at the top, so it reads like the session it opens (user, 2026-11-15).
- The wrap-up's box holds what the app draws under Claude's last message, in its order: the recap in the secondary text color, then the question in the your-turn text color, then its choices, one for each of the wrap-up's `actions` (user, 2026-11-15; read in the wrap-up tool's own description, Claude desktop app 5.11492.5.0, which says the app draws its box under the message Claude writes after the call).
  It sits below the scrolling part, so the question stays in view however far the message scrolls (user, 2026-11-15).
- Clicking one of the choices answers the session with it: Plates opens the session, as clicking the tile does, and presses the app's own button for that choice once the app shows that button, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#answering-a-wrap-up) describes (user, 2026-11-15 and 2026-11-16).
  On Windows, the app shows the session, and its message box takes keyboard focus ([hooks log](../hooks/log.md#2026-11-15-ui-automation-can-press-a-sessions-wrap-up-button-and-send-a-message-from-its-message-box)); whether the app comes in front of another app's window is unverified, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session) says.
  On macOS, the press needs Plates turned on under Accessibility in System Settings, and a choice you click before then has macOS offer to open that setting, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#answering-a-wrap-up) says.
  If the button doesn't show, the session stays open for you to answer there.
- A session whose file in the app Plates can't read has no title Plates can match, so the card only names its choices.

## Your own tiles

- Your own tile has a gray square marker and a title, and shows no time except a timer you start ([01-aims](../product/01-aims.md#non-goals)).
- The hotkey for a new tile is Ctrl + Shift + Space on Windows and ⌃⇧Space on macOS, registered for the whole system, through `RegisterHotKey` on Windows and Carbon's `RegisterEventHotKey` on macOS, which needs no Accessibility permission (documented, [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey), [RegisterEventHotKey](https://developer.apple.com/documentation/carbon/1514524-registereventhotkey), as of 2026-11-16).
  If another app already holds the shortcut, Plates logs that and goes on without it.
- The last item in the rack is a phantom tile: a dashed outline with a faint + and the hotkey's name beside it.
  Clicking it, or pressing the hotkey, brings the rack forward, slides it out of the dock if it's docked, and turns the phantom tile into an inline name field.
  Enter adds the tile, and Esc cancels.
- After the text, the new tile's field shows two keys, which you can also click: a reminder, then Enter (user, 2026-11-16).
  The reminder reads "Tab" beside a gray hourglass until you press Tab, and each Tab steps it through 5, 10, 15, 20, and 30 minutes and 1 hour, the timer menu's "Remind me in" lengths, then back to none, while Shift+Tab steps back.
  Once set, it shows the length beside a yellow hourglass, such as "10m", and Enter adds the tile with that timer running.
  Clicking the reminder steps it as Tab does, and clicking Enter adds the tile as Enter does.
- A click anywhere on the rack outside the field closes it without adding or renaming the tile, as Esc does (user, 2026-11-16).
- Double-clicking your own tile turns its title into the same inline field, to rename it.
- An × appears on your own tile when you hover it, as on an ended session's tile (user, 2026-11-15).
  Clicking the × removes the tile to [the archive](#the-archive) and shows an Undo button for 5s (user, 2026-11-15 and 2026-11-16).

## Swiping a tile away

On a trackpad, swiping two fingers sideways over a tile does what its × does: it dismisses a session or removes your own tile, each with the same Undo, and clears an ended session's tile at once (user, 2026-11-16).
The phantom tile doesn't swipe, and no swipe starts while a name field is open.
We've read how macOS and Windows deliver the swipe but measured neither, as the last bullet says.

- Once your fingers have moved 8px, a swipe that moved more sideways than up or down slides the tile, and any other swipe leaves it alone until your fingers lift.
- The tile slides with your fingers, fading to 79% opacity at 96px and 30% at 320px, and the window's edges cut it off, since its row reaches out through the rack's padding and border (user, 2026-11-16).
- The Undo row a removed tile leaves swipes away the same way, which keeps the removal, as waiting out its 5s does (user, 2026-11-16).
- When you lift your fingers, the tile slides out over 180ms if it slid 96px or more either way, about a third of its width.
  It also slides out if it slid at least 20px and, in the 60ms before the lift, your fingers carried it on that way at 600px/s or faster.
  Otherwise it slides back into place over the same 180ms.
- On macOS, the scrolling that coasts on after your fingers lift moves nothing, and on Windows it moves the tile, as the last bullet says.
- A swipe doesn't move the pointer, so the hover card's 400ms wait would run out mid-swipe.
  Once a swipe goes sideways, Plates closes the tile's card, and it doesn't open again until the pointer leaves the tile, as after a click.
- On macOS, GPUI passes a trackpad's scroll on with AppKit's phases: `Started` as the scroll begins, `Ended` as your fingers lift, and the coasting that follows as `Moved` (read in gpui-pre-macos 0.3.12, `src/events.rs`, `platform_input_from_native`, the `ScrollWheel` arm).
  That AppKit sends `Ended` at the lift is unverified until a Mac run swipes a tile.
  On Windows, GPUI reads a precision touchpad's scrolls through DirectManipulation, and sends `Ended` only once the coast has stopped, with the coast as `Moved` (read in gpui-pre-windows 0.3.12, `src/direct_manipulation.rs`, `OnViewportStatusChanged` and `OnContentUpdated`).
  So on Windows the tile would follow the coast as if your fingers still moved, and how that feels is unverified until someone swipes a tile there.

## The timer

- Clicking the hourglass starts a 5-minute timer.
  Right-clicking it, or a force click on a Mac trackpad, pressing on past the click, opens a menu in a popup at the pointer that can reach past the rack's edges, one length to a row, in two parts (user, 2026-11-15 and 2026-11-16):
  - "Remind me in": 5, 10, 15, 20, and 30 minutes, and 1 hour, which start a timer and keep the tile on the rack, and picking one while a timer runs starts it over
  - "Snooze": for 2 hours, for 4 hours, and until tomorrow, which take the tile off the rack until then, as [Snoozing](#snoozing) says
- A force click opens the menu at the deeper press, and the click it ends in starts or cancels no timer (user, 2026-11-16).
- A grid of chips for the minutes, above a list of the rest, didn't sit well beside the list's rows, whether the chips were filled, outlined, or inset.
- A timer that starts, by your click or on its own, first rotates the hourglass a half turn over 500ms, as you'd turn one over to start it, and the yellow shows once it's upright (user, 2026-11-16).
- While a timer runs, yellow fills the hourglass from the bottom up: it starts over the bottom bar, so a timer that just started shows yellow at once, and the yellow rises steadily to the top bar as the time runs out (user, 2026-11-16).
  It fills the whole icon rather than the lower bulb as sand would, since the lower bulb alone is too small to read at 20px.
  Hovering a running hourglass dims the yellow, and clicking it cancels the timer (user, 2026-11-16).
  The hourglass shows no × on hover: the dimming already says a click cancels, and an × we tried there crowded the 20px icon.
  After a click on the hourglass starts a timer, the yellow stays bright until the pointer leaves the hourglass, since dimming at once would look as if the click had canceled the timer it started (user, 2026-11-16).
- A session tile starts a 5-minute timer on its own when it moves to your turn or needs approval, and the timer stops when the session moves to working or ends.
- Reading a turn at your turn stops its timer, and if the session is still at your turn 2 minutes after you last looked at it, a 3-minute reminder starts, for a session you opened and left for another before answering (user, 2026-11-16).
  Each look, a click on the tile or opening the session in the app, starts the 2 minutes over, and stops the reminder if it runs.
  A timer you set or canceled yourself during the turn is yours, and no look stops it or starts a reminder over it.
  Approvals and questions keep their timer when you look, so they need no reminder of their own.
- When the timer runs out, the tile is time-up, and that becomes its status:
  - your own tile's square turns yellow, and the tile takes a yellow tint
  - a session tile takes a tint of its own status color, amber for your turn and orange for an approval, and its dot keeps showing Claude's state
  - the timer slot's hourglass stays full of yellow, where a running timer's fill ends, so the tile's yellow reads as the timer having run out, and clicking it turns it over to start another 5 minutes, so hovering it doesn't dim it as it does a running timer (user, 2026-11-16), while clicking anywhere else on the tile clears time-up
- A timer runs on the wall clock, whether or not anyone is at the desk.
- The timer slot's icon, divider, and hover highlight are translucent white, so they take on a tile's tint without adding a color of their own (user, 2026-11-15).

### Snoozing

Snoozing a tile takes it off the rack until a time, for a session you can't act on for a while, or one of your own you'll deal with later (user, 2026-11-16):

- The timer's menu snoozes a tile for 2 hours, for 4 hours, or until tomorrow, and the hover card's Tomorrow chip snoozes it until tomorrow.
- Until tomorrow means until 4:00 the next morning, local time, so the tile is back whenever you sit down.
  Before 4:00, a night that went past midnight still counts as the day before, so it then means the coming 4:00.
  Plates takes the offset from UTC when you snooze, so on a night the clocks change, the tile comes back at 3:00 or 5:00 by the new time.
- A snoozed tile waits for its time whatever its session does, since you snoozed it as one you can't act on yet.
  Its session's events still keep it up to date, and a session that ends while snoozed goes for good.
- At its time, the tile comes back time-up, where a new tile of its kind would go: a session's after its parent's family or the last session tile, and your own after your others.
- While any tile is snoozed, a violet moon with their count sits at the right of the grip band, level with the grip, its badge 8px inside the rack's border at the top and at the side, and on the docked strip, a violet moon follows the pips.
  The count takes a small orange dot while a snoozed session needs you, which leaves the session snoozed.
- Clicking the moon lists the snoozed tiles, each with its marker as it stands now, its title, and when it comes back, on the 12-hour or 24-hour clock the OS shows you, such as "Returns 4:30 PM" or "Returns tomorrow 04:00" (user, 2026-11-16).
  The list opens 4px under the moon's badge, leaving the badge whole, and to its left, its right edge on the tiles' right edge, so it stays within the rack's width, and clicking the moon again closes it (user, 2026-11-16).
  Clicking one brings it back now, as it stands, with no timer.
- Violet is a snooze's color, apart from yellow's timer running now (user, 2026-11-16).
  A tile that Plates 0.1.0 set for tomorrow, without snoozing it, shows a violet moon in its timer slot, "Reminder tomorrow" in the hover card, and a violet outline on the docked strip.
  Hovering the moon dims it, and clicking it cancels the reminder.
- The moon's list leaves out the tiles you remove or dismiss with × or a swipe, which [the archive](#the-archive) lists.

## The archive

The archive holds your own tiles you remove and the sessions you dismiss, with × or a swipe, until you bring them back or clear it (user, 2026-11-16).
While it holds any, its badge sits in the grip band: an archive-box icon and the count, in the muted color, left of the moon, or in the moon's place while no tile is snoozed.

- An ended session's tile you clear goes for good, with no Undo.
- Your own tile waits there without its timer, and an Undo in its 5s puts it back with the timer it had.
- A dismissed session's events keep its marker current, and it leaves the archive when it comes back on its own, as [Session tiles](#session-tiles) says, or when it ends, which drops it for good.
- Clicking the badge opens a list of the tiles, newest first, where the moon's list opens, and the list scrolls past 8 rows.
  Each row shows the tile's marker as it stands now and how long ago you removed or dismissed it: "just now" under a minute, then "12m ago", "3h ago", "yesterday" from 24 to 48 hours, and "4d ago".
- Clicking a row brings the tile back to the rack as it stands now, in the place [Snoozing](#snoozing) gives a tile that comes back, and clicking the badge again closes the list.
- Clear all, in the list's heading, forgets them: your own tiles go for good, and the dismissed sessions leave the list but stay dismissed until they come back on their own.
- The archive keeps the newest 50 of your own tiles you removed.

## Saving

Plates saves your own tiles, each with its timer, and those in the archive, each with when you removed it, as JSON in its data folder, `%APPDATA%\Plates\` on Windows and `~/Library/Application Support/Plates/` on macOS, and loads them when it starts.
In the same folder, it saves where the rack last came to rest, as [01-window](01-window.md#launching-and-quitting) describes.
It also saves the session tiles, shown and dismissed, with when you dismissed each, so a session that finished before Plates restarted, and so sends no more events, keeps its tile (user, 2026-11-15).
Each keeps its state, summary line, and timer, and the last turn's message, wrap-up, and background tasks, which its card shows.
When it starts, Plates brings each restored tile up to date with any turn that ended or began while it was closed, from the session's transcript ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#titles-and-summaries)).

## Colors and type

| Use | Color |
| --- | --- |
| Tile | `#1C1D21` |
| Text, and secondary text | `#ECEDEF`, `#A3A6AE` |
| Working and new: dot, and working text | `#74AEFF`, `#9CC4FF` |
| Your turn: dot, and text | `#F2B441`, `#F2C46A` |
| Unread: dot, and the color it brightens to | `#5BCB8A`, `#1D3A2B` |
| Done: dot | `#5E626B` |
| Approval: dot, and text | `#FF7A45`, `#FF9B6E` |
| A running timer's yellow, and your own tile's time-up square | `#F4D35E` |
| A snooze: the moon and its count, and a reminder for tomorrow's moon, words, and outline | `#B39DFF` |
| Your own tile's marker | `#5E626B` |
| Time-up tints: your own tile, your turn, and approval | `#2B2715`, `#2F2616`, `#321F17` |

The type is Inter: titles at 13.5px semibold, summary lines and secondary text at 12px, and the hotkey's name on the phantom tile at 11px.
Code in a summary line is in a monospace font each OS ships with: Consolas on Windows and Menlo on macOS.
On the hover card, Claude's text is 12px with a line height 1.35 times its size, and code blocks are 11.5px.

## Left open

- Where you change the hotkey.
