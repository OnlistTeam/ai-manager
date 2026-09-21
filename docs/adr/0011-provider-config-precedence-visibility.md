# ADR-0011: Provider Switching Does Not Take Over Shell Environment Variables

- Status: Superseded by ADR-0035 (2026-09-07; decision 1, "never delete or modify user shell variables", remains in force)
- Date: 2026-08-24

## Context

CC Switch's old environment-conflict feature scanned variables such as `ANTHROPIC_*` and `OPENAI_*`
and offered to back up and then delete shell configuration. AI Manager currently does not register
that set of delete commands, but provider switching still writes each tool's live configuration. When
the user has another Key/Base URL in `~/.zshrc`, `~/.profile`, or the system environment, the CLI may
prefer the external environment while the UI still shows the database provider as "currently
effective", leading users to believe their custom configuration is broken or has been taken over.

## Decision

1. AI Manager never automatically deletes, overrides, or migrates the user's shell/system environment
   variables, and does not restore the old `delete_env_vars` product command.
2. The upstream read-only scanner is reused but projected immediately at the compatibility boundary
   into a safe product model: only the variable name, type, `~`-relative source, and safe http/https
   endpoint are returned. API keys, tokens, and other raw values never cross the IPC.
3. The active state on a provider card is interpreted as "selected in AI Manager", not as an assertion
   that the CLI will ultimately use it. The page separately shows an "external environment may take
   precedence" state and its source.
4. The page shows, for each manageable tool, the live file locations the provider engine actually
   reads and writes, such as Claude settings, Codex config/auth, Gemini `.env`/settings, and OpenCode
   config; only home-relative logical paths are shown, and neither the full user home directory nor
   the product database path is handed to the renderer.
5. A custom provider's saved Base URL remains visible on the card; an external URL is shown only when
   it has no username, no password, and an http/https scheme. Credentials are shown only as "set".
6. When the same variable appears in both the current process and a shell file, the actionable file
   source is shown first to avoid duplicates; the system/current-process source is shown only when no
   file can be located.

## Consequences

- The user's existing Key/Base URL is neither taken over nor deleted by AI Manager, and the user can
  see why terminal behavior may differ after a switch.
- "Which provider is selected" and "what the CLI ultimately uses" are no longer conflated.
- This slice provides read-only diagnostics only; it does not offer one-click editing of environment
  variables or temporary overrides for launched sessions, which need their own permission,
  shell-semantics, and rollback design.
