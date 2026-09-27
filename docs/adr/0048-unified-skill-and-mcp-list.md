# ADR-0048: One list for Skills and MCP, with a switch per app

- Status: accepted
- Date: 2026-09-26
- Amended 2026-09-26: found items are taken over by their first switch; the
  Import button and Copy to… are gone (decision 3, 5).
- Supersedes, for Skills and MCP only: the "software selector" in ADR-0038
  (one tab per installed tool or desktop app) and ADR-0045's shared-file row
  above the MCP list. Global Prompts keep both.
- Partly superseded by ADR-0062: decision 5 (row actions: every Skill row now
  has Open location and Edit SKILL.md, every MCP row has Edit) and the part of
  decision 6 that left rows with no location (each row now carries one detail
  line). ADR-0062 also adds a column header above the switches.

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
3. **Found items join the same list and switch like any other.** An app it
   was found in shows as on, the rest as off, and the list is one order by
   name, so nothing marks a row as different. The first switch the user
   clicks takes the item over: `app_extensions_adopt_detected` runs once per
   app the item was found in (switching each on, which it already is), then
   once for the clicked app with the state asked for. The command takes the
   item id and an `enabled` flag and adopts only that item: a first call
   brings it under management, a later call only sets its own app, including
   an app it was never found in. The MCP path is upstream's `import_from_*`
   merge narrowed to one id, done in the compatibility layer. Adopting an id
   that is neither found nor managed is `ExtensionNotFound`. No confirmation
   dialog: the click already says what should change.

   A first version showed found items as read-only dashed tiles with an
   **Import** button. Users read the item as already theirs and could not
   say where "import" would put it, and the row then jumped from the found
   group to the managed one. "Managed by AI Manager" is our bookkeeping, not
   something the user needs to act on.

4. **No aggregate native command.** The list is `useQueries` over the existing
   per-app `app_extensions_list` queries, combined by id (managed rows) or by
   id among found rows. Toggles keep writing one app's cache entry; a takeover
   and the background tasks invalidate `extensionKeys.all`. If any app's list
   has never been read the page shows the read error rather than a partial
   list; a failed refresh keeps the last rows and pauses changes, as before.
5. **Row actions.** Managed: Update (when one is known) and Remove. Found
   Skills: Open location and Edit SKILL.md. Copy to… is removed with its
   command (`app_detected_skill_copy`): switching the Skill on in another app
   puts it there, managed, so a second, unmanaged way to do the same thing
   only raised the question of which to use. Add stays in the
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
- The unused adoption dialog copy, the `extensions.inventory.*` group
  headings, `extensions.adoption.*` and `extensions.copy.*` are removed;
  `extensions.list.*` replaces them in all 13 locales.
- `ExtensionCard`'s found-item branches are no longer reached from the page
  (only prompts render cards). They are left in place with their tests and
  recorded here as debt to remove in a follow-up.
- Two found MCP servers with the same id in different apps become one row,
  and a takeover keeps the first app's spec; the other app only gains a flag.
  This is upstream's import rule, and it is why the MCP remove confirmation
  already says removal applies everywhere.

## Alternatives rejected

- **A native "list everything" command.** It would duplicate the per-app
  reads and their gating and give toggles a second cache to keep in sync.
- **A page-level "Import existing" button** (upstream's shape). Found items
  stay out of the list until imported, so a Skill the user can see working in
  Claude Code is missing here, and the dialog asks them to learn the same
  managed/unmanaged split.
- **An Import button per row** (this ADR's first version). See decision 3.
- **Taking every found item over when the page loads.** It would copy and
  record things the user never touched; a click on a switch is the consent.
- **Per-app file buttons next to the switches.** Hidden in a tooltip they are
  undiscoverable; visible they double the width of every row.
