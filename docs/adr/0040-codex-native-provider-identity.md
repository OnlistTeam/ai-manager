# ADR-0040: Codex Native Provider Identity Compatibility

- Status: Accepted
- Date: 2026-09-20
- Extends: ADR-0035

## Context

The product's API-key Codex template introduced `ai_manager_openai`, unlike the
inherited third-party presets' `custom`. Native `model_provider` is a persistent
Codex history namespace, not a display label or the product database row ID.
Creating an endpoint from this template can therefore separate the active
configuration from existing history metadata. `resume --all` only removes the
working-directory filter; it must not be described as a universal history repair.
Local evidence shows a provider mismatch but is not an end-to-end picker test.

The product's settings-preservation wrapper also treated legacy top-level endpoint
and bearer-token fields as unrelated preferences. It could restore fields that the
upstream switch had intentionally removed.

## Decision

1. Generate the API-key template with upstream's `custom` provider ID. Product
   branding belongs in display metadata, never in a new native history namespace.
2. For a new Codex connection, adapt the new template to an existing explicitly
   configured custom provider ID, case-sensitively, when its table exists. Move
   only the new template's table; never copy the old endpoint, token or auth mode.
   Built-in IDs are never overridden. Missing configuration uses the template;
   invalid/unreadable configuration or table collisions fail before persistence.
3. New records do not perpetuate the legacy product ID. An idempotent retry keeps
   the persisted attempt's ID (including a legacy one), not today's live ID. This
   prevents a retry from unexpectedly migrating an existing record.
4. Imported/saved records retain their identities on ordinary edit/switch. Do not
   rewrite history, infer identity from conversation contents, or silently migrate
   existing records. Recovery requires a separately reviewed backup/rollback plan
   for configuration and both history stores. No schema or dependency change.
5. Preserve upstream ownership of `base_url`, `openai_base_url` and
   `experimental_bearer_token` along with the existing Codex connection/MCP keys.
   Never paste old values back after a switch; newly written values still win.

## Verification and Limits

Regression tests cover catalog generation, custom/preset creation, case-sensitive
IDs, inline/quoted TOML tables, collisions, malformed inputs, store-only creation,
idempotent retries and unchanged fixture history. The adapter neither restores nor
suppresses upstream's ChatGPT login cleanup on an API-key switch, so `auth.json`
removal stays owned by the upstream switch and its existing preservation setting.
The production history files and index are not mutated by these tests or this
source change.

This prevents new accidental namespaces; it does not by itself repair an already
affected live configuration or guarantee every CLI/app history source appears in
the picker. Profile overrides and reserved-provider auth transitions remain owned
by the native tool/upstream compatibility layer, not guessed by this adapter.
