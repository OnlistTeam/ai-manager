# ADR-0062: Skill and MCP rows say what they are, and MCP can be edited in place

- Status: accepted
- Date: 2026-09-27
- Supersedes: the "no path on `Extension`" clause in `domain/extension.rs`
  (and ADR-0045's "A path field on `Extension`" rejection), ADR-0045's
  rejection of per-row MCP edit buttons, ADR-0048 decision 5 (which row
  actions each kind has) and the part of ADR-0048 decision 6 that left Skill
  and MCP rows with no location at all.

## Context

Three complaints came back about the unified Skills and MCP list
(ADR-0048):

1. "Where did editing go? And the path isn't shown anymore." A Skill's Open
   location and Edit SKILL.md were offered only while it was *found*; the
   first switch click takes a found item over, and the two icons vanished
   with it. An MCP connection could not be edited at all, and ADR-0048 had
   removed the file-path row above the list. Once a connection is managed,
   the product database is the source of truth and each tool's file is a
   written copy, so editing `~/.claude.json` by hand is overwritten by the
   next write. Editing has to happen in the app or not at all.
2. "The icon switches are too terse." Each row ends in one small mark per
   app and nothing names the columns; the only way to learn which mark is
   which app is to hover each one.
3. "If a Skill or MCP server was made for one project, does switching it on
   for another tool still work?" Often it does not: a server started as
   `node ./dist/index.js` depends on the folder it is started in, a
   `${TOKEN}` placeholder is filled in by some tools and passed through
   literally by others, and a SKILL.md that says "read
   `~/.claude/settings.json`" assumes one tool. Nothing on the page said so.

## Decision

1. **One detail line under every Skill and MCP row, managed or found.**
   - MCP: the command line of a local server (`npx -y @playwright/mcp@latest`,
     arguments quoted as a shell would need them), or the address of a
     remote one. Environment variable and header values are never part of
     it. An argument that follows a flag whose name contains `key`, `token`,
     `secret`, `password` or `auth` is shown as `••••`, as is a
     `NAME=value` argument with such a name, a URL's userinfo, a
     credential-named query value, and anything shaped like a known
     credential (`sk-…`, `ghp_…`).
   - Skill: the folder it lives in, `$HOME` shortened to `~` (a managed
     Skill's single stored copy; a found Skill's folder in the tool), plus
     `owner/repo` when it came from GitHub.

   `Extension` gains `detail: Option<String>` (at most 400 characters) and
   the rule in `domain/extension.rs` is narrowed accordingly: no *payload*
   (spec, body, environment or header values) and no field the renderer
   sends back; one display line is allowed. ADR-0045 had already argued that
   a path is not a payload. The line is built in `domain::extension_detail`,
   so the masking rules are tested in one place.

2. **MCP connections can be edited in place.** A pencil on every MCP row,
   where CC Switch users expect it, opens the existing `McpInstallModal` in
   edit mode, prefilled with name, description, transport, command and
   arguments or address, and environment variables or headers with their
   values (ADR-0047 already shows values plainly). The id is fixed; saving
   replaces only the fields the form owns, keeps every other key of the
   stored spec (`timeout`, `cwd`, tool allow-lists) and every app's on/off
   flag, and hands the row to upstream's `McpService::upsert_server`, which
   rewrites every app where it is on, the same path an install takes. A
   failed write puts the previous row back the same way.

   Two commands: `app_mcp_get(scope, id)` returns an `McpEditForm` and
   `app_mcp_update(scope, id, draft)` takes the ordinary `McpInstallDraft`
   and its validation. The draft types still derive neither `Serialize` nor
   `Debug`; the outbound form is a separate type that is `Serialize` only,
   so a value cannot reach a log through `{:?}`. The form is read only while
   the dialog is open and is not kept in the query cache after it closes.
   Pasting configuration stays an add-only convenience.

   Editing a *found* connection first takes it over from every app it was
   found in, exactly as its first switch click would
   (`app_extensions_adopt_detected`), then saves. The update command itself
   refuses an id that is not managed.

3. **Every Skill row has Open folder and Edit SKILL.md.** A managed Skill
   resolves to its stored copy through `app_skill_resource_open(skill,
   action)`; a found one keeps `app_detected_skill_resource_open`. The
   renderer sends an id and an action, never a path, and the native side
   checks the folder is a single directory name inside the store and still
   has a SKILL.md.

4. **A portability hint, computed natively.** `Extension.portability` is
   `{ reason, worksIn }` and is the same in every app's entry, so the page
   can warn before a switch is turned on:
   - `relativePath`: a local server's command or an argument (or the value
     after `--flag=`) starts with `./`, `../`, `.\` or `..\`, or is a bare
     `dir/file.ext`, and no `cwd` or `working_dir` is set. Package specs
     (`@scope/pkg`, `pkg@1.2`), `scheme:` references, URLs, absolute and
     home paths, and `owner/repo` without an extension do not count.
     `worksIn` is empty: this depends on the project, not the app.
   - `envReference`: a command, argument, environment value, address or
     header value contains `${`. None of the upstream adapters in
     `src-tauri/src/mcp/` rewrite these, so what matters is whether the tool
     expands them: Claude Code, Gemini CLI, Grok Build and Hermes do; Codex,
     OpenCode and Claude Desktop pass the text through. That table lives in
     the compatibility layer (`extension/mcp/display.rs`) and becomes
     `worksIn`.
   - `toolHome`: the Skill's own SKILL.md (first 64 KiB only) names a
     Skill-capable tool's home folder as `~/…`, `$HOME/…`, `${HOME}/…`,
     `%USERPROFILE%\…` or its absolute path. The list of home folders is
     data, each tool's skills folder one level up as upstream resolves it,
     including a user's override; `worksIn` is the tool or tools named.

   The row shows one small warning mark beside the name when the item is on
   in an app outside `worksIn`; its tooltip and accessible name are one
   sentence saying what may break there and what to do. An off switch for
   such an app adds "may not work here" to its tooltip and name. No badge
   text.

5. **A slim column header names the switches.** Above the list, one row
   whose right side has a cell per app in the same order and width as the
   switches: the same mark and a short name (`Claude`, `Codex`, …) from the
   artwork tables, truncated, with the full name in a tooltip. Both the
   header and the switch group use the same column classes
   (`SCOPE_COLUMNS_CLASS`, `SCOPE_CELL_WIDTH_CLASS`) and sit at the right
   edge of the same padded row, so they line up at every width, including
   when a narrow row wraps its switches onto a second line. The header is
   hidden from assistive technology because every switch already names its
   app. The left side stays empty.

## Consequences

- Wire format: `Extension` has two more optional fields. Zod declares them
  `nullish` so fixtures and payloads that predate them stay valid; native
  always sends them. Rust tests that asserted "no command on the wire" now
  assert the command line appears and environment values do not.
- Three new commands (`app_mcp_get`, `app_mcp_update`,
  `app_skill_resource_open`) in their own file,
  `commands/app_extension_edit_api.rs`, and three new message keys:
  `error.mcp.editUnavailable` (a stored spec the form cannot represent),
  `error.mcp.updateFailed`, `error.mcp.updateRestoreFailed`.
- Listing Skills now reads each Skill's SKILL.md (bounded) once per app
  list. Lists are read per app and cached for the session, so this is a few
  hundred small reads at most.
- The environment-expansion table is a claim about other products and will
  drift. It errs toward warning: a tool not listed is treated as not
  expanding.
- Global Prompts are unchanged: per-app tabs and the shared-file row of
  ADR-0045.

## Alternatives rejected

- **Letting people edit the tool's file directly.** The next write from the
  product overwrites it, so it would look like the edit was lost.
- **A separate edit dialog.** The add form already validates every field
  and shows values; a second form would drift from it.
- **Computing portability in the renderer.** It would need the spec, which
  never crosses IPC, and the SKILL.md text.
- **One badge per concern on the row.** Three kinds of warning text on a
  dense list is the clutter the page is trying to avoid; one mark with a
  sentence is enough to prompt a look.
- **Full app names in the header.** Eight names do not fit over eight
  32-pixel switches; widening every switch column for the header would cost
  every row its density.
