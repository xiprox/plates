# Record

The documents agents write for Plates live here, in the kinds [writing.md](../.claude/rules/writing.md) describes.
Documentation people write for other people lives in `docs/` instead.

- [areas/product/01-aims.md](areas/product/01-aims.md): who Plates is for, what it aims at, and what it won't do.
- [areas/product/plan.md](areas/product/plan.md): what's being built, in order, and where each step stands.
- [areas/product/log.md](areas/product/log.md): what we found about building and running Plates as a program.
- [areas/rack/01-window.md](areas/rack/01-window.md): the rack's window, and how it moves, docks, and leaves your keyboard focus alone.
- [areas/rack/02-tiles.md](areas/rack/02-tiles.md): what each tile shows, the timer, how a tile asks for attention, and adding, renaming, and removing tiles.
- [areas/rack/03-holding-pings.md](areas/rack/03-holding-pings.md): how Plates holds its tone and its calls for attention while you type or read a reply in the Claude app.
- [areas/rack/log.md](areas/rack/log.md): what we found about the rack's window, from the window test in `experiments/areas/rack/`.
- [areas/core/01-core.md](areas/core/01-core.md): what `plates-core` holds, and its rules for tiles, timers, and saving.
- [areas/hooks/01-claude-code-hooks.md](areas/hooks/01-claude-code-hooks.md): how Claude Code's hooks and transcripts tell Plates what each session is doing.
- [areas/hooks/log.md](areas/hooks/log.md): what we found about Claude Code's hooks and the Claude desktop app.
- [areas/tasks/01-task-list.md](areas/tasks/01-task-list.md): how Claude keeps each session's task list, and how the rack shows it beside the session's tile.
- [areas/tasks/02-waits-and-pings.md](areas/tasks/02-waits-and-pings.md): how Claude ends a turn as waiting on something outside the session, and rings you when something you requested a ping for happens.
- [areas/tasks/log.md](areas/tasks/log.md): what we found about keeping a task list for each session through an MCP server.
- [areas/build/releasing.md](areas/build/releasing.md): how a release is made, and what files it holds.
- [areas/build/log.md](areas/build/log.md): what we found about building, versioning, and releasing Plates.
- [decisions/001-rust-and-gpui.md](decisions/001-rust-and-gpui.md): Plates is written in Rust, with GPUI for its interface.
- [decisions/002-build-on-windows-first.md](decisions/002-build-on-windows-first.md): Plates is built on Windows first, and tried on macOS once it's built.
- [decisions/003-releases-with-release-please-and-velopack.md](decisions/003-releases-with-release-please-and-velopack.md): releases come from release-please, packed with Velopack, for Windows and macOS.
