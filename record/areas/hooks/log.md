# Hooks: log

What we found about Claude Code's hooks and the Claude desktop app, newest entry first.

## 2026-11-15: A desktop session's transcript marks a typed prompt with origin.kind human, after a system reminder

**Asked:** how does the transcript of a session in the Claude desktop app hold the prompts you type, so the transcript reader can name a session Plates first saw after its first prompt?

**Expected:** JSON Lines, with each typed prompt as a record whose `message.content` is the prompt's text.

**Found:** we read the structure of one of this repository's session transcripts, the record types and keys only, on the Claude desktop app's Claude Code (measured on Windows 11 25H2, 2026-11-15).

- The transcript is JSON Lines, with records of `type` `queue-operation`, `user`, `attachment`, `last-prompt`, `file-history-snapshot`, and others.
- A prompt a person typed is a `user` record with `origin.kind` `human` and `isSidechain` false.
  Its `message.content` is either a string or a list of `text` parts, and the first prompt's list started with a `<system-reminder>` part of context before the typed text.
- A tool's result is also a `user` record, with a `tool_result` part and no `origin`, and a subagent's records carry `isSidechain` true.
- `UserPromptSubmit` carries the typed text as `prompt` (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15), so the transcript is needed only for a session whose first prompt came before Plates was listening.

**Changed:** the transcript reader takes a session's first prompt from the first `user` record with `origin.kind` `human`, outside a subagent, leaving out `<system-reminder>` parts.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md#titles-and-summaries).

## 2026-11-15: Claude Code documents the final message on Stop, failure events, and the session title on every prompt

**Asked:** does the hooks design handle every event that changes what a session is doing, and does it read each value from where the documentation puts it?

**Expected:** the seven events the design listed, covering every change of state.

**Found:** two more reads of Claude Code's documentation, for two reviews of the hooks design.
Nothing here has been measured yet.

- `Stop` carries `last_assistant_message`, the text of Claude's final response, and the page warns that the transcript file may not hold that message yet when `Stop` fires (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
- `Stop` doesn't fire when you interrupt Claude, and canceling a running tool fires no `PostToolUseFailure`, so an interrupted turn fires no event that ends it (documented, same page).
- When an API error ends a turn, `StopFailure` fires in place of `Stop`, with an `error` such as `rate_limit` and, optionally, the error's text in `last_assistant_message` (documented, same page).
- `PostToolUseFailure` fires after a tool that started fails.
  A permission you deny by hand fires no event of its own: `PostToolUseFailure` doesn't fire for a permission denial, and `PermissionDenied` fires only for auto mode's denials (documented, same page).
- `PermissionRequest` fires when Claude Code is about to ask you for permission to use a tool (documented, same page).
  In a session hosted through the Agent SDK's `canUseTool` callback, as the Claude desktop app hosts Claude Code, `Notification` with `permission_prompt` arrives about 6s after Claude asks, and not at all if you or a `PermissionRequest` hook answer sooner (documented, same page).
- `SessionStart` fires with a `source` of `startup` for a new session, and also of `resume`, `clear`, `compact`, or `fork` (documented, same page).
- `SessionEnd` fires when a session exits, and also on `/clear` and on switching sessions with `/resume`, with a `reason` of `clear` or `resume` (documented, same page).
- `UserPromptSubmit` fires for a prompt you send, and also when a scheduled task fires, when a background subagent reports back, and when another session sends a message to the conversation (documented, same page).
- `session_title` is present whenever the session has a custom title, whether from `--name`, `/rename`, a hook, or the Agent SDK's `renameSession()`, and absent for a title the app generates.
  `UserPromptSubmit` carries it too (documented, same page).
- An `http` hook whose connection fails, or whose response isn't a 2xx status, ends in a non-blocking error, and the action goes ahead (documented, same page).
- An `http` hook's default timeout is 600s, except 30s on `UserPromptSubmit`.
  A session's `SessionEnd` hooks share a budget of 1.5s, which Claude Code raises to the longest per-hook `timeout` in the settings files, up to 60s (documented, same page).
- `claude --desktop --resume <session-id>` opens a CLI session in the Claude desktop app by its ID, a session's name doesn't work in place of the ID, and it doesn't move a session that's open in another terminal or still running in the background (documented, [desktop](https://code.claude.com/docs/en/desktop.md), as of 2026-11-15).

**Changed:**

- The your-turn summary comes from `last_assistant_message`.
- `PostToolUseFailure` and `StopFailure` move tiles as `PostToolUse` and `Stop` do.
- `PermissionRequest` moves a tile to needs approval, without the 6s wait of `permission_prompt`.
- `SessionStart` brings back an ended tile and leaves any other tile it already shows as it is.
- An event from a session Plates doesn't show adds its tile.
- The receiver answers each request before acting on it.
- Step 6 tries `claude --desktop --resume` on a desktop session.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md) and [the plan](../product/plan.md).

## 2026-11-15: Claude Code documents hooks in desktop sessions and an HTTP hook type, but no link that opens one desktop session

**Asked:** do the hooks in Claude Code's user settings file fire for sessions in the Claude desktop app, what does each event carry, can a hook reach Plates without a helper program, and is there a link that opens one desktop session?

**Expected:** a small helper program for each hook to run, which would forward the event to Plates, and possibly no hooks at all in desktop sessions.

**Found:** an agent read Claude Code's documentation for us, and we read the Windows registry on our PC.
Apart from that look at the registry, nothing here has been measured yet.

- "Claude Code fires the same hook events wherever it runs: sessions in the terminal, IDE extensions, the Desktop app, and cloud sessions" (documented, [hooks](https://code.claude.com/docs/en/hooks.md), as of 2026-11-15).
  Cloud sessions don't read the settings file on our machine (documented, same page).
- Every event carries `session_id`, `transcript_path`, `cwd`, `hook_event_name`, and `permission_mode` (documented, same page).
- `Notification` adds `message`, an optional `title`, and `notification_type`: `permission_prompt` once Claude has waited about 6s for an approval, and `idle_prompt` about 60s after Claude finishes, if nothing has been typed and no background agent runs (documented, same page).
  A `Notification` hook can't block.
- `SessionStart` carries `session_title` when the session has a custom title, such as one set with `--name` or `/rename`, and leaves it out for a title the app generates (documented, same page).
- A hook can be of type `http`, which POSTs the event's JSON to a URL, with a default timeout of 600s (documented, same page).
- The documented deep link, `claude-cli://open`, opens the terminal CLI and takes `q`, `cwd`, and `repo`, with no session ID (documented, [deep links](https://code.claude.com/docs/en/deep-links.md)).
  The desktop app's one documented `claude://` link, `claude://code/new`, opens its new-session page, optionally on an SSH connection, and takes no session ID (documented, [desktop](https://code.claude.com/docs/en/desktop.md)).
- On our Windows PC, the `claude` URL protocol is registered to the Claude desktop app's executable, under `HKCU\Software\Classes\claude` (measured on Windows 11 25H2, 2026-11-15, [results/2026-11-15-claude-protocol.txt](results/2026-11-15-claude-protocol.txt)).

**Changed:** the receiver takes `http` hooks directly, with no helper program; a session tile's title falls back to the session's first prompt; and clicking a session tile brings the Claude app forward until a link to one session turns up.

**Affects:** [01-claude-code-hooks](01-claude-code-hooks.md), [01-aims](../product/01-aims.md), and [02-tiles](../rack/02-tiles.md).
