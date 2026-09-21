# ADR-0036: Sessions and Local Context/Storage Combined into a Sixth Navigation Item, "Local Data"

- Status: Accepted
- Date: 2026-09-08
- Supersedes: ADR-0034 decision 1 (`APP_ROUTES` fixed at five items)
- Amends: ADR-0035 decision 4 (last sentence, see below)

## Context

When ADR-0034 (2026-09-05) flattened the sidebar from nine items to five, the rationale given for where "local
sessions" belongs was "local sessions are the product of installed tools in Software". That rationale does not
hold: sessions are **the user's data**, while Software is **the install state of software**; someone looking for
"that project I chatted with Claude about last time" will not click "Software".

At the same time, the "local context and storage" panel (`services.runtime.*`, a collapsed card hanging at the
bottom of the API Endpoints page) was misplaced from the start: the four kinds of resources it shows — global
prompt, memory, project session data, user profile — none of them is the business of an API endpoint. The only
item genuinely belonging to endpoints, "runtime configuration", had already been made into a separate "Open config
file" button on the card in an earlier change (`ServicesOpenConfigAction`), and the panel explicitly
`filter(kind !== "configuration")`.

There is also hard evidence of overlap between the two: `session_count` in
`compat/ccswitch/provider_runtime/storage.rs` directly calls `SessionStore::list(None, Some(tool))`, the same
`SessionStore` as the sessions page's `app_sessions_list`.

## Decision

1. `APP_ROUTES` in `app/routes.ts` expands from five items to six: `home / tools / services / extensions / data /
settings`. The new route id is `data`, not `sessions` — `isAppRoute("sessions")` must continue to return
   `false`, so that `"sessions"` remembered by old builds (the collapsed-segment/tab value of the ADR-0032/0034
   era) still correctly falls back to Home; no alias mapping. `NAV_ITEMS` inserts
   `{ id: "data", labelKey: "nav.data", icon: HardDrive }` after `extensions` and before `settings`; the order
   determines both the sidebar order and the direction of route animations.
2. Create `src/pages/data/DataPage.tsx` with four layers from top to bottom: a cross-tool summary strip
   (`MetricStrip`, reading the `providerRuntimeContextQueryOptions` cache already warmed at startup, at zero
   cost) → the local sessions area (renders the existing `SessionsPage` directly without changing a line; its
   current `SectionHeader as="h2"` is exactly the demotion ADR-0034 made to fit it into a tab, which fits
   perfectly under the `h1` shell of the `data` page) → the local context and storage area (tool pills +
   always-open panel, the layout the user decided on in this session; see known trade-offs below).
3. `ProviderRuntimeContextPanel` / `ProviderRuntimeResourceItem` / `ProviderRuntimeStorageSummary` move from
   `pages/services/` to `pages/data/`. Along with the move:
   - Remove the `<details>` collapse from `ProviderRuntimeContextPanel` — it existed only because the card was an
     appendix at the bottom of the endpoints page; on a page devoted entirely to it, collapsing by default works
     against the user.
   - Remove the third cell, "Saved conversations" (`storage.sessionCount`), from
     `ProviderRuntimeStorageSummary`. It answers the same question as the session list, but the two paths have
     different truncation rules (the backend sets 0 and marks `measurement_limited` on failure; `SessionList` has
     its own `limited`/`totalCount`), so placing them side by side would be self-contradictory. The four-language
     key `services.runtime.storage.conversations` is deleted accordingly.
   - `ServicesOpenConfigAction` stays on the endpoints page untouched: the config file has exactly one entry, and
     that entry is on the endpoints page — a decision from the earlier change; `filter(kind !== "configuration")`
     is preserved verbatim, with the guard test in `tests/pages/services/ServicesPage.test.tsx`.
   - The tool pills reuse `features/provider-management/ToolScopeTabs` (already filtered by
     `canManageProvider`), and the caller adds one more condition, `status !== "notInstalled"`, to avoid listing
     a tool whose every path reads "not created". The selected state uses page-local `useState`, initialized from
     `useProductSettings().data?.toolScope` and never written back — `useServiceScope` is not reused, because
     that hook writes the selection into `ProductSettings.toolScope`, and changing tools on the Local data page
     would also change the scope of the API Endpoints page, violating ADR-0034 decision 3, "tab/pill selection is
     page-local UI state and does not enter ProductSettings".
