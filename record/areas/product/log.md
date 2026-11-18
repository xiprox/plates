# Product: log

What we found about building and running Plates as a program, newest entry first.

## 2026-11-16: A dry run's packages carry Plates' icon, and its macOS installer offers /Applications or ~/Applications

**Asked:** do the packages a release would make carry the app icon that `-i` and `src/plates/build.rs` give them, and where does the macOS installer put `Plates.app`?

**Expected:** the icon on the macOS bundle and on `Setup.exe`, as `vpk pack -i` sets them, and in `plates.exe`, as `build.rs` embeds it, and `Plates.app` in `/Applications` or `~/Applications`, as Velopack documents.

**Found:** the Release workflow's dry run on a snapshot of the working tree built and packed both platforms, so the Windows-only code that macOS can't build, such as the tray's click handler and `build.rs`, compiled there (run 43862446532, 2026-11-16, [results/2026-11-16-dry-run-packages.txt](results/2026-11-16-dry-run-packages.txt)).

- `Plates.app`'s `Info.plist` names `plates.icns` as `CFBundleIconFile`, and the bundle's `plates.icns` has the same SHA-256 as `src/plates/icons/plates.icns`.
- `Setup.pkg`'s `Distribution` enables installing for the current user's home and for the local system, its package installs to `/Applications`, and it has a `postinstall` script.
- `plates.exe`, the portable package's `Plates.exe`, and `Setup.exe` each hold one icon group of 8 images, from 16px to 256px, the sizes `plates.ico` holds, and a version resource with `FileDescription` `Plates`.
- `plates.exe`'s manifest still holds the `detached` console allocation policy that `tools/build-release.ps1` merges in after `build.rs` added its resources.

We installed neither package, so the Start menu and desktop shortcuts, the Apps entry, and a Dock icon at launch stay unverified.

**Changed:** nothing; the packages are as the release workflow and `build.rs` make them.

**Affects:** [releasing](../build/releasing.md#the-app-icon).

## 2026-11-16: v0.1.0's Plates.app has an icon Plates didn't supply, and no LSUIElement

**Asked:** what v0.1.0's macOS app bundle holds, which an installed Plates shows in Finder and in Apps, macOS 26's list of apps.

**Expected:** no icon of Plates' own, since v0.1.0's release passed `vpk pack` none.

**Found:** v0.1.0's `plates-osx-arm64-Portable.zip` holds `Plates.app`, whose `Info.plist` gives `CFBundleName` `Plates`, `CFBundleIdentifier` `com.plates.plates`, `CFBundleIconFile` `DefaultApp.icns`, and no `LSUIElement` (read with `plutil -p`, 2026-11-16, [results/2026-11-16-second-plates.txt](results/2026-11-16-second-plates.txt)).
`DefaultApp.icns` sits in the bundle's `Resources`, and nothing in Plates' repository makes a file of that name.

**Changed:** nothing in v0.1.0; the next release's `vpk pack` gets `plates.icns` with `-i`.

**Affects:** [releasing](../build/releasing.md#where-an-install-puts-plates), and the Dock icon question in [01-window](../rack/01-window.md#left-open).

## 2026-11-16: A second Plates quits after asking the running one to show its rack

**Asked:** what happens when you open Plates while it already runs, such as from the Start menu after Start at login opened it?

**Expected:** a second rack with no session tiles, since nothing stopped a second Plates, and it would fail to listen on port 47231.

**Found:** after the change below, we started the debug build, `./target/debug/plates`, three times while another Plates ran, and each time it quit with exit code 0, the second and third 0.13s and 0.08s after they started (measured on macOS 26.5.2, 2026-11-16, [results/2026-11-16-second-plates.txt](results/2026-11-16-second-plates.txt)).
The first run after the build took 2.58s to quit, which we take to be macOS checking a new program on its first launch (unverified).
The running Plates was another session's, and it may have been a build from before the change, which answers 200 to any request, so whether it moved its rack to the middle doesn't follow from this run.
We didn't open `Plates.app`, and didn't run this on Windows.

**Changed:** the receiver answers a post to `/plates/show` with 200 and sends the rack the same Show command as the tray's "Show Plates", and `plates listen` answers it with 404.
A starting Plates that can't listen on the port posts there, quits if the answer is 200, and otherwise opens its own rack without session tiles.

**Affects:** [01-window](../rack/01-window.md#launching-and-quitting).

## 2026-11-16: An .icns holding PNGs at 16px and 32px reads back as garbage at those sizes

**Asked:** does the `.icns` that `tools/make-icons.mjs` writes hold every size it should?

**Expected:** yes, since Apple's icon types `icp4` and `icp5` take a PNG at 16px and 32px, as the larger types do.

**Found:** `iconutil -c iconset` turned the `icp4` and `icp5` PNGs into noise, and every larger PNG back into its image (measured on macOS 26.5.2, 2026-11-16).
An `.icns` that `iconutil -c icns` built from the same PNGs held its 16px and 32px images as `ic04` and `ic05`, in an ARGB form whose data starts with `ARGB`, and PNGs for the rest.

**Changed:** `tools/make-icons.mjs` writes the 16px and 32px images as `ic04` and `ic05` in that ARGB form, and `iconutil` reads all 10 images back.

**Affects:** nothing in the design docs.

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
