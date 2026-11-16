# Rack: log

What we found about the rack's window and tiles, newest entry first.

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
`showcase::compose` lays the rack twice on a 1,200 by 750 canvas, a blue-to-violet gradient, and the card over the second rack where its window sat beside the rack's on screen, with a soft shadow under each, and saves it at 2x, 2,400 by 1,500 (user, 2026-11-16).
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
