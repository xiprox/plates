<h1><img src="src/plates/icons/plates.png" alt="" width="44" align="top"> Plates</h1>

An always-on-top overview of your Claude Code sessions and the other things you're juggling, which you can toss around and tuck away.

![Three copies of the rack: as it sits, with a session's task list open, and with that session's hover card open over it. The rack juggles a magic links admin tool asking to merge its PR, a landing page redesign waiting on you to pick a footnote, a mobile scroll bug being fixed with a swipe test running under it, a reminder to merge a PR once its checks pass with its timer running, and a reminder to respond on Slack that's come due. The task list shows two tasks on you, two waiting on others, and two to do, and the card shows the landing page session's recap, its question, and the buttons that answer it](.github/readme/rack.png)

## What it does

- **A tile for each session.**
  Plates keeps a tile for each Claude Code session on the rack, its floating window.
  The tile shows Claude's current step while Claude works, and when Claude asks you something or wants a tool approved, it turns amber or orange and starts a timer.
  Clicking it opens the session in the Claude desktop app.
- **A hover card.**
  Hovering a tile shows Claude's last message, and buttons for the choices Claude offers at the end of a turn in the Claude desktop app.
- **Your own tiles.**
  For anything else, press Ctrl+Shift+Space on Windows or ⌃⇧Space on macOS, type a name, and press Enter.
- **Timers and snoozing.**
  Set a timer on any tile, and Plates plays a soft tone when it runs out.
  Snooze a tile to take it off the rack for 2 or 4 hours, or until tomorrow morning.
- **Task lists.**
  Claude keeps a short list of a session's bigger pieces of work on its tile, so you see which wait on you, which wait on someone else, and which are done.
- **A rack you can toss and dock.**
  Drag the rack anywhere, toss it aside, or hold it against a side of the screen to dock it as a strip of colored dots.
  On Windows, clicking the rack leaves your keyboard focus where it was.

## How to install it

Plates runs on Windows (x64) and macOS (Apple silicon), with Claude Code in the Claude desktop app or in a terminal.

1. Download the installer from the [latest release](https://github.com/xiprox/plates/releases/latest): `plates-win-x64-Setup.exe` for Windows, or `plates-osx-arm64-Setup.pkg` for macOS.
2. Run the installer.
   The installers aren't signed, so Windows SmartScreen or macOS Gatekeeper stops it the first time.
   On Windows, click "More info", then "Run anyway".
   On macOS, open System Settings > Privacy & Security, and click "Open Anyway", as [Safely open apps on your Mac](https://support.apple.com/en-us/102445) describes.
3. Connect Plates to Claude Code from a terminal.
   In PowerShell on Windows:

   ```powershell
   & "$env:LOCALAPPDATA\plates\current\plates.exe" hooks install
   ```

   On macOS:

   ```bash
   /Applications/Plates.app/Contents/MacOS/plates hooks install
   ```

   You'll see the hooks it adds to `~/.claude/settings.json`, and it writes them once you confirm, after backing the file up.
   It also registers `plates-tasks`, Plates' task list server, with Claude Code.
4. Start a Claude Code session.
   You'll see its tile on the rack.

Plates' icon in the tray on Windows, or the menu bar on macOS, shows the rack, starts Plates at login, and quits it.
On macOS, the hover card's buttons press the Claude desktop app's own buttons, so they need Plates turned on under System Settings > Privacy & Security > Accessibility.

To disconnect Plates, run the same command with `hooks uninstall`, which removes only what Plates added.

## How it works

Claude Code runs Plates' hooks at each step of a session, such as when you send a prompt, Claude calls a tool, or a turn ends.
Each hook passes its event to Plates on `127.0.0.1`.
Plates reads Claude's last message from the session's transcript, and the session's title from the Claude desktop app's files.
The task lists come from `plates-tasks`, a Model Context Protocol (MCP) server that Claude Code starts for each session.
Plates has no account and sends nothing off your machine.

## Where it stands

We build Plates for one setup first: several Claude Code sessions in the Claude desktop app, on a Windows PC and a Mac.
Opening a session from its tile and answering its choices work only for sessions in the Claude desktop app.

## How to build it

With stable Rust installed, run this from the repository's root:

```bash
cargo run --release -p plates
```

`src/` holds the Cargo workspace: `plates` is the app, `plates-core` holds its rules for tiles, timers, and saving, `plates-hook` is the program Claude Code's hooks run, and `plates-tasks` is the task list server.
`record/` holds the design docs, the plan, the logs, and the decisions, which Claude Code agents write as we build Plates.
[record/index.md](record/index.md) maps them.
