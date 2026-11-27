# Rack: pull requests

```text
Purpose:     How a session's tile shows the pull request its work is in, and where that pull request stands on GitHub.
Depends on:  02-tiles (02-tiles.md), for the tile, its hover card, its timer, and its brighten; 01-task-list (../tasks/01-task-list.md), for the server and the file that keep what Claude links; 01-claude-code-hooks (../hooks/01-claude-code-hooks.md), for the app's session files Plates reads and how it finds a program.
Decides:     Where Plates learns a session's pull requests, which one a tile shows, how Plates asks GitHub about it, what the chip and its pop-out show, and what a pull request that changes does to its tile.
```

A session tile shows the pull request its session works on as a chip at the end of its title: a pull request icon colored by where it stands, and its number, as in "#525" (user, 2026-11-27).
Resting the pointer on the chip opens a pop-out with the pull request's title and its state in words, and clicking the chip opens the pull request in your browser.
Plates learns which pull requests a session has from the Claude desktop app and from Claude, and asks GitHub where each stands through GitHub's `gh` command.

## Where a session's pull requests come from

- **The Claude desktop app.**
  The app's file for each session lists the session's pull requests in `prs`, in the order the app linked them (unverified), each with its `prNumber`, `repo` as `owner/name`, `url`, and `state` (measured on macOS 26.5.2, 2026-11-27, [hooks log](../hooks/log.md#2026-11-27-the-claude-desktop-apps-file-for-a-session-lists-its-pull-requests)).
  Plates reads the file once a second for the session's title already ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md)), so a pull request the app links shows on the tile within a second, and Claude needs no call for it.
  An entry with `dismissed` true stays off the tile.
  We take `dismissed` to mean you dismissed the pull request's card in the app (unverified, same entry).
- **Claude, with `pr_link`.**
  The `plates` server, which keeps the task lists, has a `pr_link(url)` tool, and its instructions tell Claude to call it when it opens a pull request for the session's work, or when you name the one it's for.
  It's there for a session in a terminal, which the app keeps no file for, and it does no harm in an app session, since Plates drops a link the app has too.
  The server keeps the pull request's URL in the session's task file, under `pulls`, and refuses any URL but `https://github.com/<owner>/<name>/pull/<number>`, dropping whatever follows the number, such as `/files`.

A session's pull requests are the app's, in the app's order, then the ones Claude linked, in the order it linked them, without repeats.

## Which one the tile shows

- The last open pull request in that order that isn't dismissed, with "+2" after its number when the session has two more open ones.
- Once none is open, the last merged or closed one, so a session whose work landed shows that it did.
- No chip when every pull request is dismissed, or the session has none.

Until GitHub answers, Plates counts a pull request as open if the app's file says `OPEN` and as merged otherwise, and one Claude linked as open.

## Asking GitHub

Plates asks `gh pr view <url> --json number,title,state,isDraft,reviewDecision,mergeable,statusCheckRollup` about each pull request a tile shows, one at a time, on a thread of its own rather than the rack's, since an ask took 820ms to 960ms (measured on macOS 26.5.2, 2026-11-27, [rack log](log.md#2026-11-27-gh-pr-view-answers-in-under-a-second-and-lists-a-pull-requests-checks-one-by-one)).

- Plates asks about an open pull request once a minute, and about a merged or closed one once, for its title.
- Plates stops asking about a pull request once no tile shows it.
- When an ask fails, as when `gh` isn't signed in, Plates asks again a minute later, and the tile keeps what it showed.

Plates looks for `gh` once, when a tile first shows a pull request: on `PATH`, then in `/opt/homebrew/bin` and `/usr/local/bin`, where Homebrew puts it on a Mac with Apple silicon and with Intel ([Homebrew's FAQ](https://docs.brew.sh/FAQ#why-should-i-install-homebrew-in-the-default-location)), then, on macOS, where an interactive login shell finds it, as Plates finds `claude` ([01-claude-code-hooks](../hooks/01-claude-code-hooks.md#installing)).
Without `gh`, the chip shows only what the app's file says.
`gh` asks GitHub with the account you signed it in to.

### Where a pull request stands

`statusCheckRollup` lists each of a pull request's checks, and its `__typename` says which kind it is: a `CheckRun` has a `status` and, once that's `COMPLETED`, a `conclusion`, and a `StatusContext`, a commit status, has a `state` ([GitHub's GraphQL reference](https://docs.github.com/en/graphql/reference/commits#union-statuscheckrollupcontext), as of 2026-11-27).
We've seen only check runs (measured on macOS 26.5.2, 2026-11-27, [rack log](log.md#2026-11-27-gh-pr-view-answers-in-under-a-second-and-lists-a-pull-requests-checks-one-by-one)).
Plates counts a check as:

- failed for `FAILURE`, `ERROR`, `TIMED_OUT`, `ACTION_REQUIRED`, or `STARTUP_FAILURE`
- running for a check run that isn't `COMPLETED`, or a commit status that's `PENDING` or `EXPECTED`
- passed for `SUCCESS`
- nothing for the rest, such as `SKIPPED`, `NEUTRAL`, or `CANCELLED`

Plates gives a pull request the first of these that holds: merged, closed, failed if any check failed, in conflict if `mergeable` is `CONFLICTING`, a draft, running if any check runs, passed if any passed, and otherwise open.

## The chip

- The chip sits 6px after the title.
  A title wraps to two lines only on a tile with no summary line ([02-tiles](02-tiles.md#layout)), and there the chip sits level with the first line.
  A title too long for its lines truncates before the chip.
  At the end of a long title, the chip sits 10px from the timer slot (user, 2026-11-27).
- Its fill is white at 7% opacity, and its number is in the secondary text color, so a chip on every tile doesn't pull the eye from the tiles' own states (user, 2026-11-27).
  Only its 11px icon takes a color:

  | Pull request | Icon |
  | --- | --- |
  | Open, with its checks passed, running, or none | green pull request icon |
  | Failed, or in conflict | red pull request icon |
  | Merged | purple merge icon |
  | Draft, or closed | pull request icon in the muted color |

  An open pull request whose checks are running, or that has none, is green like one whose checks passed, because gray marks a draft on GitHub (user, 2026-11-27).
- Clicking the chip opens the pull request in your browser and nothing else: the click stops at the chip, so it doesn't open the session, and the tile's card and the chip's pop-out stay closed until the pointer leaves the tile's text, as after a click on the tile ([02-tiles](02-tiles.md#the-hover-card)).
  Without that, a click just before the pop-out opened left the pop-out over the browser (seen by the user on macOS 26.5.2, 2026-11-27, [rack log](log.md#2026-11-27-a-click-on-the-pull-request-chip-just-before-its-pop-out-opened-left-the-pop-out-over-the-browser)).

## The pop-out

- When the hover card's wait ends with the pointer on the chip, the pop-out opens instead of the card ([02-tiles](02-tiles.md#the-hover-card)).
  It grows out of the chip as the card grows out of its tile, in a popup window of its own (user, 2026-11-27).
  It opens 6px under the chip from the chip's left edge, above the chip where the screen has no room below, and further left where it has none on the right.
- It shows, top to bottom:
  - the number and the repository, as "#525 · owner/name"
  - the title, once GitHub has said it
  - the state in words, in the icon's color, such as "2 checks failed · Changes requested", where a pull request in conflict says "Merge conflicts" (user, 2026-11-27)
  - "1 more open pull request" when the session has another open one
- It stays open while the pointer is on the chip, the gap, or the pop-out, and clicking it opens the pull request.

The pop-out is a window of its own because GPUI keeps a tooltip inside the window it belongs to, so over a chip near the tile's right end, a tooltip sat against the rack's left edge ([rack log](log.md#2026-11-27-gpuis-tooltip-cant-leave-the-racks-window-so-the-pull-request-chips-pop-out-is-a-window-of-its-own)).

## When a pull request changes

- When GitHub's answer changes where a pull request stands, as from running to failed, its chip brightens once: its fill washes up to the icon's color at 45% opacity and back over 1.4s, the length of a tile's brighten.
- When a pull request breaks, going to failed or in conflict from neither, each tile showing it starts its 5-minute timer, if its session isn't working and its tile has no timer running, run out, or set for later (user, 2026-11-27).
  The timer stops once the session starts working, as when Claude picks up the failure, so the tone rings only for a break nobody took up in 5 minutes ([02-tiles](02-tiles.md#the-timer)).
  Nothing else about a pull request rings, since Claude often watches and merges its own pull requests (user, 2026-11-27).
- Plates' first answer about a pull request after it starts changes nothing, so a pull request that was already failing when a tile first showed it neither brightens nor starts a timer.

## Known limits

- Plates knows only pull requests on github.com.
- Plates waits on `gh` with no time limit, so an ask that hangs holds up the asks behind it until it ends.

## Left open

- Whether `prs` runs in the order the app linked its pull requests, which the tile's pick depends on; a session that links an older pull request after a newer one would settle it.
- Whether `dismissed` marks a pull request whose card you dismissed in the app; dismissing one and reading the session's file would settle it.
- Whether the app's files on Windows hold `prs` as they do on macOS, and whether `gh` runs from Plates on Windows without a console window; running a session that opens a pull request on Windows would settle both.
- We haven't seen a chip brighten, or a break start a timer, since no pull request changed while we watched; a pull request whose checks fail while its session isn't working would settle both.
