# ADR-0021: Manage MCP for Claude Desktop as an Independent Extension Scope

- Status: Accepted
- Date: 2026-08-28
- Related: ADR-0002, ADR-0003, ADR-0005, ADR-0009, ADR-0017

## Context

The product's Extensions page currently only accepts a `ToolId`, while Claude Desktop is a desktop
configuration surface, not a ninth CLI. ADR-0005 already explicitly forbids fabricating a `ToolId`
for Claude Desktop. The existing `claude_desktop_config.rs` manages Claude Desktop's 1P/3P inference
gateway and profile; it preserves unknown root fields in the ordinary config file, but it is not an
MCP management engine.

Claude's official MCP documentation specifies that Claude Desktop's user configuration lives at:

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Both store connections under the root object `mcpServers`. Claude Code's `~/.claude.json` is a
separate configuration; the two cannot substitute for each other. R3.1 requires the unified panel to
list, import, enable/disable, install, and remove MCP for Claude Desktop while continuing to honor
the eight-step external configuration write and the vendor lifecycle ownership boundary.

## Decision

1. Domain adds a typed `ExtensionScope`: `tool { id: ToolId }` and
   `desktopApp { id: DesktopAppId }`. Extension IPC, Query keys, and MCP write commands all use this
   scope as identity; no fake `ToolId` is added, and Claude Desktop is not mapped onto Claude Code.
2. The only writable desktop scope in the first phase is `desktopApp/claude-desktop + mcp`. The
   Desktop App inventory projection gains a `canManageMcp` capability; the scope only appears in the
   UI for an installed Claude Desktop running on macOS/Windows. Native still performs an independent
   allowlist check on the scope/kind combination; renderer capability is not a source of
   authorization.
3. Own `app.db` Schema v20 adds
   `enabled_claude_desktop BOOLEAN NOT NULL DEFAULT 0` to the global `mcp_servers`. It has the same
   semantics as the existing per-CLI toggles; when importing an old schema from CC Switch it always
   takes the default `false`, and ownership of the external desktop configuration is never guessed.
4. The live configuration uses the official path and the `mcpServers` root field. On read, the file
   size must be bounded, UTF-8, a JSON root object, `mcpServers` an object, and every server spec
   valid; unknown root fields are preserved as is.
5. Every Claude Desktop MCP write executes Read → Validate → Backup → Modify → Temp Write →
   Validate → Atomic Replace → Verify. The backup is a snapshot of the original bytes within this
   operation; failure at any step attempts an atomic restore of the original file (removing the new
   file if none existed before), and a failed restore escalates into a separate error.
   New files are written with private permissions. All CLI/desktop MCP writes continue to share the
   same process-level global lock.
6. Operation gains a `desktopApp` target that is mutually exclusive with `tool`, with a separate
   mutex for desktop apps. MCP install/remove reuses the same background state machine, structured
   logs, validation, and Task Center presentation; desktop tasks do not occupy Claude Code's tool
   lock.
7. The desktop lifecycle boundary of ADR-0009/0017 is unchanged: AI Manager manages only Claude
   Desktop's user MCP configuration and does not download, replace, update, or uninstall the vendor
   application itself.

## Alternatives

- **Treat Claude Desktop as Claude Code.** Would write the wrong config file, wrongly share the
  operation lock, and violate ADR-0005. Rejected.
- **Add `ToolId::ClaudeDesktop`.** Would mix a desktop app into the CLI install/update/version
  capability table. Rejected.
- **Only read the live file and not store the desktop toggle in the global MCP record.** Cannot
  stably express the disabled state, and after a restart cannot distinguish "managed by AI Manager
  but off" from "never managed". Rejected.
- **Let the renderer submit config paths or JSON directly.** Widens the sensitive-configuration
  egress channel and bypasses the native allowlist/transaction boundary. Rejected.

## Consequences

- Positive: CLIs and Claude Desktop work under one Extensions mental model, while identity, config
  files, and concurrency locks stay genuinely independent.
- Positive: inference gateway/profile fields and MCP fields in the ordinary config can coexist
  safely; a failed write never replaces a valid old config with a half-written file.
- Negative: the Extension wire model, settings memory, Operation target, database, and
  four-language UI must migrate in lockstep; this is a cross-layer Feature that must be accepted
  together with the full set of Rust/renderer/boundary tests.
- Negative: only macOS/Windows is promised for now. Even if an app of the same name is detected on
  other platforms, the MCP write capability is not opened; later support requires separately
  verifying the vendor's public paths and formats.
