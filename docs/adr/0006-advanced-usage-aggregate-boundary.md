# ADR-0006: Advanced Usage Exposes Only a Product-Level Aggregate Boundary

- Status: Accepted
- Date: 2026-08-24
- Amended: 2026-09-27, decisions 2 and 4 (the page syncs on its own when it opens and when the window returns)

> 2026-08-30: ADR-0031 supersedes this document's Advanced Mode navigation gate; the aggregation, privacy, and explicit-sync boundaries are unchanged.

## Context

The product spec §25 states that Advanced Mode may progressively add Usage; §83 allows keeping CC
Switch's Advanced Usage backend as long as the Beginner UI does not display it. Upstream already has
request logs, daily summaries, model pricing, and local session parsers for Claude Code, Codex,
Gemini CLI, OpenCode, Grok Build, and Pi, but the raw Usage command surface includes request IDs,
providers, models, status codes, error bodies, and paginated detail. Re-registering that command set
directly would expand the product IPC from a small Domain API back into the upstream backend and
expose ordinary pages to unnecessary sensitive metadata.

## Decision

1. Advanced Mode gains a top-level Usage entry; when Advanced Mode is turned off, that entry and any
   remembered Usage route immediately fall back to Settings.
2. The product registers only two Usage commands: read the aggregate snapshot for the last 30 local
   calendar days, and a local session sync. When the sync completes, the same response returns the
   new aggregate snapshot.
3. The Domain wire format contains only total requests, estimated USD cost, tokens, success rate,
   cache hit rate, daily trend, and aggregates by product `ToolId`. Request IDs, session IDs,
   providers, models, prompts, responses, error bodies, paths, commands, and credentials never cross
   the product boundary.
4. Opening the page shows the saved snapshot from the product database straight away and starts the
   local session sync in the background; the sync starts again when the window regains focus while
   the page is open (amended 2026-09-27, see below). Nothing scans at app startup or while the page
   is closed, and nothing touches the network. The sync reuses upstream's incremental parsing and
   deduplication; it only reads external CLI session files and writes to the product database,
   never modifying CLI configuration. When some sources fail, only the number of problems
   is returned to the UI; raw errors enter the local log only after redaction and truncation.
5. Cost is clearly labeled as an estimate based on local model pricing and does not pose as a vendor
   invoice; the page keeps the no-data, read-failed, retained-snapshot-with-failed-refresh, and
   partial-sync-success states.

## Consequences

### Follow-up: long first syncs (2026-09-20)

The product observes the shared TanStack mutation across page mounts. A pending
scan disables duplicate submissions even after navigation; the UI reports elapsed
time and explains that a large first import may take minutes, without inventing
a progress percentage or discarding the saved aggregate. Completion and failure
remain visible on return.

Claude session imports use bounded transactions (256 records) and a per-file
pricing cache, matching the batching approach already used for Codex. The final
batch commits its cursor with its records; insert/cursor/commit errors leave the
old cursor for safe replay through existing deduplication. Parsing, billable-token
rules, statistics projections and the database schema are unchanged. This does
not add scan cancellation or promise a fixed upper bound on first-import time.

### Amendment: sync without a button (2026-09-27)

The page had one job its button did not help with: a person opens Usage to see
current numbers, so asking them to press "Sync local usage" first added a step
whose only right answer was always yes, and a snapshot that was quietly out of
date until they did. The sync is incremental (files whose modification time has
not moved are skipped by the stored cursor, changed files resume from their line
offset), so running it on arrival costs little once the first import is done.

- The sync starts when the Usage tab opens and when the window returns
  (`focus`, or `visibilitychange` to visible) while it is open. It is skipped when
  one is already running or one started less than 30 seconds ago, so switching
  tabs or glancing away does not rescan back to back. The no-overlap rule is the
  same shared mutation as before; leaving the page still does not stop a scan.
- With a summary on screen, a running sync is one quiet line beside the title.
  With nothing to show yet, the page shows the existing elapsed-time status in
  place of the empty state, so a long first import is not mistaken for "no
  usage".
- A successful sync says nothing; the numbers change. A partial sync keeps its
  warning. A failed sync shows its error with a "Try again" button, which is the
  only manual trigger left.
- The saved-snapshot rule still holds: the overview is read once per session
  and replaced only by a sync result. A read still in flight when a sync lands is
  cancelled so it cannot overwrite the newer figures.
- Usage recorded by the local routing gateway was already written to the same
  table as it happened; it simply shows up on the next sync result.

- Positive: reuses CC Switch's mature statistics and session parsing while keeping the product API
  small, auditable, and secret-free.
- Positive: the Beginner Mode information architecture is unchanged; advanced users get a working
  local usage entry.
- Negative: the first version provides no per-request logs, arbitrary filtering, model/provider
  rankings, budget alerts, or invoice reconciliation; if those capabilities are ever opened, they
  must be added as separate Domain projections rather than by re-registering the upstream command set.
