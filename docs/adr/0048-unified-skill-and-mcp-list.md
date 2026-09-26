# ADR-0048: One list for Skills and MCP, with a switch per app

- Status: accepted
- Date: 2026-09-26
- Supersedes, for Skills and MCP only: the "software selector" in ADR-0038
  (one tab per installed tool or desktop app) and ADR-0045's shared-file row
  above the MCP list. Global Prompts keep both.

## Context

Skills and MCP were listed one app at a time. Turning one MCP server on for
Claude Code and Codex meant two tabs and two switches, and nothing on either
tab said the other app had it too. Each tab also split its list into "Found
in this tool" (read-only) and "Managed by AI Manager". The native command to
bring a found item under management existed but the page never offered it,
so found items could be looked at and nothing else.

Both kinds are one global record with a flag per app upstream (`McpApps`,
`SkillApps`). CC Switch, whose users we inherit, already shows them that
way: one row per item with a small icon switch for each app
(`UnifiedMcpPanel`, `UnifiedSkillsPanel`, `AppToggleGroup`). A prompt is the
other shape: each app has its own single active file, so per-app tabs remain
the honest layout there.

## Decision

1. **`ExtensionTab.layout`** says which shape a kind has: `unified` for Skills
   and MCP, `perApp` for Global Prompts. The page branches on this field, not
   on kind names.
2. **One row per item.** Name and a one-line description on the left, the
   row's actions, then one icon switch per supported app at the right edge so
   the columns line up. On is an outlined, filled tile; off is a faded grey
   mark, so state is not colour alone. Desktop apps carry a small screen badge
   because Claude Desktop and Claude Code share a mark. Installed apps that
   are not integrated for the kind are named once below the list.
3. **Found items join the same list.** Their app icons show where they are
   (dashed tile) and cannot be switched; the row offers **Import**. Import
   runs `app_extensions_adopt_detected` once per app the item was found in.
   The command now takes the item id and adopts only that item: a first call
   brings it under management, a later call for another app only switches it
   on there. The MCP path is upstream's `import_from_*` merge narrowed to one
   id, done in the compatibility layer; the live file is still read, never
   written. Adopting an id that is neither found nor managed is
   `ExtensionNotFound`. No confirmation dialog: importing rewrites nothing
   the user owns.
4. **No aggregate native command.** The list is `useQueries` over the existing
   per-app `app_extensions_list` queries, combined by id (managed rows) or by
   id among found rows. Toggles keep writing one app's cache entry; import
   and the background tasks invalidate `extensionKeys.all`. If any app's list
   has never been read the page shows the read error rather than a partial
   list; a failed refresh keeps the last rows and pauses changes, as before.
5. **Row actions.** Managed: Update (when one is known) and Remove. Found
   Skills: Open location, Edit SKILL.md, Copy to…, Import. Add stays in the
   page header and starts in the app the shell routed to, else the first
   supported app; the modal names that app and the new row's switches reach
   the rest.
6. **The shared-file row is gone from Skills and MCP.** With every app on
   screen there is no single file to name, and repeating N paths above the
   list is the clutter ADR-0045 argued against. CC Switch has no such row
   either. Global Prompts still show it.

## Consequences

- `localExtensionGroups.ts` is deleted: the row merge answers "which apps have
  this Skill" directly, so the page no longer reads the local inventory. Its
  tests moved to `unifiedExtensionRows.test.ts`.
- The unused adoption dialog copy and the `extensions.inventory.*` group
  headings are removed; `extensions.list.*` and four `extensions.adoption.*`
  keys replace them in all 13 locales.
- `ExtensionCard`'s found-item branches are no longer reached from the page
  (only prompts render cards). They are left in place with their tests and
  recorded here as debt to remove in a follow-up.
- Two found MCP servers with the same id in different apps become one row,
  and importing keeps the first app's spec; the other app only gains a flag.
  This is upstream's import rule, and it is why the MCP remove confirmation
  already says removal applies everywhere.

## Alternatives rejected

- **A native "list everything" command.** It would duplicate the per-app
  reads and their gating and give toggles a second cache to keep in sync.
- **A page-level "Import all" button** (upstream's shape). It imports things
  the user did not look at; a per-row button says exactly what will change.
- **Per-app file buttons next to the switches.** Hidden in a tooltip they are
  undiscoverable; visible they double the width of every row.
