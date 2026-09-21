# ADR-0032: Two-Tier Navigation, and a Reopen Notice After Switching Endpoints

- Status: Partially superseded
- Date: 2026-09-05
- Extends: ADR-0031 (progressive disclosure extended to the navigation layer); maintains ADR-0007 (proxy takeover remains an explicitly confirmed local routing capability)

> 2026-09-05: Decision 1 (three-segment navigation, the "More tools" collapsed segment, and local expand memory)
> has been superseded by ADR-0034 — the sidebar is flattened to five items, and local routing, usage, sessions,
> and the OpenClaw workspace become tabs of their parent pages. Decision 2 (reopen notice after switching) and
> decision 3 (direct live-config write, takeover still requires confirmation) remain in effect.

## Context

The product targets non-technical users and is positioned as a "software manager" for AI coding tools. After
ADR-0031 removed Advanced Mode, the sidebar became nine flat items: the professional surfaces inherited from CC
Switch — local routing, usage, sessions, OpenClaw workspace — appeared on the first screen alongside Home,
Software, and API Endpoints. Meanwhile, after successfully switching services on the API endpoints page, users got
no notice at all: the default switching method in both CC Switch and this product writes the tool's live
configuration directly (Claude Code's `settings.json` env, Codex's `config.toml` and `auth.json`, etc.), and
already-running tool processes do not automatically pick up the new endpoint. Codex and Grok Build read
configuration only at startup; Claude Code's official documentation says the `settings.json` env is hot-applied to
running sessions, but does not state whether the API address is re-read per request. The product copy only had
failure and retry, with no "switched, please reopen".

## Decision

1. Navigation splits into three segments: `primary` (Home, Software, API Endpoints, Skills / MCP), `more` (local
   routing, usage, sessions, OpenClaw; collapsible, collapsed by default, title `nav.moreTools`), and `footer`
   (Settings, pinned to the bottom). When the active route is in the `more` segment, that segment is considered
   expanded; the user's manual expand/collapse state is saved only in local `localStorage` and does not enter
   `ProductSettings`, the database, or the native API. This is not a mode switch: all pages are always reachable,
   only the first screen shows just the four everyday items.
2. After a successful switch or failover, uniformly notify two things: which service was switched to; and that
   the running instance of that tool will not automatically pick it up — it is only used after reopening. The
   notice carries an "Open now" action reusing the existing tool launch flow and `OpenToolModal`. No per-tool
   "takes effect via" field is introduced for this: the same sentence currently holds for all eight manageable
   tools, and an enum whose values are all identical goes stale more easily than one true sentence; if real
   testing later shows Claude Code can hot-switch, differentiate with a capability field then.
3. Provider switching continues to write live configuration directly by default and backs up before switching.
   Proxy takeover stays only on the "Local routing" page and enabling it requires user confirmation (ADR-0007
   unchanged). Rationale: direct writes are transparent to non-technical users, need no daemon, and tools keep
   working after the manager is closed; takeover can hot-switch, but it routes all AI traffic through this
   product's process, and when the process exits tools point at a dead local proxy.

## Consequences

- Positive: only everyday tasks remain on the first screen, while professional surfaces are still one click away;
  deep-linked or remembered routes are always visible and highlighted.
- Positive: after switching, users know what to do next and do not misjudge "the terminal is still using the old
  endpoint" as a failed switch.
- Negative: pages in the `more` segment take one more click; `Sidebar`'s props change from `items` to `sections`,
  and callers and tests must be updated in step.
- Negative: the notice treats all tools alike; if a tool can genuinely hot-switch, the notice is slightly
  redundant until the capability field is introduced.
