# 004: Plates updates itself from its GitHub releases

An installed Plates looks for a newer release among the repository's GitHub releases through Velopack's `GithubSource`, downloads it, and installs it when you click its row in the rack's menu.
Decided by the user on 2026-11-25: once the repository was public, they asked to "build auto updates", with an entry in the rack's menu and "a teal dot or something when there's an update".
This reverses the part of [decision 003](003-releases-with-release-please-and-velopack.md) that has Plates ship without an update check of its own.

## Context

Under decision 003, you updated Plates by running a newer installer over the installed copy, though each release already carried Velopack's update packages and feeds.
A copy installed on 2026-11-19 was still 0.3.0 on 2026-11-25, after 0.4.0 and 0.5.0 had shipped.
It lacked 0.4.0's fix, "keep a desktop session's tile when the app moves or detaches it", and a session's tile split in two after a rewind (seen by the user, 2026-11-25).
The repository went public on 2026-11-25, and a release's update feed then downloaded without a token ([build log](../areas/build/log.md#2026-11-25-once-the-repository-is-public-a-releases-update-feed-downloads-without-a-token)).

## Options

1. **Keep updating by hand.**
   Nothing to build, but installed copies fall behind, as the 0.3.0 one did.
2. **Read the GitHub releases with Velopack's `GithubSource`.**
   The packages and feeds are there already, and each look spends at least one request against GitHub's API limit for an address without a token, as [01-updates](../areas/build/01-updates.md#looking-for-an-update) counts them.
3. **Publish the feeds somewhere else, such as GitHub Pages, and read them with Velopack's `HttpSource`.**
   No limit per address like the API's, at the cost of a second place every release has to reach.
   GitHub Pages has a soft bandwidth limit of 100GB a month and may rate-limit (documented, [GitHub Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits), as of 2026-11-25).

## Decision

Option 2, as [01-updates](../areas/build/01-updates.md) designs it, which we chose while building it: it needs no change to the Release workflow, and a look every 6 hours spends a few requests against the API's limit, where option 3 would give every release a second place to publish to.

## What it commits us to

- The repository stays public, since a private one hides its releases from a copy without a token.
- Every release keeps each channel's `releases.<channel>.json` and its packages, which the Release workflow attaches ([releasing](../areas/build/releasing.md)).
- A copy installed before this decision's release updates by hand once, to a release that has the update check.
