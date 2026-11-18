# Releasing

A release is one merge: release-please keeps a release pull request (PR) open on `main`, and merging it starts the Release workflow, which builds, packs, and publishes the release.
[Decision 003](../../decisions/003-releases-with-release-please-and-velopack.md) says why, and `.github/workflows/release.yml` is the workflow.

## How a version is chosen

release-please reads each commit title on `main` since the last release, so titles follow Conventional Commits, as [writing.md](../../../.claude/rules/writing.md#commit-messages) describes.
Before 1.0, a `feat` or a breaking change (`!`) raises the minor version, and a `fix` raises the patch.
A type `CHANGELOG.md` leaves out, such as `chore`, `docs`, or `ci`, doesn't make a release on its own (read in release-please's main branch on 2026-11-16, `src/strategies/base.ts`, `changelogEmpty`).
A `feat` or `fix` title, without its type, becomes a line in `CHANGELOG.md`.
The first release is 0.1.0, `release-please-config.json`'s `initial-version`.

## How to make a release

1. Push `main` with a `feat` or `fix` commit since the last release.
   You'll see release-please open or update its release PR, with the next version and the changelog.
2. Squash-merge the release PR, since the repository takes no merge commits ([CLAUDE.md](../../../CLAUDE.md)).
   You'll see a GitHub release and its tag appear.
3. Wait for the Release workflow's two `package` jobs, one per platform, to finish.
   You'll see these files on the release, where `<channel>` is `win-x64` or `osx-arm64`:

   | File | What it is |
   | --- | --- |
   | `plates-win-x64-Setup.exe` | the Windows installer |
   | `plates-win-x64-Portable.zip` | the Windows app, run without installing |
   | `plates-osx-arm64-Setup.pkg` | the macOS installer |
   | `plates-osx-arm64-Portable.zip` | the macOS app bundle, run without installing |
   | `plates-<version>-<channel>-full.nupkg`, and `-delta.nupkg` from the second release on | the packages installed copies update from |
   | `releases.win-x64.json`, `releases.osx-arm64.json` | the update feeds installed copies read |

Velopack names each file from the pack ID `plates` and the channel (read in Velopack 1.2.161, `src/vpk/Velopack.Core/DefaultName.cs`, `GetSuggestedSetupName`, `GetSuggestedPortableName`, and `GetSuggestedReleaseName`).

## The app icon

`tools/make-icons.mjs` draws the app icon and writes it to `src/plates/icons/`, as `plates.ico` for Windows and `plates.icns` for macOS.
`src/plates/build.rs` puts `plates.ico` in `plates.exe` as a resource, and the workflow hands `vpk pack` the platform's icon with `-i`.
In a dry run's packages, `Plates.app` names `plates.icns` as its icon and holds the same bytes as `src/plates/icons/plates.icns`, and `plates.exe`, the portable package's `Plates.exe`, and `plates-win-x64-Setup.exe` each hold an icon with the `.ico`'s 8 sizes, from 16px to 256px (read on 2026-11-16, [product log](../product/log.md#2026-11-16-a-dry-runs-packages-carry-plates-icon-and-its-macos-installer-offers-applications-or-applications)).
Which icon the Start menu and desktop shortcuts show is unverified, and installing a dry run's `Setup.exe` on Windows would settle it.

## Where an install puts Plates

| Installer | Puts Plates in | Where you open it by hand |
| --- | --- | --- |
| `plates-win-x64-Setup.exe` | `%LocalAppData%\plates\` | a shortcut in the Start menu, and one on the desktop |
| `plates-osx-arm64-Setup.pkg` | `/Applications/Plates.app` or `~/Applications/Plates.app`, as you choose while installing | Applications in Finder, and Apps on macOS 26 |

Velopack's installer decides every cell, and the Windows folder is lowercase because Velopack names it `%LocalAppData%\{packId}` from the pack ID `plates` (documented, [Velopack installers](https://docs.velopack.io/packaging/installer), as of 2026-11-16).
The desktop shortcut is Velopack's default, and Plates keeps it (user, 2026-11-16).
A dry run's `Setup.pkg` offers to install for all users or for you alone, installs to `/Applications`, and runs a `postinstall` script, which Velopack documents as opening the app (read with `pkgutil --expand`, 2026-11-16, [product log](../product/log.md#2026-11-16-a-dry-runs-packages-carry-plates-icon-and-its-macos-installer-offers-applications-or-applications)).
That Apps lists an installed `Plates.app`, and that `Setup.exe` makes both shortcuts, are unverified, and installing a dry run's packages would settle them.

## How to release from a tag

1. Push a tag named for the version, such as `v0.2.0`.
   You'll see the Release workflow create a GitHub release for the tag and attach both platforms' files to it, as step 3 above lists them.

A tag release leaves `CHANGELOG.md`, `Cargo.toml`'s version, and `.release-please-manifest.json` as they were, and its notes are GitHub's generated ones.
Whether release-please's next release PR proposes a version the tag already took is unverified, and pushing a tag while a release PR is open would settle it.

## How to try the packaging without releasing

1. In GitHub, open Actions, then Release, then Run workflow.
   You'll see a new Release run start, with two `package` jobs.
2. When the run finishes, download its `packages-win-x64` and `packages-osx-arm64` artifacts.
   They hold a release's files, versioned `<version>-dryrun.<run number>` from `Cargo.toml`'s current version and without delta packages, and the workflow publishes nothing.
