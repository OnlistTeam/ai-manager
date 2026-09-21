# ADR-0037: Files the Product Manages Itself Keep a Single Write Entry

- Status: Accepted
- Date: 2026-09-09
- Relates to: ADR-0036 (the "Local data" destination), design spec §36 (basic Prompt management)

## Context

The term "global prompt" appears twice in the product, and both point to **the same file**:

- The "Global prompt" tab on the Skills / MCP page is a **library**: multiple prompts are stored in the DB, and
  selecting one writes its content to `prompt_files::prompt_file_path()` (`~/.claude/CLAUDE.md`, etc.),
  single-select takes effect, with an additional "Import current". The write points are at
  `src-tauri/src/compat/ccswitch/extension/prompt.rs:271,318`.
- The `instructions` resource item in the "local context and storage" panel at the bottom of the Local data page
  is a single line of **resource location**: path, size, plus an "Open and edit" that hands the same file to the
  system default editor. The path source is `src-tauri/src/compat/ccswitch/provider_runtime.rs:142` — which
  calls the very same `prompt_file_path()`.

The problem is not duplicated navigation; it is that both paths can write, while only one knows "which library
entry is currently in effect". After the user edits the file with an external editor from the data page, the
extensions page still marks some library entry as "In use" even though the content no longer matches; unless the
user remembers to click "Import current" on their own, this inconsistency is never signaled.

But the overlap is **not universal**. In the capability table, `can_manage_prompts` is true only for Claude Code
(`src-tauri/src/compat/ccswitch/tools/capabilities.rs:20`), while `provider_runtime::app_type()` resolves a
global prompt file for all eight tools. In other words, for the other seven tools, the line on the data page is
their **only** entry.

## Decision

1. The resource item's write entry is determined by the capability bit, not the tool name (AI_RULES rule 8): when
   `canManagePrompts` is true, the `instructions` item's button changes from "Open and edit" to "Go manage",
   navigating to the prompts tab of the Skills / MCP page with the current tool scope; when false, everything
   stays as is and it remains direct editing. The check lives in `DataStoragePanel`, which already holds both the
   `manageable` tool table and the currently selected tool.
2. Resource items no longer assume that "has a management UI" is unique to `instructions`. `resourcePresentation`
   gains a `managedDescription` slot per kind: non-null means what this kind of file should say once the product
   takes it over, null means there is no management UI and the item can only locate or open. Currently only
   `instructions` is non-null. When the caller supplies `onManage` but the kind has no `managedDescription`, it is
   treated as not taken over, so copy and behavior never disagree.
3. Route intents gain `extensionsKind`. Previously `ExtensionsTab` could only express "which scope to open", and
   which tab it landed on relied on the `ProductSettings.extensionKind` memory or on the implicit inference
   `routeScope?.kind === "desktopApp" ? "mcp"`. The handoff must land on the prompts tab — landing back on the
   remembered Skills tab would be the same as no handoff. In `useExtensionScope`, the priority of `displayedKind`
   is inserted after `localKind` and before the implicit inference; when the user manually clicks a tab,
   `setRouteKind(null)` hands control back, sharing the same lifecycle as the existing `routeScope`. The implicit
   inference is kept: `ToolsPage` passes a desktopApp scope without a kind, and there is no reason to change that
   path.
4. The resource item on the data page is **not deleted**. The panel answers "which files does this tool actually
   read, and how large are they"; erasing the global prompt would leave that question incompletely answered. It
   continues to show path and size, only no longer opening a write entry of its own.

## Known Trade-offs

- `onOpenExtensions` is now passed all the way from `AppShell` down to `ProviderRuntimeResourceItem`, four
  layers. This is the same pattern as the existing callback of the same name in `ToolsPage`; no context was
  introduced for it.
- "Runtime configuration" is handled differently: it is excluded from the panel entirely by
  `filter(kind !== "configuration")`, with the entry made into the endpoints page's `ServicesOpenConfigAction`
  (ADR-0036 decision 3). The global prompt did not copy that, because that endpoints-page move was a case of
  "this thing does not belong in this panel at all", whereas the global prompt does belong — it is one of the
  local files this tool reads.
- The handoff only resolves the product's internal dual write entries. If the user edits `~/.claude/CLAUDE.md`
  directly with an editor outside AI Manager, the library's "In use" entry will still drift out of sync. The real
  fix is to have the library detect file drift and prompt, which is a separate issue and not handled.
