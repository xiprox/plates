# Plates

An always-on-top overview of your Claude Code sessions and the other things you're juggling, so none of them drops while you're busy with the rest.

One small floating rack sits on top of everything. Each Claude session gets a tile that shows what it's doing and when it needs you. You can add your own tiles for anything else and set a quick timer on any tile to be reminded to look again.

![Three copies of the rack: as it sits, with a session's task list open, and with that session's hover card open over it. The rack juggles a magic links admin tool asking to merge its PR, a landing page redesign waiting on you to pick a footnote, a mobile scroll bug being fixed with a swipe test running under it, a reminder to merge a PR once its checks pass with its timer running, and a reminder to respond on Slack that's come due. The task list shows two tasks on you, two waiting on others, and two to do, and the card shows the landing page session's recap, its question, and the buttons that answer it](.github/readme/rack.png)

**Status:** design settled, build starting. Targets Windows and macOS. Rust + [GPUI](https://www.gpui.rs/).

- [record/index.md](record/index.md) maps the design docs, the plan, the logs, and the decisions.
- [record/areas/product/plan.md](record/areas/product/plan.md) is what's being built and where it stands.
