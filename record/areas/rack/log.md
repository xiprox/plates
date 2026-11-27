# Rack: log

What we found about the rack's window and tiles, newest entry first.

## 2026-11-27: Italics and code inside bold were the commonest Markdown the tiles drew as raw marks

**Asked:** a tile showed "```text" as its summary line, for a message that opened with a code fence (seen by the user on macOS 26.5.2, 2026-11-27).
Which other Markdown forms do Claude's messages use that the tiles and the hover card don't read?

**Expected:** a few rare forms, since the summary line and the card already read code, bold, links, headings, lists, quotes, fences, and tables.

**Found:** over the 20,211 messages Claude wrote in every transcript on one Mac, each a text block, these forms drew wrong, most common first ([results](results/2026-11-27-markdown-forms.txt)):

- Code inside bold, as in `` **`Dialog`:** ``, on 2,074 lines: bold took its text as it was, so the backticks showed.
- Italics, `*text*`, on 976 lines, and `_text_` on 1 line.
- A list item the card nested at the wrong depth, 61 of 35,087 items, 55 of them among the 129 indented 4 or more spaces: the card nested an item a level for every 2 spaces, so one indented 4 spaces under a parent at the margin sat two levels in instead of one.
- A line that starts with an issue number, such as "#503 merged" (hypothetical), 61 times, which the tile and the card took for a heading, so it lost its # and the card drew it in semibold.
- A fence as a message's first line, 46 times, the form behind the tile the user saw.
- A task item, `- [ ] text`, 29 times, which drew its box as "[ ]".
- A quoted list item, `> 1. text`, 26 times, which drew as a paragraph.
- Bold and italics at once, `***text***` or `**bold *italic***`, 14 times.
- `<br>` in a table row, on 8 lines, and a backslash before a mark, as in `\*`, 5 times.
- An image, `![text](target)`, 3 times, and a code span opened by two backticks, 2 times.

A fence of four backticks came 3 times, each holding a line of three, and the card closed it at that line.
Claude wrote no strikethrough, `__bold__`, autolinks, footnotes, `~~~` fences, or fences and tables inside a quote.
Its 327 bare URLs draw as the text they are, and its 3 `$math$` spans keep their dollar signs.

**Changed:** `markup::parse` drops `*`, `_`, `**`, and `***` around text.
A mark opens only before a character that isn't a space and closes only after one, a `_` inside a word opens or closes nothing, and `__` stays as typed.
That is a subset of CommonMark's rules, and the run below counted the marks still showing, not marks dropped that should have shown.
`markup::parse` also ends a code span at a run of backticks as long as the one that opened it, turns `<br>` into a space, shows a mark after a backslash, and draws an image as a link.
`markup::blocks` needs a space after a heading's #s, reads the headings and list items inside a quote, draws a task item's box as ▢ or ✓, and nests an item under the nearest item indented less.
`Fence` closes at a line of only backticks, at least as many as opened it, and `first_line` steps into a fence and past a rule.

We ran the new code over the same messages: none panicked, and each of the 58 marks still showing is one that should, such as a footnote's asterisk, "SELECT *", or a file name that starts with `_`.
Italics lose their marks, but on macOS they draw upright: Plates hands GPUI only Inter's four upright faces, and GPUI takes the Inter family from those before the system's fonts and picks its nearest face (read in gpui-pre-apple 0.3.12, `src/text_system.rs`, `load_family` and `font_id`).
We haven't seen italics on Windows, so whether they slant there is unverified until a Windows run shows a tile with italics.

