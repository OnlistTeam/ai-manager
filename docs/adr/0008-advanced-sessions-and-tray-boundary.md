# ADR-0008: Sessions and Tray Switching Open Only an Opaque Product Boundary

> 2026-08-30: ADR-0031 supersedes this document's Advanced Mode visibility gate; the opaque-reference, path-isolation, and safe-resume boundaries are unchanged.
>
> 2026-09-10: the boundary is narrowed from "the renderer cannot see location information" to "the renderer cannot supply location information".
> The list still contains only opaque references; but once the user explicitly opens a session, the `thread` carries that session's working
> directory and a single manually runnable resume command. See "The output side is not the defense line" below.

## Context

CC Switch already has eight-tool session scanners, message parsers, and tray provider switching. The
old Session IPC handed `sourcePath`, `sessionId`, and `resumeCommand` to the renderer and let the
renderer pass arbitrary command strings to the shell; the old tray also hosted usage, website,
lightweight mode, and several other entries that have not yet entered the AI Manager product model.

Re-registering the old commands directly or copying the old tray would bypass the Phase 7 Domain,
Advanced Mode, and local privacy boundaries.

## Decision

1. Sessions is a top-level Advanced Mode page; all three native commands re-validate Advanced Mode
   rather than relying solely on the frontend hiding navigation.
2. The eight upstream scanners are reused, but are projected into the product Domain immediately in
   `compat/ccswitch/session`: the list returns only a SHA-256 opaque reference, ToolId,
   length-limited title/summary, project basename, timestamps, and a resumable flag.
3. Messages are loaded only after the user selects a session. Single messages, total bytes, message
   count, and list count all have fixed caps; control characters are stripped and messages are
   rendered as plain text. The same `thread` payload carries the session's working directory and a
   single manual resume command, both for display only: the backend assembles the command from the
   same allowlist as the argv, and a command string handed back by the renderer is never executed.
4. On resume, the backend rescans and resolves the opaque reference, rebuilding argv from the
   per-ToolId allowlist. The renderer can never supply an executable path, working directory, raw
   session path, or command string. The macOS, Windows, and WSL platform bridges all pass argv item
   by item.
5. Tray quick-switching appears only in Advanced Mode and iterates only the provider lists allowed by
   the product `ToolCapabilities`. Menu events carry only the ToolId and the full SHA-256 of the
   provider ID; on click the list is re-read and a unique match is required; stale or ambiguous
   references are always rejected.
6. The tray and the main UI both call the same `ProviderDirectory::switch`, sharing the AppState
   provider mutation lock. On success the tray is rebuilt and a `provider://changed` event containing
   only the ToolId is emitted to refresh renderer queries.
7. The upstream tray's usage, website, lightweight mode, cloud sync, and other unreviewed entries are
   not restored.

## The Output Side Is Not the Defense Line

The original text treated "the renderer cannot _supply_ paths and commands" and "the renderer cannot
_see_ session IDs and paths" as one and the same boundary. The second half does not hold up:

- An attacker who can already compromise the renderer could simply call `app_session_thread` and read
  the entire session body, which contains the user's private prompts and plenty of absolute paths,
  far more sensitive than a session ID. Blocking the ID does not change the attacker's capabilities.
- The cost, however, lands on the user. After the terminal handoff, failures happen inside the
  terminal process, and AI Manager cannot see that error line (one observed case: the session was held
  by a `codex app-server` that had been alive for a day, codex reported `already has an active writer`,
  and the terminal flashed and closed). At that point the user knows neither which session nor which
  directory is involved, and has no way to investigate.

Therefore all input-side constraints are kept — resume accepts only opaque references, the working
directory and argv are resolved and rebuilt by the backend itself, and the platform bridges pass argv
item by item — while the output-side restriction is relaxed only after the user explicitly opens a
session.

## Consequences

- The original mature local format parsing and switch transaction continue to be reused, keeping the
  cost of following upstream format changes low.
- The renderer can display the session body the user explicitly requested, but cannot obtain raw
  location information usable for arbitrary file reads or command execution.
- The first slice does not include session deletion, batch operations, custom titles, virtual
  scrolling, or the OpenClaw workspace; each of those needs its own deletion confirmation, recovery
  strategy, and scale validation and cannot be exposed casually through the old IPC.
