# Rack: the window

```text
Purpose:     What the rack's window is, how it moves and docks, and when it takes keyboard focus.
Depends on:  01-aims (../product/01-aims.md).
Decides:     The window's shape, the drag, throw, and dock gestures, the docked strip, and the focus rules.
```

The rack is Plates' one window: a 304px-wide rounded panel that stays on top of every other window and holds the tiles [02-tiles](02-tiles.md) describes.
You drag it from anywhere on it, throw it out of the way, and dock it to a side edge by holding it there.
[The mockups](https://claude.ai/artifact/A81TcK3BfzDVyQ2qaCqgNd) show it, and their interactive prototype is where the numbers on this page come from, as values to start tuning from on the real window.

## Shape

- The window is borderless, with 14px rounded corners, and has no taskbar or Dock entry.
- On macOS, it shows on every Space and over full-screen apps.
- It has no title.
  A 28px strip at its top holds a centered, horizontal grip of six dots.
- It has 12px of padding on the left, right, and bottom, and its tiles sit 6px apart.
- Its background is `#131417`, with a 1px `#2A2C31` border.

## Drag and throw

- A press anywhere on the rack becomes a drag once the pointer moves 5px.
  A click on a tile, its timer, or its check still lands, and a drag that starts on a tile doesn't also click it.
- Letting go mid-drag keeps the rack's speed, measured over the drag's last 90ms and capped at 3,200px/s.
- The rack then slides on, keeping about 4% of its speed after each second (a factor of e^−3.2), and bounces off a screen edge with 45% of its speed.
- A throw only moves the rack and never docks it, because a throw means "out of my way for now" (user, 2026-11-15).

## Docking

- You drag the rack against the left or right screen edge and hold it there.
  A blue bar fills along that edge, and after 1.5s the rack shrinks to the strip.
  Letting go before the bar fills leaves the rack where it is.
- The strip is 22px wide, with one 8px dot per tile, a pip, in the tiles' order.
  A session's pip is a circle and your own tile's is a square, colored as the tile's marker is, except that a running or finished timer shows yellow, since the strip has no room for the timer.
- When a tile starts needing you, the strip nudges 12px out of the edge once, then sits still.
- Hovering the strip lists the tiles that need you, with their pips.
- Clicking the strip slides the rack back out, at 950px/s away from the edge.

## Focus

- Clicking the rack never takes keyboard focus from the app you're in.
- Only the inline field for adding or renaming a tile takes focus, and focus goes back to the app you were in when you press Enter or Esc.

## OS snapping

Plates moves the rack's window from its own drag, so the OS never starts a window move, and Windows Snap and macOS window tiling get no move to act on (unverified, until [the window test](../product/plan.md#1-window-test) tries it).

## Left open

- Docking uses the left and right edges only.
- With several monitors, which one the rack starts on, and whether it remembers its place per monitor.
- Whether the rack hides from screen capture, and whether it hides while a full-screen app is in front.
