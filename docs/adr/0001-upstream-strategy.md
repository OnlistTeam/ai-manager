# ADR-0001: Upstream Strategy — Forking From and Syncing With CC Switch as the Foundation

- Status: Accepted
- Date: 2026-08-18

> 2026-08-30: ADR-0031 supersedes this document's visibility policy of hiding already-delivered pages behind Advanced Mode; the selective upstream reuse and sync strategy is unchanged.

## Context

This product (AI Manager) aims to give non-technical users a way to install, manage, configure, and maintain AI coding tools such as Claude Code, Codex, and OpenCode. CC Switch (https://github.com/farion1231/cc-switch, MIT, Jason Young) has already implemented and validated most of the underlying capabilities: multi-tool configuration management, provider switching, MCP/Skills/Prompts, Usage, Backup, the SQLite data layer, and cross-platform logic. The phase-one goal is to deliver a beginner-friendly experience at the lowest development cost, not to rewrite these foundations.

## Decision

1. **Start from a fork**: the repository is based on cc-switch v3.19.2 (commit fd14f9c4); the `upstream` remote points to CC Switch and `origin` points to this project's repository.
2. **No wholesale merges**: the UI will diverge completely; `upstream/main` is never merged directly.
3. **Sync by cherry-pick**: when upstream ships a new version, create a temporary `sync/ccswitch-x.y.z` branch → read the changelog → cherry-pick only the commits related to the Rust backend, configuration parsing, and tool support → run the full test suite → merge into main.
4. **Compatibility-layer isolation**: create `src-tauri/src/compat/ccswitch/` as the only layer that touches upstream internals; it exposes only this product's Domain Model (Tool/Provider/Extension/HealthItem/Operation). The frontend and the Application layer never see CCSwitch\* internal types.
5. **License**: keep the root MIT LICENSE and the copyright headers of upstream files; add THIRD_PARTY_NOTICES.md to record CC Switch attribution. Branding, icons, name, and the new UI use this product's own brand.

## Alternatives

- **A. Build from scratch**: enormous backend effort (configuration parsing, multi-tool adaptation, and cross-platform details would all have to be relearned the hard way), violating the "70% reuse" principle. Rejected.
- **B. Depend on CC Switch as a library**: upstream is not designed as a library and has no stable public API; a Tauri application is also unsuitable for reuse as a crate. Rejected.
- **C. Keep following the upstream UI (reskin)**: cannot deliver the Beginner Mode information-architecture rework, and the spec explicitly forbids a reskin. Rejected.
- **D. Fork + compatibility layer + cherry-pick (this decision)**: maximizes reuse while the boundary layer preserves the freedom to restructure.

## Consequences

- Positive: underlying capabilities are available immediately; upstream fixes can be absorbed selectively; the UI evolves freely; license compliance is clear.
- Negative: upstream internal refactors may increase cherry-pick conflicts, which the compatibility layer must absorb; sync discipline must be maintained (see docs/ARCHITECTURE.md §6).
- Constraint: stable upstream features (Proxy/Failover/Sessions, etc.) keep their backend code, are hidden by default in the Beginner UI, and are opened up progressively through Advanced Mode.
