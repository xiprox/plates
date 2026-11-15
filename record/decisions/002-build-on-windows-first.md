# 002: Plates is built on Windows first, and tried on macOS once it's built

Plates is built and tested on Windows through every step of the plan, and the whole app, the window test's macOS half included, is tried on the Mac in one session at the end.
Decided by the user on 2026-11-15, after the window test passed on Windows.

## Context

[Decision 001](001-rust-and-gpui.md) commits the plan's step 1 to trying GPUI's window on both OSes before the rack is built, since the window is what decides whether GPUI holds.
The window test passed on Windows ([rack log](../areas/rack/log.md#2026-11-15-by-hand-the-prototypes-throw-bounce-and-dock-numbers-got-in-the-way)).
Only the user can run Plates on macOS, and bringing out the Mac costs a session of its own each time (user, 2026-11-15).

## Options

1. **Run the window test's macOS half now.**
   It settles decision 001 before the interface is built on GPUI.
   It costs one Mac session now and another once the app is built.
2. **Build the app on Windows, then try all of it on macOS in one session.**
   It costs one Mac session in all.
   If GPUI can't give the rack its window on macOS, the interface built on GPUI is at risk, while `plates-core`, which holds no GPUI code, is not.

## Decision

Option 2 (user, 2026-11-15).

## What it commits us to

- The plan's step 1 is the window test on Windows, and its macOS half runs in step 7 with the rest of the app.
- If GPUI can't give the rack its window on macOS, we weigh the options then, with what rebuilding the interface would cost known (user, 2026-11-15).
- Decision 001's commitment to trying both OSes before the rack is built no longer holds.
