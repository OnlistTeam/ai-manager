# ADR-0047: MCP install draft carries environment variables and headers

- Status: accepted
- Date: 2026-09-26
- Amends: ARCHITECTURE §6.6 (guided MCP install draft), the MCP note in ADR-0029

## Context

The guided MCP form took a name, a command with arguments, or a URL, and
nothing else. The draft deliberately had no `env` or `headers` field, and the
form said so in a sentence of its own.

Most MCP servers people actually add need a credential: GitHub, search, and
database servers read an API key from an environment variable, and hosted
servers want an `Authorization` header. A connection added without one starts
and then fails, and the user has no way to fix it inside the product. What the
user holds is usually a JSON block from the server's README. CC Switch
accepts that block directly and has environment variable fields; its
unified server spec (`mcp/validation.rs`) already stores `env` and `headers`,
and every tool adapter translates them (Codex `http_headers`, OpenCode
`environment`, Hermes and Claude Desktop as is).

## Decision

1. **The draft gains typed variables.** `connection.stdio` has `env` and
   `connection.http` / `connection.sse` have `headers`, each an ordered list of
   `{ name, value }`. A list rather than a map, so a repeated name reaches
   validation instead of being collapsed by the decoder. Zod (renderer) and
   serde `deny_unknown_fields` (native) still reject any other field.
2. **Validation, both sides:** at most 64 entries; environment names are
   POSIX portable (`[A-Za-z_][A-Za-z0-9_]*`), header names are RFC 9110 tokens
   compared case-insensitively; names are unique; values are single-line and at
   most 8 192 characters. Blank rows are dropped in the renderer.
3. **Values are secrets.** The draft types still implement neither `Serialize`
   nor `Debug`; validation errors name the rule, never the variable or value;
   the operation record keeps only the connection name. The value is shown in
   plain text in the form, matching how saved API keys are shown elsewhere.
4. **The compat layer writes upstream's own slots.** `connection_spec` emits
   `env` / `headers` objects in the unified spec and hands it to
   `McpService::upsert_server`, which performs the per-tool translation and the
   eight-step write. Empty lists emit no key.
5. **Pasting JSON is a renderer convenience, not a new input.** A secondary
   "Paste configuration" action parses a README block
   (`mcpConfigPaste.ts`: `mcpServers` / `servers` / `mcp` wrappers, a named
   object, a bare fragment, or a single server) into the form's fields. The
   user reviews them and submits the typed draft. The JSON text never crosses
   IPC, and the first view of the form still shows no JSON (design spec §36).
   When a paste holds several servers the first is used and the form says
   which; one form describes one connection.

## Not changed

Deep-link import still refuses MCP servers with `env` or `headers`. The
draft could now carry them, but the link preview has no way to show and
confirm a credential, and a link is a weaker origin than the user's own
typing. Extending it is separate work with its own review.

## Consequences

- Servers that need a key can be added and work on first run, in every tool
  that supports MCP, without editing configuration files.
- The product now writes credentials into tool configuration files. Those
  files already hold credentials written by the tools themselves and by
  upstream's MCP panel; the eight-step write applies unchanged.
- Fields a README may carry beyond command, arguments, env, URL and headers
  (`cwd`, `timeout`, `disabled`, tool allow-lists) are ignored by the paste
  parser rather than silently widened into the draft.
