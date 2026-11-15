# Product: aims

```text
Purpose:     What Plates is for, who it serves, and what it won't do.
Depends on:  nothing.
Decides:     Who Plates is for, its aims, and its non-goals.
```

Plates is built for us first: one person running several Claude Code sessions in the Claude desktop app, with other work beside them, on a Windows PC and a Mac.
It keeps every session and every other thing being juggled in view, so none of them is forgotten for an hour.
When two choices are otherwise even, Plates takes the calmer one, because it's meant to work as a focus aid rather than another source of noise (user, 2026-11-15).

## Aims

- **Everything in view at a glance.**
  One small rack, Plates' floating window, stays on top of every other window and shows a tile for each session and one for each thing you add yourself, called your own tiles.
  You read it without opening anything.
- **A tile asks for attention once, then waits.**
  A tile that starts needing you brightens once, and nothing flashes, pulses, or plays a sound ([02-tiles](../rack/02-tiles.md#attention)).
- **The rack never gets in the way.**
  You throw it aside or dock it to a side edge, and clicking it never takes your keyboard focus ([01-window](../rack/01-window.md)).
- **Adding a tile takes a hotkey, a name, and Enter.**
- **It's light enough to leave running all day** ([decision 001](../../decisions/001-rust-and-gpui.md)).
- **It runs on Windows and macOS.**

## Non-goals

- Syncing tiles or sessions between the PC and the Mac: each machine has its own rack (user, 2026-11-15).
- Sounds, OS notifications, and a quiet mode (user, 2026-11-15).
- Renaming sessions, since the new name wouldn't reach the Claude app (user, 2026-11-15).
- A clock on your own tiles beyond the timer you start (user, 2026-11-15).
- Jumping to a session in a terminal window, since our sessions run in the Claude desktop app (user, 2026-11-15).
- Cloud sessions, whose hooks don't read the settings file on our machine ([hooks log](../hooks/log.md#2026-11-15-claude-code-documents-hooks-in-desktop-sessions-and-an-http-hook-type-but-no-link-that-opens-one-desktop-session)).
