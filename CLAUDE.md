# Plates

Plates is an always-on-top overview of the Claude Code sessions and other work its user is juggling, on Windows and macOS.
[record/areas/product/01-aims.md](record/areas/product/01-aims.md) says who it's for and what it won't do.

## Layout

- `src/` holds the product's code, as the members of the Cargo workspace at the root, and `tools/` holds what builds, checks, and tests it.
- `docs/` is reserved for documentation people write for other people.
  Create or edit it only when asked, and keep the author's voice.
- `record/` holds the documents agents write.
  [record/index.md](record/index.md) is the map.
  Keep its top level to `index.md`, `decisions/`, and `areas/`, and give each part of Plates a folder under `areas/`.
- `experiments/` holds probes and prototypes, and their raw results, in `experiments/areas/<area>/`, the same path the area has under `record/`.
  A finding worth keeping goes in the area's log in `record/areas/<area>/`.
- `.claude/rules/writing.md` is how everything agents write is written, and Claude Code loads it into every session.
- `tools/prose-lint.mjs` makes the mechanical checks from writing.md, on Markdown and on the comments in code.
  A hook runs it after every edit, and its findings come back to you as context.
- `.claude/agents/doc-reviewer.md` makes the judgment checks the lint can't, and `.claude/agents/code-reviewer.md` checks product code against its design and the rules below.
  If an agent in `.claude/agents/` isn't available as an agent type, run a general-purpose agent told to read that definition and act as that agent, in the background if its front matter says `background: true`.
  That happens when `.claude/agents/` didn't exist as the session started ([sub-agents](https://code.claude.com/docs/en/sub-agents.md), as of 2026-11-15).

## Working rules

**Product code never imports from `experiments/`.**
A prototype that works reaches the product by being rebuilt in `src/`, so a prototype's shortcuts never ship.
An experiment needs no design doc: run it, and write down only what's worth keeping.

**Batch everything that needs the Mac into one deliberate session.**
Only the user can run Plates on macOS, so write each Mac test to answer as much as it can in one run, and print a report that stands on its own.

**Group a folder's files by what they're for once it grows past about a dozen.**
Name each group's folder, and each file, for what it holds, and in Rust put `dock.rs` beside `dock/` rather than a `dock/mod.rs`.
In Dual Input, another of our repositories, a folder that reached 17 flat files hid which of them belonged together, while a folder made for files that won't grow only adds nesting.

**Never delete as a side effect.**
Deleting a file, or running `git checkout`, `restore`, `stash`, or `reset` on a path, is its own proposal: ask first, and say why.
Uncommitted work these discard can't be recovered, so anything else that discards it, such as `git clean` or `Remove-Item`, is a proposal too.

**Commit only the changes you made.**
Other sessions edit files in this working tree too, so `git add -A` would put their unfinished work in your commit.
Before you commit or rebase, run `git status --short`.
Stage by explicit path, never with `git add -A` or `git add .`, and tell the user which changes you left out.
If a change you didn't make is in a file you're about to edit, stop and ask.

**Never create a merge commit.**
A merge commit turns the history into a graph, which `git log` and `git bisect` read worse than a line.
To bring a branch up to date, rebase it onto `origin/main`, and push a rebased branch with `--force-with-lease`.
If the tree holds changes you didn't make, stop and ask before rebasing, because git won't rebase over them without a stash.
Don't run `git merge`, or `git pull` without `--rebase`.
The Claude desktop app's `sync_with_base_branch` tool merges too, so don't use it.

**Ask before a code review.**
After writing or substantially changing product code, ask the user whether to run the `code-reviewer` agent, because the user decides which changes are worth a review.
Experiments get none.

**Nothing personal in the tree.**
The repository can go public, and everything in the tree goes with it.
No file in the tree may carry a username, a home folder path, a machine name, or an email address.
Before committing, run this in Git Bash or a macOS terminal.
No output means it found none of your account name, machine name, or email address:

```bash
git grep -inE --untracked "$(whoami)|$(hostname)|$(git config user.email)"
```

It can't see inside images or compressed files, so check screenshots and captures by eye before adding them.
