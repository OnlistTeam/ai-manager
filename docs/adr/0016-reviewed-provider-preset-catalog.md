# ADR-0016: Upstream Provider Presets Enter the Product Through a Reviewed Catalog

- Status: Accepted
- Date: 2026-08-26

## Context

The upstream implementation tracked in the repository maintains a large number of provider presets
for Claude Code, Codex, Gemini CLI, OpenCode, Grok Build, OpenClaw, Hermes, and Pi. Rewriting these
templates entirely would increase sync cost, but handing the upstream objects directly to the renderer
would simultaneously expose free-form `settings_config`, tool-native key slots, OAuth/managed-account
fields, partner promotion ordering, region tags, and public links that may contain a path/query. That
would both expand the product IPC and make the "compatible services catalog" look like a product
endorsement or a region-specific edition.

Beginner Connect only needs to let the user pick a service and enter a display name, API key, and
model; it does not need the renderer to understand the native configuration formats of eight tools.
The existing upstream `add` / live-config transaction is already the write authority and should
continue to be reused.

## Decision

1. A build-time generator loads the eight tracked TypeScript preset modules directly from the
   repository and produces a versioned JSON catalog; a sync test requires the generated output to match
   the committed catalog byte for byte, so upstream changes never enter the product silently.
2. The catalog accepts only API-key templates that have an empty key slot, a parseable safe HTTPS
   endpoint, and can be read and written by the existing compatibility layer. OAuth, managed accounts,
   hidden entries, unsupported format conversions, non-HTTPS or credential-bearing endpoints, and
   templates with an existing plaintext secret are all excluded.
3. The generated output drops partner/referral fields, region classification, and the original
   ordering; `default` and `official` are two independent facts. Each tool has exactly one default
   entry, but only a genuine first-party API may be marked official: Grok Build defaults to xAI,
   Hermes defaults to Nous Research; OpenClaw and Pi are clients rather than model vendors, so they
   default to OpenRouter without posing as official. The remaining compatible services are sorted by
   neutral name. User-facing homepage and get-key links keep only public HTTPS origins without
   username, password, path, query, or fragment.
4. The renderer receives only the stable ID, service name, default name/model, public links, and
   official flag of a `ProviderConnectionPreset`; the create payload is only
   `presetId / name / apiKey / model`, and both Zod and Rust reject extra raw configuration fields.
5. Native resolves the real template from a parse-once cache by `(ToolId, presetId)`, then injects the
   key and model using the compatibility layer's existing `apply_draft` and the upstream write
   transaction. Long-tail adaptation is responsible only for safe field projection of Grok TOML,
   OpenClaw/Pi camelCase JSON, and Hermes snake_case mappings; the database, live files, switching,
   rollback, and additive semantics remain authoritatively implemented by the upstream
   `ProviderService`. When the catalog is loaded, templates are re-validated for endpoint, absence of
   plaintext secrets, key slot, and full round-trip capability of the default model; on failure the
   whole catalog fails closed.

## Consequences

- Non-technical users get the current 435-entry, eight-tool compatible services catalog without the
  product maintaining a second set of hand-written templates or showing region-specific sources to
  users worldwide.
- Upstream upgrades can still be followed by regenerating, but new partner fields, OAuth flows, or
  unsafe configurations do not automatically expand product capabilities.
- API keys still enter native one-way from the form only; free-form JSON, real endpoint templates, and
  tool-native key slots do not cross the IPC.
- Generic custom providers, OAuth/managed accounts, ordering, import/export, full credential
  validation, and per-service proxies remain separate safety slices and are not passed off as
  supported by this catalog.