**Affects:** [02-tiles](02-tiles.md#session-tiles), its summary line, and [the hover card](02-tiles.md#the-hover-card).

## 2026-11-27: GPUI's tooltip can't leave the rack's window, so the pull request chip's pop-out is a window of its own

**Asked:** why did the pull request chip's pop-out sit against the rack's left edge, with room to spare on the right?

**Expected:** a tooltip beside the pointer, since GPUI places one at the pointer.

**Found:** GPUI places a tooltip 1px from the pointer, moves it to the pointer's left when it would pass the window's right edge, and pins it to the window's left edge when it fits on neither side (read in gpui-pre 0.3.12, `src/window.rs`, `prepaint_tooltip`).
The rack's window is 304px wide and the pop-out was 250px, so over a chip near the tile's right end it fit on neither side, and sat at the rack's left edge (seen by the user on macOS 26.5.2, 2026-11-27).

**Changed:** the pop-out is a popup window that grows out of the chip, as the hover card grows out of its tile.

**Affects:** [04-pull-requests](04-pull-requests.md#the-pop-out).

## 2026-11-27: A click on the pull request chip just before its pop-out opened left the pop-out over the browser

**Asked:** why did a pull request's pop-out stay on screen over the browser after a click on its chip?

**Expected:** the pop-out to stay closed, since the click opened the pull request and took the pointer's attention elsewhere.

**Found:** a click on the chip just before the pop-out's wait ended opened the browser, and the pop-out then opened over it and stayed (seen by the user on macOS 26.5.2, 2026-11-27).
The click didn't cancel the wait. The pop-out was then GPUI's tooltip, and GPUI closes a tooltip only on its own window's pointer events (read in gpui-pre 0.3.12, `src/elements/div.rs`, `register_tooltip_mouse_handlers`), which stopped reaching the rack once the browser was in front (unverified).

**Changed:** a click on the chip cancels the wait, closes any pop-out, and keeps the card and the pop-out closed until the pointer leaves the tile's text, as a click on the tile does.
The pop-out, a window of its own since the entry above, reads the pointer's place on screen each frame, as the hover card does.

**Affects:** [04-pull-requests](04-pull-requests.md#the-chip).

## 2026-11-27: `gh pr view` answers in under a second, and lists a pull request's checks one by one

**Asked:** how long does `gh pr view` take to say where a pull request stands, and how does it report the checks?

**Expected:** under a second, and the checks summed up in one state.

**Found:** `gh pr view https://github.com/xiprox/plates/pull/8 --json number,title,state,isDraft,reviewDecision,mergeable,mergeStateStatus,statusCheckRollup,url` took 820ms, and 960ms in a second run, whose output is in [results](results/2026-11-27-gh-pr-view.txt), with gh 2.91.0 (measured on macOS 26.5.2, 2026-11-27).
That pull request was merged and had no checks, so `statusCheckRollup` was `[]`.
Its `mergeable` was `UNKNOWN`.
For an open pull request in a private repository of ours, `statusCheckRollup` listed its 7 checks one by one, each with `__typename` `CheckRun`, `status` `COMPLETED`, and `conclusion` `SUCCESS`, beside its `name`, `workflowName`, `startedAt`, `completedAt`, and `detailsUrl`.
Its `mergeable` was `MERGEABLE`, and its `reviewDecision` was empty, which we take to mean the repository asks for no review (unverified).
We didn't save that pull request's output, since it names a private repository.

**Changed:** Plates asks `gh` from a thread of its own, one pull request at a time, and sums the checks up itself.

**Affects:** [04-pull-requests](04-pull-requests.md#asking-github).

## 2026-11-23: The Mac drew the README's three-rack picture at the sizes read from the code

**Asked:** does `plates showcase --capture` on the Mac draw the three-rack picture that [the Windows capture entry](#2026-11-20-on-windows-plates-showcase---capture-cant-draw-the-rack) laid out, at the sizes it read from the code?

**Expected:** a canvas of 1,514 by 946 logical pixels, the plain rack 537 logical pixels tall, and the rack with the task list open 770.

**Found:** the picture is 3,027 by 1,892px at the display's 2x scale, so 1,513.5 by 946 logical pixels, and its racks are 537 and 770 logical pixels tall, counted from their dark rows (measured on macOS 26.5.2, 2026-11-23).
The canvas is half a logical pixel narrower than read, and we haven't traced why.

**Changed:** `.github/readme/rack.png` is the three-rack picture, and README.md's description of it names the task list.

**Affects:** [the plan's step 7](../product/plan.md#7-macos), which no longer waits on the picture.

## 2026-11-24: On macOS, a menu stayed see-through, so menus open at full opacity there

**Asked:** why do the rack's tiles show through the grip band's menu on macOS?

**Expected:** nothing showing through, since the menu's panel is an opaque color, and the menu fades in by drawing at a rising opacity that reaches full at 120ms.

**Found:** two of the user's screenshots showed a tile's title, its summary line, and its hourglass through the menu's panel and its hovered row (seen on macOS 26.5.2, 2026-11-24).
In a showcase run that opened the menu by itself and logged each frame, the menu drew 15 frames, the last at 129ms at full opacity, and then no more.
In a second run it drew a frame about every 8ms for 1.6s, all at full opacity after 120ms.
We couldn't see which frame macOS put on screen, since `screencapture` needs a Screen Recording permission the Claude app doesn't have.
So we took the see-through menu for a window that stopped drawing partway through the fade.
With the fade gone on macOS, the menu was opaque (seen by the user on macOS 26.5.2, 2026-11-24), which fits that reading, though we still haven't seen the frames macOS showed.

**Changed:** on macOS, every menu opens at full opacity, with no fade.

**Affects:** [01-window](01-window.md#launching-and-quitting), which says each menu fades in.

## 2026-11-23: A GPUI div that scrolls sideways has to be a flex container, or its content never overflows it

**Asked:** why didn't a table in the hover card scroll under a sideways trackpad swipe, when its text ran past the card's edge?

**Expected:** the swipe's events not to reach the table, either because the card's vertical scroller took them or because the card's window got none.

**Found:** a temporary listener on the table's scroller printed all 273 of the swipe's scroll events, 68 of them more sideways than vertical, and the table still didn't move (measured on macOS 26.5.2, 2026-11-23, [results](results/2026-11-23-table-scroll.txt)).
GPUI lays out a div as a block unless it's told to be a flex container (read in gpui-pre 0.3.12, src/style.rs, `Style::default`).
As a block, the scroller stretched the table's row of columns to its own width, so the columns ran past the row instead of widening it.
GPUI clamps a scroller's offset to how far its children's bounds reach past it (read in gpui-pre 0.3.12, src/elements/div.rs, `Div::prepaint` and `Interactivity::prepaint`), and the row reached no further than the scroller, so there was nothing to scroll.
With `flex()` on the scroller, the row took its own width: 565px and 636px for two tables in a 322px scroller, and a sideways swipe scrolled the first.

**Changed:** the card's table scroller is a flex row, as gpui-component's horizontal scroll area is.

**Affects:** [02-tiles](02-tiles.md#the-hover-card), whose table rule now cites this measurement.

## 2026-11-20: On Windows, `plates showcase --capture` can't draw the rack

**Asked:** can we take the README's picture again on Windows, with a third rack that shows a session's task list open?

**Expected:** no, since GPUI's `render_to_image` returns "not implemented" on Windows when read in gpui-pre's source ([the capture entry](#2026-11-16-gpui-draws-the-rack-into-a-picture-offscreen-with-no-permission-to-record-the-screen)).

**Found:** a build with the `capture` feature printed `Couldn't save <file.png>: render_to_image not implemented for this platform` and saved nothing (measured on Windows 11 25H2, 2026-11-20).
So the README's picture can only be taken on the Mac.

**Changed:** the picture's layout and the capture that takes it, which the Mac runs next:

- `plates showcase --capture` gives the landing page session its made-up task list, closed, draws the rack with the list open, closes the list, and opens the session's card as before.
- `showcase::compose` lays the rack three times, plain, with the list open, and with the card, with their tops level, on a 16:10 canvas that grows to keep 88 logical pixels around the racks.
  With the showcase's 6 tiles and the list's closed bar, the rack is 537 logical pixels tall and the list's rack 770, so the canvas is 1,514 by 946 (read in plates at 385819c, `rack_view.rs`, `rack_height_for`, and `task_list.rs`'s row heights, unverified until the Mac takes the picture).
- `showcase::compose` rounds the canvas's corners to 16 logical pixels, see-through past them (user, 2026-11-20).
  The README's two-rack picture got the same corners from a one-off program that applies the same function to it.

**Affects:** the README's picture and its description in README.md, which wait for the Mac session in [the plan's step 7](../product/plan.md#7-macos).

## 2026-11-20: On macOS, a window placed at fractions of a point can change size, and the moving rack's frames came late

**Asked:** why do dragging and throwing the rack feel slow and laggy on macOS, when they're smooth on Windows?

**Expected:** the cause to be the debug build the Mac ran, whose GPUI layout and paint are unoptimized (unverified), or the refresh rate: the Windows display runs at 240Hz ([2026-11-15 entry](#2026-11-15-on-windows-gpuis-popup-window-moves-on-every-frame-at-240hz-once-a-subclass-removes-its-frame-and-keeps-clicks-from-activating-it)), and this Mac's displays at up to 120Hz and 100Hz.

**Found:** a temporary probe in Plates moved the window of the rack `plates showcase` opens around a 120pt circle on every frame for 4s, and logged each frame (measured on macOS 26.5.2, 2026-11-20, on the 120Hz built-in display of an M1 Pro MacBook Pro, [results](results/2026-11-20-drag-frames.txt)).
Separate Swift scripts moved a plain AppKit window through runs of 400 rects, the first computed as Plates' `to_ns` computes them, and read where macOS reports the pointer.

- Before the fix, the release build drew a frame in 0.80 to 0.99ms at the median against the debug build's 4.78 to 5.17ms, but its frames came late about as often: in each run of either build, 111 to 133 gaps between frames ran over 9ms and 37 to 47 over 17ms.
- With the window held still and the rack drawing every frame, the release build drew 455 and 456 frames in 3.8s, one per 8.3ms refresh, and no gap ran over 17ms, though 34 and 26 ran over 9ms.
- In the one release run before the fix that logged GPUI's events, 168 of the window's 439 moves called `setFrameSize` on GPUI's view.
  GPUI answered 163 of them by drawing the window at once from `displayLayer`, outside its display link, and restarting the link.
  Of that run's 440 frames, 163 came from `displayLayer` and 277 from the display link, and 45 gaps ran over 17ms, against none with the window held still.
- `to_ns` computes the window's width and height by subtracting one edge from another, so 232 of the first Swift script's 400 rects, all at fractions of a point, asked a size off by float error, from 303.99999999999994 to 304.00000000000006pt wide.
  AppKit puts a window on whole points, even on a 2x screen, and 128 of those 232 came out a point wider, taller, or both.
  In our runs, the size changed only when the origin sat at fractions of a point and the size was off: the 168 rects that asked exactly 304x400pt kept their size, and so did 400 moves at fractional origins with an exact size and 400 at whole-point origins with a size 1e-10pt short, though no run tried a size long at a whole-point origin.
- With the pointer at rest on the external 1x display, macOS reported it at 2116.54296875, 1079.44921875pt.
  A drag moves the rack by the pointer's movement since the press, so it puts the rack at fractions of a point when the pointer moves by fractions, which that reading suggests but doesn't show (unverified; logging `NSEvent::mouseLocation` through a drag on the built-in display would settle it).
  A throw's glide puts the rack at fractions of a point regardless.
  On Windows, `GetCursorPos` reports whole pixels and Plates rounds the rect before `SetWindowPos`.

**Changed:** on macOS, Plates rounds the window's rect to whole points before it moves the window.
In two release runs, 11 and 9 gaps then ran over 9ms and 3 and 0 over 17ms, and GPUI drew no frame outside its display link during the move.
A first version snapped to half points, the 2x screen's pixels, and measured the same in release, but in the Swift script AppKit moved 300 of 400 half-point origins to whole points and none of the whole-point ones, so Plates rounds to whole points.
The debug build, measured with half points only, had 15 and 26 gaps over 9ms and 6 and 8 over 17ms, with gaps up to 99.5ms, so a debug build isn't a fair test of how the rack feels.
The probe moved the window on its own, and a drag by hand is unmeasured.
The Windows display's 240Hz still moves the rack twice as often as this Mac's 120Hz, which no change to Plates can match.

**Affects:** nothing in the design docs, since [01-window](01-window.md#drag-and-throw) sets the drag and throw, not the points the window lands on.

## 2026-11-20: On macOS, a frame drawn for the window's old height can show stretched to its new one

**Asked:** why does a tile's task list jitter now and then as it opens and closes?

**Expected:** a smooth animation, since the rack's window only changes height, which moves nothing inside it, as when a tile grows in ([2026-11-15 entry](#2026-11-15-the-racks-window-can-change-height-on-every-frame), measured on Windows only).

**Found:** in a screen recording of four opens and four closes, 5 of 366 frames show the tiles stretched by about 3.5% (measured on macOS 26.5.2, 2026-11-20, [results](results/2026-11-20-resize-stretch.txt)).
In those frames the tiles below the open list sit 68 to 69px apart instead of 66px, while the window's bottom edge stays where it was the frame before.
All 5 came while a list closed.
While a list opened, one frame squeezed the gaps to 65px.

Plates resizes the window on every frame of the animation, through a task that runs after the frame is drawn.
GPUI sets no contents gravity on the layer it draws into, so when the layer's size changes, Core Animation stretches the drawing to fit, its default (read in gpui-pre-macos 0.3.12, `src/window.rs`, `MacWindow::open`, and gpui-pre-apple 0.3.12, `src/metal_renderer.rs`, `configure_layer`).
So we take a stretched frame to be one drawn for the window's height before a resize and shown after it (unverified).

**Changed:** on macOS, Plates sets the window's layers to keep their drawing at the top-left corner, a `contentsGravity` of `topLeft`, so such a frame shows unscaled.
With it, the jitter is gone by eye (user, 2026-11-20), and a recording of the same opens and closes, measured the same way, would confirm it frame by frame.
Whether Windows stretches such frames too is unmeasured.

**Affects:** [01-task-list](../tasks/01-task-list.md#opening-and-closing), which pins the window's drawing on macOS because of it.

## 2026-11-17: On Windows, raw input sees typing keys only while you type, in any app, and clicks while you browse too

**Asked:** can Plates tell that you're working at the PC, typing or clicking in any app, from idling at it, scrolling or watching, so a timer that runs out could hold its tone, brighten, and nudge until you're done?

**Expected:** `GetLastInputInfo`, which `native::idle_for` reads, gives only the time of the last input event, of any kind ([GetLastInputInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo)), so it can't tell typing from the wheel or mouse movement.
Windows' raw input names each event's kind, and with `RIDEV_INPUTSINK` a window in the background receives the input of every app ([RAWINPUTDEVICE](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawinputdevice)).

**Found:** a probe counted the user's input by kind through raw input, from a background window, while the user went through six parts, twice (measured on Windows 11 Pro build 26200, 2026-11-17, [first run](../../../experiments/areas/rack/input-activity/results/2026-11-17-windows.txt), [second run](../../../experiments/areas/rack/input-activity/results/2026-11-17-windows-1737.txt)):

- Typing in another app for 20s gave 70 typing keys, in 10 of the 20 seconds, and then 63, in 13 of them.
  In the second run, the only one that timed pauses, the longest pause between two typing keys was 3.1s, and one pause ran over 2s.
- Scrolling with the wheel for 20s gave 143 wheel events, then 216, and scrolling with the keyboard for 15s gave 111 navigation keys, then 21.
- No part but typing gave a typing key, in either run.
- Clicking for 15s gave 18 button presses, then 11, but clicks came in the other parts too: 1 to 3 while resting, scrolling with the wheel, and scrolling with the keyboard.
- Moving the mouse for 10s gave nothing but movement in either run: 274 events in the second, whose report kept that column.
  In the second run, every part with the mouse in hand gave movement too.
- Windows asked for no permission.

So typing keys alone mark you as working, and clicks don't, since the clicks of browsing look like the clicks of work.
A quiet spell of about 5s outlasts every pause measured here, but 20s of typing in a probe is short, and how long you pause to think while writing a real message is unmeasured.
In the first run, the report's table lost its columns for wheel seconds, mouse movement, and touchpad reports to PowerShell fitting it to the window, and the second run's report writes each part on a line of its own.
Neither run had a precision touchpad registered, so whether a touchpad's scrolling arrives as wheel events is unmeasured.

**Changed:** [03-holding-pings](03-holding-pings.md) holds Plates' tone, brighten, and nudge while you type on Windows, counting typing keys alone.

**Affects:** [03-holding-pings](03-holding-pings.md) and [plan step 9](../product/plan.md#9-holding-pings-while-youre-busy).
macOS needs its own measurement: `CGEventSourceSecondsSinceLastEventType` takes an event type, so it may tell key-downs from the wheel, and whether it does so without a permission is unverified.

## 2026-11-17: The timer's tone is a felt-mallet strike on a glass bar, and three measures tracked what the user heard

**Asked:** why did the timer's first tone, an 880 Hz sine 400ms long, sound "compressed" and "low bits" to the user, and "suffocate" at the end, and what would it need to sound like one of macOS' own alert sounds?

**Expected:** a softer start and a longer ring would fix it.

**Found:** measured on macOS 26.5.2, 2026-11-17, against nine of macOS' alert sounds ([results](results/2026-11-17-tone-against-macos-sounds.txt), and the spectrograms beside it).
The results file lists every variant below and how it differs from the shipped one.

- The expected fix didn't work for the user: a1-ring, the first tone with a 10ms eased rise, stretched to 1.4s with a slower fade, and a2 and a3, which added a room's echoes and then a slowly beating second voice to it, still felt "compressed" and "almost like low bits" (user, 2026-11-17).
  a1-ring's last 100ms sat at -49 dBFS, with the note visible at the cut in its spectrogram.
- No single measure set a1-ring apart from macOS' sounds.
  Crest, the peak over the average, puts a1-ring's 14.4 dB inside the nine sounds' 13.9 dB (Keys) to 28.8 dB (Note).
  The level of the last 250ms doesn't either: Glass's and Complete's sit at -97.5 and -86.3 dBFS, but Input's at -36.1 dBFS.
  Energy above 6 kHz in the first 5ms of the onset doesn't either: Glass, Note, and Chord carry it within 2 dB of their whole level or above it, and Complete, Tink, Input, and Keys 36 to 48 dB under.
  a1-ring carries it 52.6 dB under, 4 dB past Complete, and k3 48.1 dB under, beside Complete.
- What differed is how the sound is made.
  We built a struck bar from its modes, the tones a bar rings with at once: for glass, 1, 2.756, and 5.404 times the note, to which we added a weak octave at 2.
  Each mode fades faster the higher it rings, as Glass's, Note's, and Complete's overtones do in their spectrograms (read by eye).
  We rendered it at 48 kHz in 24-bit stereo, as Glass, Ping, Tink, Pop, and Purr are, where the first tone was 44.1 kHz, 16-bit without dither, and mono.
  Struck hard at A6 (g1), it measured within 0.1 dB of Complete's peak and average level, and the user found it "much much better".
  We can't say which of these changes mattered, the fade among them.
  The user then found a two-note figure (w1) demanding, and chose one strike at A5.

Three measures then tracked what the user said of each variant:

- Sharpness: the user found g1 "sharp", and across our variants the energy in 2.5-6 kHz, where the ear is most sensitive, tracked that: g1 carried it 12.2 dB under its whole level, and k3 35.9 dB under.
  macOS' sounds span 6.3 dB (Note) to 40.6 dB (Pop) under, so the measure ranks our variants, not theirs.
  A felt mallet an octave lower made about 17 of the 24 dB between g1 and k3: n1 carries it 28.9 dB under, and w1 30.5 dB under.
  Our variants changed the mallet and the octave together, so they don't say which mattered; rendering n1's felt at A6 would.
  At A6, the bar's modes at 2 and 2.756 times the note fall inside 2.5-6 kHz, and at A5 below it (unverified: arithmetic from the mode ratios, and g1's own modes aren't kept).
  Keeping the felt on the bar 4.5ms rather than 1.0ms took the energy above 6 kHz in the first 20ms from -59.3 to -83.4 dBFS, 4 dB of it from k3's lower level.
- Hang: the user heard the felt's ring "waving at you".
  Its level stopped falling at -52 dBFS from 1.1s to 1.5s (f3).
  Each mode rings as a pair of tones, mixed differently into the two channels for stereo width, as Glass has (stereo correlation 0.34, g1 0.42).
  The upper modes' pairs are 0.7, 0.9, and 1.4 Hz apart, and in f3 the note's pair, 0.45 Hz apart, drifted back into step.
  With the note's pair in tune, the ring falls at least 6.4 dB in every 300ms.
  s1 also shortened each mode's fade by 38%, so the note falls to 1/e in 248ms.
- Loudness: the user asked for a still softer tone.
  A softer felt, on the bar 4.5ms, and 4 dB less level took s1's -22.9 LUFS to k3's -26.8 LUFS, near Glass's -25.8 LUFS, and the user chose k3.

On macOS 26.5.2, `NSSound`'s `initWithData` returned a sound for Plates' 24-bit file and `play` returned true.
On Windows, Plates hands the 24-bit file to `PlaySoundW`, and plays a 16-bit copy if that returns false (read in plates, src/plates/src/tone.rs, `play`).
We haven't run either on Windows, and the [PlaySound documentation](https://learn.microsoft.com/previous-versions/dd743680(v=vs.85)) doesn't say whether, with `SND_ASYNC`, a format it can't play returns false, so the fallback is unverified.
A timer running out on Windows 11 would settle it: a tone with no `Couldn't play the tone.` on stderr means the 24-bit file played, the message means Plates fell back to the 16-bit copy, and silence with no message means `PlaySoundW` returned true for a file it couldn't play.

**Changed:** [tone.rs](../../../src/plates/src/tone.rs) builds k3 from the bar's modes, as the prototype in `experiments/areas/rack/tone/` renders it, and the difference between them, sample by sample, is -83.1 dBFS, 51.2 dB under the tone.

**Affects:** [02-tiles](02-tiles.md#the-timer), which describes the tone.

## 2026-11-16: On Windows, a border on a box with no width draws no line

**Asked:** why did a family's line still have a gap between two sub-sessions after each tile drew it on down, through the gap below it, to the next?

**Expected:** an absolutely placed box with a left border and a height, but no width, would draw a 1px line down its left edge.

**Found:** a capture of the rack showed each sub-session's └ but no line running from the first down to the second (measured on Windows 11 25H2, 2026-11-16, [results/2026-11-16-zero-width-border.png](results/2026-11-16-zero-width-border.png)).
The same line drawn as a box 1px wide, filled with the line's color, showed and met the next sub-session's line ([results/2026-11-16-filled-line.png](results/2026-11-16-filled-line.png)).
Each picture is cropped from a copy of the screen over the rack window's rectangle, taken through .NET's `Graphics.CopyFromScreen`, and enlarged to 4 times its size.
We haven't read GPUI's source to find why it draws no border on a box with no width.

**Changed:** a line that runs on past a tile, through the 6px gap below it, is a filled 1px box, and so is the short line that branches from it into the tile.

**Affects:** nothing in [02-tiles](02-tiles.md), which says what the lines show and not how they're drawn.

## 2026-11-16: On a Mac, hovering the docked strip made it flicker between peeking and not

**Asked:** what is wrong with the docked strip on macOS?

**Expected:** hovering the strip peeks it 6px out of the edge and opens the list of tiles that need you on its inner side, as [01-window](01-window.md#docking) describes.

**Found:** in the user's screen recording, the strip, hovered and held still, jumped about 200px inward with the list drawn as a thin line beside it, then back to the edge, several times a second (seen on macOS 26.5.2, 2026-11-16).
The recording isn't kept, since it shows the user's sessions.
Drawing the docked window with the peek on, from `plates showcase` with a temporary change that docked the rack and turned the peek on, showed the list on the inner side and the strip at the edge, 216px wide in all, so the layout was right.
The peek widens the window inward, which moves the window under a pointer that stays still, and GPUI on macOS updates where the pointer is in a window only when the mouse moves, so the strip's hover ended without the mouse moving, the peek closed, and the strip came back under the pointer (unverified, since we haven't read where GPUI on macOS updates the pointer's position).

**Changed:** the whole docked window, the list included, holds the hover, and the peek closes only once the OS's pointer position is outside the window, with the rack's once-a-second tick closing a peek that GPUI left open.
Whether the flicker is gone is unverified until the user hovers the strip again.

**Affects:** nothing in [01-window](01-window.md), which says what the peek does.

## 2026-11-16: GPUI draws the rack into a picture offscreen, with no permission to record the screen

**Asked:** how can we take a picture of Plates for the README that shows made-up tiles and nothing of the desktop behind them?

**Expected:** `screencapture -l` on the rack's window, as choom-collector's `tools/capture-window.ps1` uses `PrintWindow` on Windows.

**Found:** a shell started from Claude Code has no Screen Recording permission on macOS, as `CGPreflightScreenCaptureAccess` returned false (measured on macOS 26.5.2, 2026-11-16), so `screencapture` would picture only the desktop.
GPUI's `Window::render_to_image`, which comes with its `test-support` feature, draws a window's last frame again into a texture of its own and returns it as RGBA, with the corners outside the rack's rounded panel left clear (read in gpui-pre-apple 0.3.12, `src/metal_renderer.rs`, `render_offscreen`).
GPUI blends each color by its alpha onto a texture that starts transparent, so the picture's color comes already multiplied by its alpha (read in the same file, the pipeline's blend factors).
`plates showcase --capture`, built with the `capture` feature that turns that support on, drew the rack's window and its hover card's window this way at the primary display's 2x scale, with no permission asked (measured on macOS 26.5.2, 2026-11-16).

**Changed:** `plates showcase` opens the rack on the made-up tiles of `src/plates/src/showcase.rs`, writing none of Plates' data files and leaving the tray, the menu bar, and the shortcut to a Plates that runs, and its working dots hold still.
With `--capture <file.png>`, it waits 1s for the tiles to settle, opens the landing page session's card, which stays open without the pointer on it, waits 1s more, and draws both windows.
`showcase::compose` lays the rack twice on a 1,200 by 750 canvas, a warm stone gray that darkens a little downward, and the card over the second rack where its window sat beside the rack's on screen, with a soft shadow under each, and saves it at 2x, 2,400 by 1,500 (user, 2026-11-16).
To take the README's picture again, run `cargo run -p plates --features capture -- showcase --capture .github/readme/rack.png`.
On Windows, the `capture` feature turns on only the core's half of GPUI's test support, so `render_to_image` there returns GPUI's "not implemented" error (read in gpui-pre 0.3.12, `src/platform.rs`), which is unverified on Windows.

**Affects:** nothing in the design docs.

## 2026-11-16: On a Mac, a tile's click hands the keyboard to the Claude app once Plates makes itself active and yields to it

**Asked:** after clicking a session tile, why does typing go to the rack rather than the Claude app showing the session?

**Expected:** the link's app taking the keyboard as macOS opens the link, since opening a link usually brings its app to the front.

**Found:** the rack is a GPUI `PopUp` window, which GPUI builds as an `NSPanel` with the `NonactivatingPanel` style (read in gpui-pre-macos 0.3.12, `src/window.rs`, `MacWindow::open`), so a click makes it the key window without making Plates the active app.
A link opened through the `open` command, and then through `NSWorkspace` with `activates` set, showed the session in the Claude app while the rack kept the keyboard (seen by the user on macOS 26.5.2, 2026-11-16).
AppKit's header says an app activated at another's request should have that app yield to it first, which only the active app can do.
With Plates making itself active through `activateIgnoringOtherApps`, waiting for `isActive`, yielding to the Claude app, opening the link, and calling `activateFromApplication:options:`, Plates was active before the link opened, macOS accepted the request, the Claude app was frontmost within 100ms, and the user's typing went to it (measured on macOS 26.5.2, Claude desktop app 5.11492.5, 2026-11-16, [results](results/2026-11-16-focus-handoff.txt)).

**Changed:** a tile's click on macOS hands the keyboard to the Claude app this way, and before macOS 14, which brought yielding, Plates asks for the app to be activated without yielding, which is unverified.
A click elsewhere on the rack still leaves the keyboard with it.

**Affects:** [01-window](01-window.md#focus) and [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session).

## 2026-11-16: On a Mac, the shortcut's first press opened the field and closed it at once

**Asked:** why did ⌃⇧Space, pressed while another app was in front, bring the rack forward and open the inline name field, only for the field to close before you could type, and work on a second press?

**Expected:** something taking the rack's activation away right after the hotkey gave it.

**Found:** the user saw the field close at once on the first press and stay open on the second (seen on macOS 26.5.2, 2026-11-16).
The rack closed the field whenever GPUI reported a change of the window's activation while the window wasn't active, so the window reported such a change after the field opened and before Plates became active.
Which report that was is unverified, and logging each activation change through a first press would settle it.

**Changed:** losing activation closes the field only once the rack's window has been active since the field opened.
The user then pressed the shortcut with another app in front, and the field stayed open on the first press (seen on macOS 26.5.2, 2026-11-16).

**Affects:** nothing in [02-tiles](02-tiles.md), which says what the hotkey does and not how the field closes.

## 2026-11-16: On a Mac, the rack shrinks to half size on a 1x screen beside a 2x primary

**Asked:** why did the rack squeeze its tiles on the user's 1x external monitor, and why did a rack docked there come back too small to find?

**Expected:** a mix-up between the two screens' scales, since mac.rs said a second screen with another scale wasn't handled.

**Found:** the rack sized its window by GPUI's `window.scale_factor()`, the scale of the screen the window is on, while mac.rs turns the rack's pixels into points by the primary screen's scale on every screen (measured on macOS 26.5.2, 2026-11-16, with a 2x built-in display as primary and a 1x 3440×1440 monitor beside it).

- On the 1x monitor, the rack asked for 304px, which mac.rs made 152 points, while GPUI still laid the rack out at 304 points, so its tiles squeezed and clipped.
- A rack docked at that monitor's left edge came back from `place.json` as an 11×15-point window where the two screens meet.
- The hover card sized itself the same way, and the drag threshold and throw speeds use the same scale, so those were off by 2x on that monitor too (read in this repository, not measured).

**Changed:** `native::scale` gives the rack's pixels per GPUI pixel: the primary screen's scale on macOS, and the window's scale factor on Windows, as before.
The rack and the hover card take their scale from it.
With the change, a saved place on the 1x monitor opens a 304×168-point rack, and a dock at its left edge opens a 22×30-point strip, the same sizes as on the primary screen (measured on macOS 26.5.2, 2026-11-16).

**Affects:** nothing in the design docs, which give sizes in GPUI's pixels.

## 2026-11-16: On a Mac without Inter installed, the rack draws its text in a monospace font

**Asked:** why is the rack's text monospace on macOS when it's Inter on Windows?

**Expected:** Inter missing on the Mac, since Plates named the family without carrying the font.

**Found:** macOS 26.5.2 has no Inter, and GPUI's fallback for a family it can't find starts with `.ZedMono` (measured 2026-11-16; read in gpui-pre 0.3.12, src/text_system.rs, `TextSystem::new`).
That the Windows machine drew Inter because Inter was installed there is unverified, since no Windows ships with it, and the machine's list of installed fonts would settle it.

**Changed:** Plates carries Inter 4.1's Regular, Medium, SemiBold, and Bold faces in `src/plates/fonts/`, with Inter's license, and hands them to GPUI before the rack opens.

**Affects:** nothing in the design docs. [02-tiles](02-tiles.md) already sets the type in Inter, and now that holds on every machine.

## 2026-11-15: ShellExecuteW on the rack's thread lets GPUI run a task mid-click, and Plates crashes

**Asked:** why did Plates vanish when the user clicked a session tile, and at 20:06 that day?

**Expected:** a panic in Plates' own code, now that panic.log records one.

**Found:** panic.log held `RefCell already borrowed: attempted mutable App borrow`, from a GPUI task run inside `ShellExecuteW`, which the click's handler had called through `plates::native::open_url` (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-open-url-panic.txt](results/2026-11-15-open-url-panic.txt)).

- `ShellExecuteW` runs a message loop of its own, and on the rack's thread, that loop handed GPUI a queued task while the click's dispatch still held GPUI's app state.
- The panic happened inside a window procedure, which can't unwind, so Windows ended the process with exception code `0xc0000409`, the code the 20:06 crash left.
- It needs a GPUI task queued when the click comes, so a click doesn't always crash: a synthetic click on a tile earlier that day didn't.

**Changed:** Plates opens a link with `ShellExecuteW` on a thread of its own, which owns no windows, after the click grants the foreground with `AllowSetForegroundWindow`.

**Affects:** nothing in the design docs, which describe what a click opens and not which thread opens it.

## 2026-11-15: Another program's topmost window covers the rack until the rack raises itself

**Asked:** why does the rack go behind a game in borderless full screen, and can it stay above?

**Expected:** that the game is a topmost window too, and that Windows puts the newer one on top.

**Found:** a stand-in for the game, a borderless, topmost WinForms window from another process shown over the rack, sat above it in z-order, as `EnumWindows` reported it (measured on Windows 11 25H2, 2026-11-15).

- Before the fix, the stand-in stayed above the rack for the whole 3s it showed.
- With the rack checking once a second whether another program's topmost window covers it, and raising itself with `SetWindowPos` and `HWND_TOPMOST` if one does, the stand-in was still above at 0.7s and below the rack at 2s and 3s.
- The stand-in never became the foreground window, so the check that runs when another program comes to the front didn't run, and how fast it raises the rack over a real game is unverified.
- We haven't tried a real game, or one in exclusive full screen.

**Changed:** the rack keeps itself above other programs' topmost windows.

**Affects:** [01-window](01-window.md#shape).

## 2026-11-15: The rack's window can change height on every frame

**Asked:** can the rack's window follow a tile that grows in, by changing its height with `SetWindowPos` on each frame, or does a resize each frame stutter?

**Expected:** a stutter, since GPUI rebuilds its drawing surface when the window's size changes (unverified).

**Found:** a probe sent a session's first event and then polled the window's height with `GetWindowRect`, which it could do only about every 15.6ms, the resolution of Windows' default timer (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-row-growth.txt](results/2026-11-15-row-growth.txt)).

- The height went from 234px to 300px, one 60px tile and the 6px gap below it, in 12 steps over 186ms, easing out as Plates asks.
  The 186ms is shorter than the 220ms Plates asks for because the ease-out's last 40ms or so move the tile by less than half a pixel (unverified).
- The height first changed 293ms after the probe sent the event.
  We take the delay to be Plates searching all 332 of the app's session files for an ID none of them holds, since the probe's session is one the app doesn't keep (unverified; reading all 332 took about 205ms in [the hooks log](../hooks/log.md#2026-11-15-the-claude-desktop-app-keeps-a-file-per-session-with-its-title-and-its-claude-code-session-ids)).
- We judged smoothness only from the steps, and haven't watched for a stutter by eye or on a monitor above 60Hz.

**Changed:** a tile grows in and shrinks out, and the window's height follows it each frame.

**Affects:** [02-tiles](02-tiles.md#layout).

## 2026-11-15: DWM draws the rack's drop shadow once the window's frame reaches 1px into its client area

**Asked:** can the rack have the mockups' drop shadow without a transparent margin around it, which would take clicks meant for the windows below?

**Expected:** a shadow window of Plates' own, since the rack's window, without its frame, gets no shadow from DWM.

**Found:** we ran the self-test of the redrawn window test three times on the same PC (measured on Windows 11 25H2, 2026-11-15, the last three self-test reports in `experiments/areas/rack/results/`).

- With `DwmExtendFrameIntoClientArea` margins of 1px on each side, and DWM's border color set to none, DWM drew a shadow below the rack.
  In the screen captures, the screen 2px below the rack measured an average brightness of 9.5 with the shadow and 16.3 without it, and the shadow faded out by 28px.
  The captures stay beside the build, out of the repository.
- DWM rounds the shadow's corners at 8px, and the rack draws its own 14px corners inside that.
  Against the dark desktop in the captures, the corners looked the same with and without the shadow, and over a light window they're unverified.
- The slide out of the dock drew about 680 frames a second against the display's 240, while the window's center was off its display.
  The slide is timed, so it kept its speed and only made extra moves.
  Which GPUI path redraws on those moves is unverified.

**Changed:** the window test asks DWM for the shadow, and squares the docked strip's corners, which sit flush with the screen edge.

**Affects:** [01-window](01-window.md#shape).

## 2026-11-15: By hand, the prototype's throw, bounce, and dock numbers got in the way

**Asked:** tried by hand on Windows, does the rack leave keyboard focus where it is, keep Windows Snap away, and feel the way the mockups' prototype does?

**Expected:** the prototype's numbers to carry over, since the self-test moved the window on every frame.

**Found:** the user tried the window test by hand on the same PC, in two runs, and its reports are `2026-11-15-windows-by-hand-160214.txt` and `2026-11-15-windows-by-hand-160450.txt` in `experiments/areas/rack/results/` (measured on Windows 11 25H2, 2026-11-15).

- Every flick came out at about the same speed (user, 2026-11-15).
  The prototype caps a throw at 3,200px/s, enough to cross its 1,440px-wide desk, and friction stops a throw at that cap within 1,000px, under a fifth of the 5120px-wide display.
- A bounce off a screen edge sent the rack back into the way (user, 2026-11-15).
- The 1.5s hold before docking was far too long (user, 2026-11-15).
- Undocking pushed the rack about one rack width inward (user, 2026-11-15): the prototype sends it out at 950px/s, and friction stops it about 300px from the edge.
- Typing in the inline field worked (user, 2026-11-15).
  The field took keyboard focus all 4 times, and handed it back after Enter and after Esc; the other 2 times ended with a click elsewhere.
- Over 8,282 moves from real drags, throws, and 3 docks, Windows never started a move loop of its own on the rack, so Snap had no move to act on, and never put the rack anywhere other than where it was moved.
- In the second run, all 6 clicks on tiles and the strip left keyboard focus in the app that had it.
  In the first, 6 of 13 did, and the report didn't record whether the inline field had focus during the other 7, so the window test now counts those clicks apart.
- The rack's tiles were placeholders beside a readout of the test's numbers, and its look and feel were far from the mockups' (user, 2026-11-15).

**Changed:**

- A throw takes its speed from the drag's last 60ms, with no cap a hand reaches.
- A screen edge stops a thrown rack on that axis, with no bounce.
- The dock hold is 0.5s, and the rack slides into the edge over 200ms.
- Undocking slides the rack out of the edge over 200ms, to sit flush with it.
- The window test draws the rack from the mockups' styles: Inter, the mockups' sample tiles, timer slots, hover states, the inline field, and a strip that peeks 6px out when hovered.

**Affects:** [01-window](01-window.md).

## 2026-11-15: On Windows, GPUI's PopUp window moves on every frame at 240Hz, once a subclass removes its frame and keeps clicks from activating it

**Asked:** can GPUI give the rack the window [01-window](01-window.md) designs, on Windows?
This entry covers what the window test, `experiments/areas/rack/window-test/`, measures with its `--selftest` flag, which moves the rack without a person's mouse or keyboard.
The checks that need a person's hands come later.

**Expected:** GPUI's window options to cover part of the window, and calls through its native handle to cover the rest.

**Found:** we read gpui-pre 0.3.12's source, then ran the self-test on Windows 11 25H2, build 26200.9711, with one 5120x1440 display at 100% scale and 240Hz (measured on Windows 11 25H2, 2026-11-15, three reports in `experiments/areas/rack/results/`).

- `WindowKind::PopUp` gives the window `WS_EX_TOOLWINDOW` and `WS_EX_TOPMOST`, which keep it off the taskbar and on top of other windows, and `focus: false` shows it without activating it (read in gpui-pre-windows 0.3.12, src/window.rs, `WindowsWindow::new`).
  The rack opened without taking keyboard focus from the app that had it.
- GPUI can set a window's size but not its position, so the window test calls `SetWindowPos` with the native handle that GPUI's `Window` exposes through `raw-window-handle` (read in gpui-pre 0.3.12, src/window.rs, `Window::resize` and `impl HasWindowHandle for Window`).
- Moving the window from a foreground task on every frame moved it 240 times a second through two throws and a slide out of the dock, 1,240 moves in all.
  The median gap between moves was 4.2ms, one refresh interval, and the longest was 6.5ms.
  `SetWindowPos` took a median of 0.3ms.
  The OS never put the window anywhere other than where the window test moved it, and never started a move loop of its own.
- GPUI caps an inactive window at about 30 frames a second by default (read in gpui-pre 0.3.12, src/platform.rs, `WindowOptions::default`), and the rack is never the active window, so the window test sets `inactive_frame_interval` to `None`.
- GPUI answers every `WM_MOUSEACTIVATE` with `MA_ACTIVATE` (read in gpui-pre-windows 0.3.12, src/events.rs, `WindowsWindowInner::handle_msg`), so a click on the rack would take keyboard focus.
  A window subclass answers `MA_NOACTIVATE` before GPUI sees the message, and the window also carries `WS_EX_NOACTIVATE`.
  Whether a click then leaves focus where it was takes a person's click to measure.
- GPUI keeps Windows' resize frame on the left, right, and bottom of a window without a title bar, even one that can't be resized (read in gpui-pre-windows 0.3.12, src/events.rs, `handle_calc_client_size`).
  The frame is invisible, and it made the window 320x376 around a 304x368 rack, so a docked rack would have stopped 8px short of the screen edge.
  The subclass answers `WM_NCCALCSIZE` with the whole window as its client area, and the window then measured the rack's own size.
- With `WindowBackgroundAppearance::Transparent`, GPUI clears the window to transparent, but gpui-component's `Root` fills it with the theme's background, `#0A0A0A` (read in gpui-component 0.9.1, src/root.rs, the `Root` plugin's `style`).
  That showed as dark squares outside the rack's 14px corners until the window test set the theme's background to transparent.
- Without its frame, the window gets no rounded corners, border, or shadow from DWM.
  Captures before and after turning off DWM's corner rounding and border matched inside the window, and the screen beside the window was no darker than the screen further from it.

**Changed:** nothing in the design.
The rack's window in step 4 takes what the window test does: a PopUp window with its frame removed, clicks answered with `MA_NOACTIVATE`, the inactive frame cap off, moves made from a foreground task, and a transparent theme background.

**Affects:** [01-window](01-window.md) and [decision 001](../../decisions/001-rust-and-gpui.md), which this entry supports without settling, since the checks by hand and macOS remain.
