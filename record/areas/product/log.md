# Product: log

What we found about building and running Plates as a program, newest entry first.

## 2026-11-15: A Plates started from a shell inside the Claude desktop app keeps its data in the app's package

**Asked:** why did a Plates started from Explorer not find the `place.json` we had written to `%APPDATA%\Plates\`?

**Expected:** one `%APPDATA%` for every program you run.

**Found:** the Claude desktop app is a Windows package, and a program started from a shell the app runs, such as a Claude Code session's, runs inside the package too (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-console-allocation.txt](results/2026-11-15-console-allocation.txt)).
The `place.json` that a session's shell wrote to `%APPDATA%\Plates\` landed in the package's own folder, `%LOCALAPPDATA%\Packages\Claude_<id>\LocalCache\Roaming\Plates\`, beside a `rack.json` that a Plates started from the shell had written there.
So a Plates that a session restarts keeps your own tiles and the rack's place there, where a Plates started from Explorer can't see them.
A program that Explorer starts, such as a `.cmd` file, writes to the real `%APPDATA%`.
Whether Windows redirects every write under `%APPDATA%`, or only new files, is unverified.

**Changed:** nothing in Plates, since you start it from Explorer or a shortcut.
A Plates that an agent starts with `explorer.exe "<path to plates.exe>"` runs outside the package, as yours does.

**Affects:** nothing yet.
The installer and launching at login, in [the plan's Later section](plan.md#later), will start Plates outside the package.

## 2026-11-15: The detached console policy keeps Plates' console window away when Explorer starts it

**Asked:** can `plates.exe` open no console window when you start it from Explorer, and still print to the terminal when you run `plates hooks install` in one?

**Expected:** that linking a manifest with Windows' console allocation policy of `detached` into the exe would do both, as Microsoft documents for Windows 11 24H2 and later ([console allocation policy](https://learn.microsoft.com/en-us/windows/console/console-allocation-policy)).

**Found:** (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-console-allocation.txt](results/2026-11-15-console-allocation.txt))

- Without the policy, `plates.exe` started from Explorer got a `conhost.exe` and, with Windows Terminal set as the default terminal, a new Windows Terminal window.
- With it, `plates.exe` started from Explorer got neither, and `plates.exe` run from PowerShell still printed to the terminal, and PowerShell waited for it to exit.
- Linking a second manifest fails with `CVT1100: duplicate resource.  type:MANIFEST, name:1`, because gpui-pre already embeds one through its `windows-manifest` feature (read in gpui-pre 0.3.12, build.rs, `embed_resource`).
  That feature is one of gpui-pre's default features, which gpui-kit, gpui-base, and gpui-component each turn on.
- The Windows SDK's `mt.exe` merges the policy into the built exe's manifest, and the one in SDK 10.0.22621.0 keeps it after warning `manifest authoring warning 81010002: Unrecognized Element "consoleAllocationPolicy"`.

**Changed:** `tools/build-release.ps1` builds the release and merges `src/plates/console.manifest` into `plates.exe`.
A plain `cargo build` gives a `plates.exe` without the policy, which still works and opens a console window from Explorer.

**Affects:** [01-window](../rack/01-window.md#launching-and-quitting).
