# ADR-0006: Advanced Usage Exposes Only a Product-Level Aggregate Boundary

- Status: Accepted
- Date: 2026-08-24

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
   calendar days, and a user-triggered local session sync. When the sync completes, the same response
   returns the new aggregate snapshot.
3. The Domain wire format contains only total requests, estimated USD cost, tokens, success rate,
   cache hit rate, daily trend, and aggregates by product `ToolId`. Request IDs, session IDs,
   providers, models, prompts, responses, error bodies, paths, commands, and credentials never cross
   the product boundary.
4. Opening the page for the first time only reads the product database; it does not scan sessions or
   touch the network. Only after the user clicks sync is upstream's incremental parsing and
   deduplication reused; that action only reads external CLI session files and writes to the product
   database, never modifying CLI configuration. When some sources fail, only the number of problems
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
remain visible on return. Opening the page still does not initiate a scan.

Claude session imports use bounded transactions (256 records) and a per-file
pricing cache, matching the batching approach already used for Codex. The final
batch commits its cursor with its records; insert/cursor/commit errors leave the
old cursor for safe replay through existing deduplication. Parsing, billable-token
rules, statistics projections and the database schema are unchanged. This does
not add scan cancellation or promise a fixed upper bound on first-import time.

- Positive: reuses CC Switch's mature statistics and session parsing while keeping the product API
  small, auditable, and secret-free.
- Positive: the Beginner Mode information architecture is unchanged; advanced users get a working
  local usage entry.
- Negative: the first version provides no per-request logs, arbitrary filtering, model/provider
  rankings, budget alerts, or invoice reconciliation; if those capabilities are ever opened, they
  must be added as separate Domain projections rather than by re-registering the upstream command set.
