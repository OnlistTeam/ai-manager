# ADR-0034: Sidebar Flattened to Five Items; Professional Surfaces Become Tabs of Their Parent Pages

- Status: Partially superseded
- Date: 2026-09-05
- Supersedes: ADR-0032 decision 1 (two-tier navigation and the "More tools" collapsed segment); ADR-0032 decisions 2 and 3 unchanged
- Extends: ADR-0031 (features always available, complexity progressively disclosed at the point of use)

> 2026-09-08: Decision 1 (`APP_ROUTES` fixed at five items, local sessions as a tab of the Software page) has been
> superseded by ADR-0036 — sessions and local context/storage are combined into a sixth navigation item,
> "Local data", and the sidebar returns to six items. The rest of decisions 2–5 (local routing/usage still tabs of
> the endpoints page, the OpenClaw workspace tab, tab selection not entering ProductSettings, route
> animation/canvas palette consolidation) remain in effect, only with six routes instead of five.

## Context

ADR-0032 folded local routing, usage, sessions, and the OpenClaw workspace into a "More tools" segment collapsed by
default. In practice it brought three problems: the first screen still carried the mental load of nine
destinations (as soon as the collapsed segment expanded it was back to nine); the collapsed state was a second
source of truth existing only in local `localStorage`, and the rules between deep links, remembered routes, and
manual expansion needed separate tests and explanation; and each of the four professional surfaces had its own
route, scene, and background palette, yet every one of them semantically belongs to some everyday page — local
routing and usage are extensions of "API Endpoints", local sessions are the product of installed tools in
"Software", and the OpenClaw workspace is just the Skills / MCP configuration surface for the single tool
OpenClaw. The user's 2026-09-05 decision to "reduce cognitive load" requires the sidebar to keep only the five
everyday items.

## Decision

1. `APP_ROUTES` in `app/routes.ts` is fixed to the five items `home / tools / services / extensions / settings`.
   `NAV_ITEMS` is a flat table, with `settings` pinned to the bottom via `placement: "footer"`; `NavSection`,
   `NAV_SECTIONS`, `useSidebarSections`, and `aimanager.sidebar.sections` are deleted together, and `Sidebar`'s
   props return to `items / footerItems / activeId / onSelect`. Values such as `routing` remembered by old builds
   are rejected by `isAppRoute` and fall back to Home; no alias mapping.
2. The four merged surfaces become `ScopeTabs` tabs of their parent pages; each page keeps a single `h1`, and the
   original page titles inside tab panels are demoted to `h2`:
   - API Endpoints page: `services | routing | usage`, labels reuse `nav.services / nav.routing / nav.usage`;
   - Software page: `installed | sessions`, labels are the new `tools.tabs.installed` and `nav.sessions`;
   - Skills / MCP page: `workspace` (label `nav.workspace`) is appended after the existing Skills / MCP / Prompts
     tabs and appears only when OpenClaw is installed in the tool inventory. ADR-0031 removed Advanced Mode, so
     there is no longer a mode gate; `OpenClawWorkspacePage` itself no longer performs visibility checks such as
     "jump back to settings" — visibility is controlled entirely by the tab.
3. Tab selection is page-local UI state and does not enter `ProductSettings`: the local routing / usage /
   sessions / workspace tabs never write settings; `extensionKind` of Skills / MCP still records only the three
   extension kinds, and opening the workspace tab does not rewrite it.
4. The shell's one-shot intents are held uniformly by `app/useRouteIntents.ts`: `openTools(tab?)`,
   `openServices(tool?, tab?)`, `openExtensions(scope | "workspace")`; any ordinary navigation clears all
   intents. (Revised 2026-09-05: there was originally also `openSettingsCheck()`; after Task D moved Quick Check
   to Home, this "Home → Settings Quick Check" directed intent was deleted along with the Quick Check section of
   the Settings page.) The tool intent of the API Endpoints page is preserved across tabs until the endpoints tab
   has actually been shown once; after the user leaves the endpoints tab, the remembered scope takes effect again,
   preventing a stale Home intent from overriding the choice the user made within the page.
5. Route animations, loading placeholders, and canvas palettes are consolidated to five routes;
   `SpatialSceneModel` keeps only the five models that still have consumers, and the scene CSS, gallery samples,
   and PNGs of `routing / usage / sessions` are deleted together.

## Consequences

- Positive: the first screen and sidebar always have exactly five items, with no second collapsed state to
  explain; every professional surface sits right next to the everyday task it serves.
- Positive: `Sidebar` returns to the simplest controlled list; "footer pinned to the bottom" changes from a
  `section.id` check to an explicit flag, eliminating one item of technical debt.
- Negative: local routing, usage, sessions, and workspace no longer have their own loading placeholders,
  background palettes, or deep-link routes; reaching them requires going to the parent page and then clicking the
  tab. The shell preserves programmatic direct access through intents such as `openServices(tool, "routing")`.
- Negative: to avoid merge conflicts with the parallel native supply branch (which removes "migrate to npm"),
  `pages/tools/ToolsPage.tsx` renders the sessions tab via an early return inside the existing function, and the
  page title is rendered once in each branch; after that branch is merged back, it should be split into a shell +
  `InstalledToolsPanel`, recorded as technical debt.
- Negative: the visibility of the workspace tab depends on the single check `tool.id === "openclaw"`
  (`useExtensionsWorkspaceTab.ts`), because the workspace is an OpenClaw-exclusive capability rather than a
  generic capability; if a second tool with a workspace appears later, promote it to a `ToolCapabilities` field.
