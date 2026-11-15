# Rack: the window

```text
Purpose:     What the rack's window is, how it moves and docks, and when it takes keyboard focus.
Depends on:  01-aims (../product/01-aims.md), and 02-tiles (02-tiles.md) for the markers the docked strip copies.
Decides:     The window's shape, the drag, throw, and dock gestures, the docked strip, the focus rules, and where the rack opens and how you quit it.
```

The rack is Plates' one window: a 304px-wide rounded panel that stays on top of every other window and holds the tiles [02-tiles](02-tiles.md) describes.
You drag it from anywhere on it, throw it out of the way, and dock it to a side edge by holding it there.
[The mockups](https://claude.ai/artifact/A81TcK3BfzDVyQ2qaCqgNd) show it, and their interactive prototype is where the numbers on this page come from, as values to start tuning from on the real window.

## Shape

- The window is borderless, with 14px rounded corners, and has no taskbar or Dock entry.
- On macOS, it shows on every Space and over full-screen apps.
- It has no title.
  A 28px band at its top holds a centered, horizontal grip of six dots.
- It has 12px of padding on the left, right, and bottom, and its tiles sit 6px apart.
- Its background is `#131417`, with a 1px `#2A2C31` border.
- It casts a drop shadow, which on Windows the Desktop Window Manager (DWM) draws outside the window ([rack log](log.md#2026-11-15-dwm-draws-the-racks-drop-shadow-once-the-windows-frame-reaches-1px-into-its-client-area)), and what draws it on macOS is unverified until the plan's step 7.
  The docked strip casts none, since DWM draws its shadow as a square-cornered rectangle around the whole window (user, 2026-11-15).

## Drag and throw

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
  Your own tile's pip is a gray square, outlined in yellow while the tile's timer runs and solid yellow once the tile is time-up ([02-tiles](02-tiles.md#the-timer)), since the strip has no room for the timer, and time-up needs you where a running timer doesn't (user, 2026-11-15).
- When a tile starts needing you, the strip nudges 12px out of the edge once, then sits still.
- Hovering the strip peeks it 6px out of the edge and lists the tiles that need you, with their pips.
- Clicking the strip slides the rack back out of the edge over 200ms, to sit flush with it (user, 2026-11-15).

## Focus

- Clicking the rack never takes keyboard focus from the app you're in.
- Only the inline field for adding or renaming a tile takes focus, and focus goes back to the app you were in when you press Enter or Esc.

## Launching and quitting

- The rack opens where it last came to rest, docked or not, which Plates saves to `place.json` in its data folder each time the rack stops somewhere new (user, 2026-11-15).
  With no `place.json`, or one that doesn't parse, it opens at the center of the primary monitor.
  If the saved place is on a monitor that's gone, or would leave the rack partly off its monitor, the rack opens at the nearest place inside the nearest monitor's work area.
- On Windows, starting Plates from Explorer or a shortcut opens no console window, because `tools/build-release.ps1` gives `plates.exe` a console allocation policy of `detached` ([product log](../product/log.md#2026-11-15-the-detached-console-policy-keeps-plates-console-window-away-when-explorer-starts-it)).
  The policy needs Windows 11 24H2 (build 26100) or later (documented, [console allocation policy](https://learn.microsoft.com/en-us/windows/console/console-allocation-policy), as of 2026-11-15).
  That an earlier Windows ignores it and opens the console window is unverified, and starting the release build on Windows 11 23H2 would settle it.
- Right-clicking the grip band at the rack's top opens a menu with "Quit Plates", in a popup at the pointer like the timer's menu (user, 2026-11-15).
  Each menu fades in over 120ms, and DWM's shadow appears once it has, since DWM can't fade a shadow.
  A menu opens below and to the right of the pointer, to its left only where the screen's right edge leaves no room, and above it near the screen's bottom (user, 2026-11-15).
  While the rack is docked, you click the strip to slide the rack out, and then right-click the grip band.

## OS snapping

Plates moves the rack's window from its own drag, so the OS never starts a window move, and Windows Snap and macOS window tiling get no move to act on.
On Windows, the OS started no move loop on the rack over 8,282 moves by hand (measured on Windows 11 25H2, 2026-11-15, [rack log](log.md#2026-11-15-by-hand-the-prototypes-throw-bounce-and-dock-numbers-got-in-the-way)), and on macOS it's unverified until [the window test](../product/plan.md#1-window-test) runs there.

## Left open

- Whether the rack also docks to the top and bottom edges.
- Whether the rack keeps a place for each arrangement of monitors, so that when you unplug a laptop from its external monitors, the rack goes back to where it sat on the laptop's own screen.
- Whether the rack hides from screen capture, and whether it hides while a full-screen app is in front.
