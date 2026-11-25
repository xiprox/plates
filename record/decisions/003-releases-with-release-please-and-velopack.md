# 003: Releases come from release-please, packed with Velopack, for Windows and macOS

> **Note**
> [Decision 004](004-plates-updates-itself.md) reverses the point under "What it commits us to" that Plates ships without an update check of its own: Plates looks for its own updates.

A push to `main` updates release-please's release pull request (PR), merging it creates the GitHub release, and the same workflow builds Windows and macOS, packs each with Velopack, and attaches the installers and update packages to the release.
Decided by the user on 2026-11-16: they asked to "implement release please with velopack" the way choom-collector does, with "a release on github for both platforms".

## Context

Plates was built by `tools/build-release.ps1` on the user's PC and run from `target\release\`, with nothing to install it from or update it with.
The user's choom-collector repository releases this way already, for Windows and Linux (read in choom-collector, `.github/workflows/release.yml` and `record/decisions/004-releases-with-release-please-and-velopack.md`).
Plates runs on Windows, and its macOS half waits for the plan's step 7 ([decision 002](002-build-on-windows-first.md)), so a macOS release needs Plates to build on macOS before anyone has run it there.

## Options

1. **Build locally and upload by hand.**
   Nothing to set up, but each release waits for a build on Windows and another in a session on the user's Mac.
2. **Tag releases by hand, and let continuous integration (CI) build, pack with Velopack, and attach them.**
   Installs get delta packages, but each version number and changelog is written by hand.
3. **Let release-please tag the releases, in front of the same CI packing, as choom-collector does.**
   Versions and changelogs come from commit titles, at the cost of a commit format everyone follows.

## Decision

Option 3, built in `.github/workflows/release.yml`, with option 2's pushed `v*` tag kept as a way to release without the release PR.
[releasing](../areas/build/releasing.md) says how to make one.

## What it commits us to

- Commit titles follow Conventional Commits ([writing.md](../../.claude/rules/writing.md#commit-messages)).
- Windows packs on the `win-x64` channel and macOS on `osx-arm64`, so their files never clash on the shared release: Velopack puts the channel in every file name (read in Velopack 1.2.161, `src/vpk/Velopack.Core/DefaultName.cs`, `GetSuggestedReleaseName`).
- The `velopack` crate's version and the workflow's `VPK_VERSION` move together.
- release-please writes each version into `[workspace.package]` in `Cargo.toml` through the `extra-files` entry in `release-please-config.json`, and nothing writes it into `Cargo.lock`, so CI builds without `--locked`.
- The macOS release ships before the plan's step 7 tries it: CI's macOS job is the only check `src/plates/src/native/mac.rs` gets until then.
  Decision 002 still holds for trying macOS, which step 7 does.
- Plates ships without an update check of its own, so a user updates by running a newer installer over the installed copy (unverified until a second release installs over the first).
  `main()` runs Velopack's install, update, and uninstall hooks before anything else, through `velopack::VelopackApp::build().run()`.
- Once Plates looks for updates, it will read them from the repository's GitHub releases, which a private repository hides from anyone without a token (unverified: fetching a release's `releases.win-x64.json` with no token would settle it), so updates wait for the repository to go public.
