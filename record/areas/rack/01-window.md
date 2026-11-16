# Rack: the window

```text
Purpose:     What the rack's window is, how it moves and docks, and when it takes keyboard focus.
Depends on:  01-aims (../product/01-aims.md), and 02-tiles (02-tiles.md) for the markers the docked strip copies.
Decides:     The window's shape, the drag, throw, and dock gestures, the docked strip, the focus rules, where the rack opens, what opening a running Plates does, how you quit it, whether Plates starts at login, and Plates' icon in the tray and the menu bar.
```

The rack is Plates' one window: a 304px-wide rounded panel that stays on top of every other window and holds the tiles [02-tiles](02-tiles.md) describes.
You drag it from anywhere on it, throw it out of the way, and dock it to a side edge by holding it there.
[The mockups](https://claude.ai/artifact/A81TcK3BfzDVyQ2qaCqgNd) show it, and their interactive prototype is where the numbers on this page come from, as values to start tuning from on the real window.

## Shape

- The window is borderless, with 14px rounded corners, and has no taskbar or Dock entry.
  You reach Plates from outside the rack through its icon in the tray or the menu bar ([Tray and menu bar](#tray-and-menu-bar)).
- It stays above every other window, a game in borderless full screen included (user, 2026-11-15).
  On Windows, a game in borderless full screen is a topmost window like the rack, and Windows puts whichever topmost window went on top last above the others, so the game covers the rack once Windows raises it ([rack log](log.md#2026-11-15-another-programs-topmost-window-covers-the-rack-until-the-rack-raises-itself)).
  So when another program's window comes to the front, and again 200ms later and once a second, Plates checks whether another program's shown, topmost window covers any of the rack, and if one does, puts the rack back on top without taking focus.
  A game in exclusive full screen bypasses the Desktop Window Manager, and no other window shows over it (unverified).
- On macOS, it shows on every Space and over full-screen apps.
- It has no title.
  A 38px band at its top holds a centered, horizontal grip of six dots, and at its right the badges of [the archive](02-tiles.md#the-archive) and [the moon](02-tiles.md#snoozing), the moon rightmost, each shown while it has a count (user, 2026-11-16).
  The band is as tall as the 22px badges with 8px above and below them, so the badges sit level with the grip, and the rightmost sits as far from the rack's side as from its top.
- It has 12px of padding on the left, right, and bottom, and its tiles sit 6px apart.
- Its background is `#131417`, with a 1px `#2A2C31` border.
- It casts a drop shadow, which on Windows the Desktop Window Manager (DWM) draws outside the window ([rack log](log.md#2026-11-15-dwm-draws-the-racks-drop-shadow-once-the-windows-frame-reaches-1px-into-its-client-area)), and what draws it on macOS is unverified until the plan's step 7.
  The docked strip casts none, since DWM draws its shadow as a square-cornered rectangle around the whole window (user, 2026-11-15).

## Drag and throw

- The pointer is the arrow everywhere on the rack and its menus and card, with no pointing hand over what you can click, as in a desktop app, and the closed hand only while you drag the rack (user, 2026-11-16).
- A press anywhere on the rack becomes a drag once the pointer moves 5px.
  A click on a tile, its timer, or its × still lands, and a drag that starts on a tile doesn't also click it.
- Letting go mid-drag keeps the rack's speed, measured over the drag's last 60ms, and capped only at 20,000px/s, a guard against a bad sample (user, 2026-11-15, [rack log](log.md#2026-11-15-by-hand-the-prototypes-throw-bounce-and-dock-numbers-got-in-the-way)).
- The rack then slides on, keeping about 4% of its speed after each second (a factor of e^−3.2).
  A screen edge stops it on that axis, and it slides on along the edge, because a rack that bounced would come back into the way (user, 2026-11-15).
- A throw only moves the rack and never docks it, because a throw means "out of my way for now" (user, 2026-11-15).

## Docking

- You drag the rack against the left or right screen edge and hold it there.
  A blue bar fills along that edge, and after 0.5s the rack slides into the edge over 200ms and the strip takes its place (user, 2026-11-15).
  Letting go before the bar fills leaves the rack where it is.
- The strip is 22px wide, with one 8px mark per tile, called a pip, in the tiles' order.
  A session's pip is a circle colored as its tile's marker is.
  Your own tile's pip is a gray square, outlined in yellow while the tile's timer runs, or in violet while it's set for tomorrow, and solid yellow once the tile is time-up ([02-tiles](02-tiles.md#the-timer)), since the strip has no room for the timer, and time-up needs you where a running timer doesn't (user, 2026-11-15 and 2026-11-16).
  While any tile is snoozed, a violet moon follows the pips, as the grip band's moon does on the rack ([02-tiles](02-tiles.md#snoozing)).
- When a tile starts needing you, the strip nudges 12px out of the edge once, then sits still.
- Hovering the strip peeks it 6px out of the edge and lists the tiles that need you, with their pips.
- Clicking the strip slides the rack back out of the edge over 200ms, to sit flush with it (user, 2026-11-15).

## Focus

- On Windows, clicking the rack never takes keyboard focus from the app you're in.
  On macOS, the rack is a panel that takes the keyboard when you click it, without making Plates the active app, so after a click your typing goes to the rack (seen on macOS 26.5.2, 2026-11-16).
  A click on a session tile hands the keyboard on to the Claude app, as [01-claude-code-hooks](../hooks/01-claude-code-hooks.md#opening-a-session) says, and a click anywhere else leaves it with the rack.
- Only the inline field for adding or renaming a tile takes focus, and focus goes back to the app you were in when you press Enter or Esc, or click the rack outside the field.

## Launching and quitting

- The rack opens where it last came to rest, docked or not, which Plates saves to `place.json` in its data folder each time the rack stops somewhere new (user, 2026-11-15).
  With no `place.json`, or one that doesn't parse, it opens at the center of the primary monitor.
  If the saved place is on a monitor that's gone, or would leave the rack partly off its monitor, the rack opens at the nearest place inside the nearest monitor's work area.
- On Windows, starting Plates from Explorer or a shortcut opens no console window, because `tools/build-release.ps1` gives `plates.exe` a console allocation policy of `detached` ([product log](../product/log.md#2026-11-15-the-detached-console-policy-keeps-plates-console-window-away-when-explorer-starts-it)).
  The policy needs Windows 11 24H2 (build 26100) or later (documented, [console allocation policy](https://learn.microsoft.com/en-us/windows/console/console-allocation-policy), as of 2026-11-15).
  That an earlier Windows ignores it and opens the console window is unverified, and starting the release build on Windows 11 23H2 would settle it.
- Clicking the grip, or right-clicking the grip band at the rack's top, opens a menu with "Start at login" and "Quit Plates", in a popup at the pointer like the timer's menu (user, 2026-11-15 and 2026-11-16).
  The grip is a button as tall as the moon's badge, and a second click on it closes the menu, as on the moon.
  Pressing it and moving drags the rack, as anywhere else on the rack does.
  While the rack is docked, you click the strip to slide the rack out, and then right-click the grip band.
- "Start at login" is off until you click it, since it adds Plates to what Windows runs when you sign in, and a check mark shows while it's on (user, 2026-11-16).
  On Windows, it writes a value named "Plates", holding the program's path in quotes, under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, and clicking it again deletes the value.
  Windows runs each value under that key every time you sign in (documented, [Run and RunOnce registry keys](https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys), as of 2026-11-16), and we haven't measured that: signing out and back in with Start at login on would settle it.
  The value names the program you turned it on from, so a build in another folder needs it turned on again.
  A Plates started from a Claude Code shell in the Claude desktop app runs inside the app's package, and files it wrote to `%APPDATA%\Plates\` landed in the package's own folder ([product log](../product/log.md#2026-11-15-a-plates-started-from-a-shell-inside-the-claude-desktop-app-keeps-its-data-in-the-apps-package)).
  Whether its registry writes land in the package too is unverified, and reading the `Run` key from a terminal that Explorer started, after turning it on from such a Plates, would settle it.
  Until then, turn it on from a Plates started from Explorer.
- You open Plates by hand where the installers put it ([releasing](../build/releasing.md#where-an-install-puts-plates)).
- On Windows, and on macOS when you run the `plates` program rather than open `Plates.app`, opening Plates while it runs moves the rack to the middle of the primary monitor's work area, as "Show Plates" does, and opens no second rack (user, 2026-11-16).
  Only one Plates can hold the port the hooks post to, so a second rack would never get a session's tile.
  A starting Plates that finds the port taken posts to `/plates/show` on it, and quits once the running Plates answers 200, before it reads or writes any of its data files.
  On any other answer, such as the 404 that `plates listen` gives, or with no connection within 500ms, it opens its own rack without session tiles.
  On macOS, a debug build started while another Plates ran quit 0.08s and 0.13s after it started (measured on macOS 26.5.2, 2026-11-16, [product log](../product/log.md#2026-11-16-a-second-plates-quits-after-asking-the-running-one-to-show-its-rack)), and we haven't run this on Windows.
  That the running Plates then moves its rack to the middle is unverified, and opening Plates while a build with `/plates/show` runs, on Windows and on macOS, would settle it.
- On macOS, opening `Plates.app` while it runs makes LaunchServices send the running Plates a reopen rather than start a second one, so the rack stays where it is, and the menu bar's "Show Plates" moves it to the middle (unverified, and opening an installed `Plates.app` while it runs would settle it).
  Plates registers no `on_reopen` callback, and GPUI would call one only while the app has no visible window, which never happens while the rack is open (read in gpui-pre-macos 0.3.12, `src/platform.rs`, `should_handle_reopen`).
- A menu opens below and to the right of the pointer, to its left where the monitor's work area leaves no room on the right, and above the pointer where it leaves no room below (user, 2026-11-15).
- Each menu fades in over 120ms.
  On Windows, the menu turns on DWM's shadow only once the fade ends, since DWM draws the shadow at full strength from the moment it's on (unverified; watching a menu open with its shadow on from the start would settle it).

## Tray and menu bar

- Plates puts its icon in Windows' notification area and in macOS' menu bar, since the rack has no taskbar or Dock entry, and a rack out of sight has no other way back (user, 2026-11-16).
- The icon's menu holds "Show Plates", "Start at login", and "Quit Plates".
  "Start at login" is the same setting as the grip band's, and changing it in either menu changes the check mark in both.
- "Show Plates" takes the rack out of any dock and puts it at the middle of the primary monitor's work area, since a rack out of sight can be under another window, docked at the edge of a monitor you're not looking at, or off every monitor.
- On Windows, clicking the icon does what "Show Plates" does, and right-clicking it opens the menu, as Windows' own tray icons do.
  On macOS, clicking it opens the menu, as a menu bar item does.
- The app icon is a stack of plates on the rack's own dark panel: four in the blue of a working session's marker, and one more in the orange of a session that needs you, being set down on top, its shadow already on the plate below (user, 2026-11-16).
  The tray's icon on Windows is the app icon.
- The menu bar's icon is a template image of the same moment seen side on, two plates and one being set down, which macOS tints to suit a light or dark menu bar (documented, [NSImage.isTemplate](https://developer.apple.com/documentation/appkit/nsimage/istemplate), as of 2026-11-16).
  It's side on because at 18pt a plate seen from above reads as a coin, and it stacks two plates rather than four because at 18 pixels on a 1x screen four run together.
- `tools/make-icons.mjs` draws every icon, and `src/plates/icons/` holds what it writes.

## OS snapping

Plates moves the rack's window from its own drag, so the OS never starts a window move, and Windows Snap and macOS window tiling get no move to act on.
On Windows, the OS started no move loop on the rack over 8,282 moves by hand (measured on Windows 11 25H2, 2026-11-15, [rack log](log.md#2026-11-15-by-hand-the-prototypes-throw-bounce-and-dock-numbers-got-in-the-way)), and on macOS it's unverified until [the window test](../product/plan.md#1-window-test) runs there.

## Left open

- How Start at login adds Plates to what macOS opens at login.
- How long macOS shows a Dock icon for an installed `Plates.app`, whose `Info.plist` has no `LSUIElement` ([product log](../product/log.md#2026-11-16-v010s-platesapp-has-an-icon-plates-didnt-supply-and-no-lsuielement)), before Plates makes itself an accessory app as the rack opens.
  Opening a dry run's `Plates.app` while watching the Dock would settle it.
- Whether the rack also docks to the top and bottom edges.
- Whether the rack keeps a place for each arrangement of monitors, so that when you unplug a laptop from its external monitors, the rack goes back to where it sat on the laptop's own screen.
- Whether the rack hides from screen capture.
- Whether the rack, by covering part of a game in borderless full screen, costs the game frame rate, since Windows then composes the game's frames with the rack's rather than showing them directly (unverified).
