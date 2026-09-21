# ADR-0038 — Task navigation and native file locations

Status: Accepted (user-directed, 2026-09-19)

Supersedes the navigation grouping in ADR-0034/0036; preserves ADR-0037's
single management entry for global prompts.

## Decision

- Primary destinations: Home, Software, API Endpoints, Skills, MCP, Global
  Prompts, Sessions. Settings stays pinned to the sidebar footer.
- Keep persisted route IDs `services`, `extensions`, and `data` for backwards
  compatibility; the last two now mean Skills and Sessions. Add `mcp` and
  `prompts`. Three routes share one extension engine with a fixed kind, not
  three copies of management logic. Saved legacy kind preferences cannot
  override the destination chosen in the sidebar.
- Local routing and usage remain secondary API Endpoints tabs. Configuration,
  memory, and per-tool storage move to Software → software details. OpenClaw's
  workspace remains accessible there. Sessions no longer scans storage for
  every installed tool when opening the page.
- Show installed software even with no items. Capability gates, not nonempty
  lists, determine management availability. Do not use provider capability to
  label session availability.
- MCP and Global Prompts expose a scope-level file-manager button: these
  native files are shared by that software's entries. Inactive prompt library
  rows have no separate live files. Sessions retain per-session reveal, with
  a visible text button in the thread and a compact icon in the list.
- Reveal accepts only scope/kind or an opaque session reference. Native code
  resolves vendor paths through the compatibility facade. Missing files open
  their nearest existing parent directory; revealing never creates or writes
  configuration. No dependency or database migration is needed.
- Software details belong to the software row's action group. Inside the
  modal, use one flat resource list without a duplicate tool header or nested
  path cards; refresh shares a row with storage totals. Keep managed prompt
  handoff, native-owned resource IDs, missing-file behavior, and lower-bound
  storage measurements intact. Refresh errors remain visible beside cached data.

## Verification

Coverage includes route order and fixed kinds, software details and prompt
handoff, unchanged session reveal payloads, scope-only MCP/prompt reveal,
native vendor path resolution, unsupported scopes, and missing-file fallback.
