# Build: log

What we found about building, versioning, and releasing Plates, newest entry first.

## 2026-11-25: Once the repository is public, a release's update feed downloads without a token

**Asked:** once the repository is public, does a release's update feed download without a GitHub token, as [decision 003](../../decisions/003-releases-with-release-please-and-velopack.md) left unverified?

**Expected:** yes once the repository is public, since a public repository's release files download from `github.com` with no sign-in.

**Found:** after the repository went public, `releases/latest/download/releases.win-x64.json` answered HTTP 200 with the 0.5.0 feed, 497 bytes, with no `Authorization` header (measured on Windows 11 25H2, 2026-11-25, [results](results/2026-11-25-feed-without-token.txt)).
The feed lists only 0.5.0's full and delta packages, and no earlier release's.
We didn't call the API that lists the releases, which Velopack's `GithubSource` calls before it reads a feed.

**Changed:** Plates looks for its own updates ([decision 004](../../decisions/004-plates-updates-itself.md)).

**Affects:** [01-updates](01-updates.md).

## 2026-11-16: release-please can bump Cargo.lock's own versions, with a filter on each name's `value`

**Asked:** can release-please's release PR bump the workspace crates' versions in `Cargo.lock` along with `Cargo.toml`?

**Expected:** a `toml` extra-file for `Cargo.lock` with a JSONPath filter that picks each crate's `[[package]]` table by its name.

**Found:** the 0.2.0 release PR bumped `version` under `[workspace.package]` in `Cargo.toml` and left `Cargo.lock` at 0.1.0, so every build after it rewrote the lockfile until a commit brought it to 0.2.0 (seen on 2026-11-16).
release-please's TOML updater parses the file with its own parser, which wraps every value as `{start, end, value}` to remember where it sits, then runs the JSONPath with `jsonpath-plus` and writes each match back in place (read in release-please 17.11.2, `src/updaters/generic-toml.ts` and `src/util/toml-edit.ts`).
So a filter on `@.name==='plates'` matched nothing, and one on `@.name.value==='plates'` matched the crate's table.
With an entry per crate, `$.package[?(@.name.value==='plates')].version` and the same for `plates-core` and `plates-hook`, the updater changed those three `version` lines in a copy of `Cargo.lock`, from 0.2.0 to 0.3.0, and nothing else, in release-please 17.3.0, the oldest `release-please-action@v4` allows, and in 17.11.2 (measured on 2026-11-16).

**Changed:** `release-please-config.json` has those three entries.
When you add a crate under `src/`, which `members = ["src/*"]` makes a workspace member, add an entry for it too.
If a later release-please parses TOML without wrapping values, the filters match nothing and it logs "No entries modified", which a release PR that leaves `Cargo.lock` behind would show.

**Affects:** [releasing](releasing.md), whose version bump should name `Cargo.lock` too, once the notes another session has open in it are committed.
