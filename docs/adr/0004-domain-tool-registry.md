# ADR-0004: Independent Domain Tool Registry; AppType Demoted to an Upstream-Internal Identifier

- Status: Superseded in part by ADR-0005
- Date: 2026-08-18

## Context

Upstream threads a nine-value `AppType` enum (claude/codex/opencode/gemini/openclaw/hermes/pi/grokbuild/claude-desktop) through both frontend and backend. This product's Beginner Mode exposes only Claude Code, Codex, and OpenCode (P0) plus Gemini CLI (P0.5); the product spec §11 requires ToolAdapter + ToolCapabilities to drive the UI, and §4 forbids the frontend from touching upstream types.

## Decision

1. **Establish an independent `ToolId` registry in the Domain layer**: `claude-code`, `codex`, `opencode`, `gemini-cli` (P0.5). The strings are stable and go into the DB and i18n keys.
2. **Demote `AppType` to an upstream-internal identifier** that appears only inside the `compat/ccswitch/` facade, which maintains a bidirectional `ToolId ↔ AppType` mapping. Long-tail tools without a mapping (openclaw/hermes/pi/grokbuild/claude-desktop) do not enter the Domain registry; their upstream commands are kept but are invisible in the Beginner UI.
3. **Each ToolId corresponds to one `ToolAdapter` implementation plus a static `ToolCapabilities` declaration**; the UI and Application layer always branch on capabilities, never on hard-coded ToolId special cases.

## Alternatives

- **ToolId = a subset of AppType, passed straight through**: leaks the upstream enum to the frontend, so any upstream addition or removal breaks the product layer; non-CLI entries such as claude-desktop get mixed in. Rejected.

## Consequences

- Positive: the product vocabulary is under our own control; upstream enum changes are absorbed by the facade; the capability-driven design means adding tools in P0.5/P1 only requires registering a new Adapter.
- Negative: one more mapping layer to maintain; importing CC Switch data requires filtering long-tail tool data according to the mapping.

> 2026-08-23: after the product scope expanded, the size of the CLI registry and the long-tail policy are superseded by ADR-0005;
> the principles that `ToolId` stays isolated from the upstream `AppType` and that capabilities drive the UI remain in force.
