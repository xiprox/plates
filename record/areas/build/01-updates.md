# Build: updates

```text
Purpose:     How an installed Plates finds a newer release, downloads it while it runs, and installs it when you click.
Depends on:  decision 004 (../../decisions/004-plates-updates-itself.md), for why; releasing (releasing.md), for the
             files a release holds; 01-window (../rack/01-window.md#launching-and-quitting), for the rack's menu;
             02-tiles (../rack/02-tiles.md#colors-and-type), for the teal; 01-task-list (../tasks/01-task-list.md),
             for the task server each session runs.
Decides:     When Plates looks for an update, what it downloads, what the grip and the rack's menu show, and what
             installing does.
```

An installed Plates looks for a newer release on GitHub, downloads it in the background, and puts a teal dot on the grip.
Clicking its row in the rack's menu, such as "Update to 0.6.0", installs it and starts Plates again (user, 2026-11-25).

## Looking for an update

Plates looks through Velopack's `UpdateManager`, with a `GithubSource` for `https://github.com/xiprox/plates` and no token.
Each look asks GitHub's API for the newest 10 releases, downloads each one's `releases.<channel>.json`, and picks the newest full package in them (read in velopack 1.2.161, `src/sources/github.rs`, `get_releases`, and `src/manager.rs`, `check_for_updates`).
The channel is the one the copy was packed on, so Windows reads the `win-x64` feeds and macOS the `osx-arm64` ones (same crate, `src/manager.rs`, `get_practical_channel`).

Without a token, GitHub's API answers 60 requests an hour from one IP address (documented, [Rate limits for the REST API](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api), as of 2026-11-25).
A look that finds nothing newer makes one API request, since without a token Velopack downloads each feed from its `browser_download_url` on `github.com`.
A look that downloads an update makes one more API request for each package it downloads (read in velopack 1.2.161, `src/sources/github.rs`, `get_asset_url_from_name` and `download_release_entry`).
Whether GitHub counts the `github.com` downloads against the same limit is unverified, since its page doesn't say.

- Plates looks 30s after it starts, again every 6 hours, and when you click the row while it offers a look.
- A look may take 2 minutes and a download 30, after which it counts as failed, since Velopack's requests have no time limit of their own (read in velopack 1.2.161, `src/download.rs`, `HttpOptions`, whose `TimeoutMilliseconds` of 0 never times out), and a request that stalls would leave the row as it was until Plates restarts.
- A look that finds a newer release downloads it at once.
  It takes the delta packages from the installed version up to the new one when the feeds hold a delta for the new one.
  Otherwise, or if applying the deltas fails, it takes the full package (read in velopack 1.2.161, `src/manager.rs`, `create_delta_update_strategy` and `download_updates`).
- An update downloaded before Plates last quit is ready as Plates starts, with no look (same file, `get_update_pending_restart`).
  Plates turns off Velopack's install of a downloaded update as the program starts, which would otherwise install it the next time any `plates` starts, such as a second one opened while the rack runs, or `plates hooks install` (read in velopack 1.2.161, `src/app.rs`, `VelopackApp::run`).
- A copy outside Velopack's layout, such as a build run from `target\release\`, never looks, since `UpdateManager::new` finds no Velopack manifest beside Plates and fails (read in velopack 1.2.161, `src/locator.rs`, `auto_locate_app_manifest`).
  A copy unzipped from the Portable zip has that layout, so it looks and updates as an installed one does.

## What the rack shows

While an update is ready, a 6px teal dot sits on the grip's top-right corner, telling you the rack's menu has news before you open it.

The rack's menu, the one the grip opens ([01-window](../rack/01-window.md#launching-and-quitting)), has a row for updates above "Quit Plates".
Its words say where an update stands, and its right end shows the version that runs, such as "0.5.0":

| Where it stands | The row reads | A click |
| --- | --- | --- |
| No look has finished | "Check for updates" | looks |
| A look is running | "Checking for updates…" | does nothing |
| The last look found nothing newer | "Up to date" | looks |
| The last look, its download, or the updater's start failed | "Couldn't check for updates" | looks |
| An update is downloading | "Downloading 0.6.0…" | does nothing |
| An update is ready | "Update to 0.6.0", with the teal dot in place of the version | installs it |
| The update is installing | "Updating…" | does nothing |
| Plates has no Velopack layout around it | "Development build" | does nothing |

A click that looks leaves the menu open, and the row follows the look.
The menus of the tray icon and the menu bar item have no row for updates.

## Installing

Clicking "Update to 0.6.0" starts Velopack's updater, which waits up to 60s for Plates to quit, puts the new version in place, and starts Plates again (documented, [Velopack: integrating](https://docs.velopack.io/integrating/overview), as of 2026-11-25, and in velopack 1.2.161's comment on `wait_exit_then_apply_updates`).
Plates starts the updater off the click, since starting a program on the rack's thread can let GPUI run work while the click still holds its state, as `ShellExecuteW` did ([rack log](../rack/log.md#2026-11-15-shellexecutew-on-the-racks-thread-lets-gpui-run-a-task-mid-click-and-plates-crashes)).
Once the updater has started, Plates quits as "Quit Plates" does.

On Windows, after starting the updater and before quitting, Plates stops each `plates-tasks.exe` that runs from its own folder, as `tools/build-release.ps1` stops the ones in `target\release\`, since Windows won't replace a running program's file and each Claude Code session keeps its own server running.
Claude Code then starts each session's server again at the session's next call to a task tool, as it did on macOS (measured on macOS 26.5.2, 2026-11-20, [tasks log](../tasks/log.md#2026-11-20-a-running-session-didnt-get-its-restarted-servers-new-instructions)), and whether it does on Windows is [left open](#left-open).
Other running instances of an app can make a Velopack update fail or be terminated (documented, [Velopack: integrating](https://docs.velopack.io/integrating/overview), as of 2026-11-20), and whether Velopack's updater counts `plates-tasks` among them is unverified.
On macOS, Plates leaves them running, since macOS replaces a running program's file (unverified: updating an installed Plates on the Mac while a session runs its server would settle it).

## Left open

- Whether an update installs and starts Plates again, on Windows and on macOS.
  Installing the first release with the update check, publishing the release after it, and clicking "Update to …" would settle it.
- Whether Claude Code starts a stopped `plates-tasks` again on Windows, as it did on macOS.
  Calling a task tool in a session after an update would settle it.
- What a hook does while the updater replaces the folder: Claude Code runs `plates-hook.exe` from it at every hook event, so a hook could find the program missing or half-copied, and one still running, for up to its 300ms wait to connect, could hold the file.
  Running hooks in a session during an update would settle it.