4. No changes on the `src-tauri` side: the wire contracts of the two commands `app_sessions_list` /
   `app_provider_runtime_context` are unchanged; the frontend merely moved where they are consumed from the
   "Software tab" and "endpoints page collapsed card" to the new page.
5. **The i18n namespace is deliberately not moved**: `services.runtime.*` stays where it is and is not renamed
   along with the component move. Although the project convention is "page directory = top-level namespace", this
   is recorded as a known inconsistency:
   - Both guard points (`PRODUCT_NAMESPACES` in `tests/i18n/messageKeys.test.ts` and
     `tests/config/productProvenanceLocales.test.ts`) are additive allowlists; no assertion checks which
     namespace a key lives in — the test pass rate would be exactly the same after the move, buying no
     enforcement.
   - `routing` / `usage` / `sessions` each kept their top-level namespace after ADR-0034 demoted them to tabs; a
     namespace has never equaled a location, and the codebase has already accepted this.
   - The cost is 30 keys × 4 languages + ~25 call sites, and the components make heavy use of
     template-concatenated keys (e.g. `` `services.runtime.action.${resource.action}` ``); a missed prefix is
     not a TS error and only surfaces as a bare key at runtime, while the i18n tests only check that the four
     languages agree with each other — "consistently all wrong" still passes green.
   - While the keys stay under `services`, they are automatically protected by `productProvenanceLocales`; after
     a move, one must remember to add them to the hard-coded array, and forgetting means the guard silently
     stops working.
     Changing this later is a pure rename with zero behavior change, as a separate commit.
6. `SpatialSceneModel` keeps only the five models that still have consumers (`environment / tools / services /
extensions / settings`) — this decision does **not** add a sixth model; the `data` route has only a canvas
   palette (natural moss green, hue 96, saturation reduced to 42%) and the `"section"` placeholder shape of
   `RouteLoadingFallback`; the machine proof is that the `SHIPPED_MODELS` array in
   `tests/config/spatialPageVisuals.test.ts` stays at five items.
7. `pages/tools/ToolsPage.tsx` removes its internal tabs: `useToolsTab.ts` and `ToolsTabs.tsx` are deleted
   wholesale, and the page returns to a single `return` and a single `h1` (the file previously rendered the
   sessions tab via an early return to avoid conflicts with a parallel branch — the technical debt recorded in
   item 4 of the ADR-0034 "Consequences" — which is paid off here; the `InstalledToolsPanel` split remains
   separate technical debt and is not handled). The entire `RouteIntents.toolsTab` chain (never passed as an
   argument in production code) is deleted with it.

## Known Trade-offs

The page has two "choose tool" controls: the dropdown in the local sessions area (a filter, including "All tools",
from the existing `SessionsToolbar`) and the pills in the storage area (a focus, necessarily single-select). The
two tool sets are in fact identical (the eight tools that support session/service management), so technically
they could be merged into one page-level selector, at the cost of making `SessionsPage` controlled and moving its
tool dropdown to the top of the page. This change keeps both controls per the layout the user chose this time,
distinguishing their roles by copy (filter sessions vs. view which tool's storage). If it feels in the way on a
real device, the merge path is the one above.

## Consequences

- Positive: looking for "last session" and looking at "a tool's local files" are in the same destination; no need
  to guess whether they hang under "Software" or "API Endpoints".
- Positive: the collapsed card at the bottom of the endpoints page disappears, and the page goes back to talking
  only about endpoints; that card no longer needs to compete semantically with the "runtime configuration" button.
- Negative: the `services.runtime.*` namespace is inconsistent with where it now lives (`pages/data/`), a
  deliberately recorded technical debt (see decision 5); the next change to this copy should migrate it along the
  way.
- Negative: the sessions filter dropdown and the storage pills coexist, so the page has two semantically similar
  "choose tool" controls (see known trade-offs).

## Incidental Fix

`SessionsToolbar`'s filter dropdown spread all 10 `TOOL_IDS` into its options, but the backend `session.rs` only
recognizes 8 — Kimi Code and DeepSeek DSH have no session source, and `sessions.tool.*` only has 8 copy keys, so
selecting them inevitably showed the bare keys `sessions.tool.kimi-code` / `sessions.tool.deepseek-dsh` and
inevitably returned empty results. This change only adds the four-language translations for these two keys (no
more bare keys; an empty result is the truth); the correct fix is to add a `canBrowseSessions` capability and
filter the dropdown options by capability, which is a backend schema change, recorded as technical debt and not
done here.
