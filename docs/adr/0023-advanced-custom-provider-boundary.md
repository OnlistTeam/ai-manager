# ADR-0023: Advanced Generic Custom Provider Keeps the Typed Native Boundary

- Status: Accepted
- Date: 2026-08-28

> 2026-08-30: ADR-0031 has superseded the Advanced Mode visibility gate in decision 2 of this document. The typed
> native boundary, HTTPS validation, and tool-native write decisions remain in effect; custom endpoints are now the
> default entry of "Add API endpoint", and presets are an optional quick fill.

## Context

ADR-0016 lets Beginner Connect select only reviewed Provider presets; the renderer cannot submit arbitrary endpoints
or `settings_config`. This boundary must not be relaxed for R5.8; yet Advanced users genuinely need to connect to
HTTPS services outside the catalog that are compatible with the current tool's API protocol. The native shapes in
which the eight tools store Base URL, API Key, and model are not the same, and the existing
`apply_draft` / `ProviderService::add` already carry field adaptation and the database/live-config transaction
respectively.

The existing configuration archive import/export covers the full database and can therefore carry Providers, but the
format is plaintext SQL with permissions tightened to `0600`. It is not an encrypted archive. Although the
repository's existing `zip 2.4.x` exposes an AES API, its own API documentation explicitly warns that the
cryptographic implementation has not been reviewed for correctness and should be treated as insecure, so it cannot
be used to protect Provider keys. Reliable password-based encryption also needs a dedicated format, KDF/AEAD, error
semantics, key zeroization, and dependency audit, and must be decided together with the R5.1 SecretStore semantics
of "whether backups carry keys".

## Decision

1. Add an independent `ProviderCustomCreateDraft` and `app_provider_custom_create` without modifying the Beginner
   `ProviderCreateDraft`. The strict contract on both ends accepts only `name / apiKey / model / baseUrl`; the API
   Key still flows only one way, renderer → native, and Debug output hides the API Key and the endpoint.
2. The custom entry is rendered only in Advanced Mode. The remote address must be an HTTPS Base URL with a host and
   without username/password, query, or fragment, at most 2 KiB; plain HTTP (including localhost) is not part of
   this initial slice. Name, model, and key also have bounded input, and native is the final validation authority.
3. Native uses the `ToolId` to select the canonical default template from that tool's reviewed catalog, then writes
   only the four public fields through the existing `apply_draft`. The template's service homepage is removed so
   that a custom endpoint is never disguised as the default service provider; the renderer never receives the
   template's raw JSON, headers, environment variable names, auth mode, or protocol switches.
4. Creation continues to reuse the existing deterministic request-id, the Provider mutation lock,
   `ProviderService::add/update/switch`, and the partial-commit convergence path. The first service can still be
   activated; when a current service already exists it is only saved; the explicit address check still runs after
   a successful save against the exact Provider ID returned by the backend.
5. This capability only promises "the endpoint implements the API shape the current tool expects" and does not
   promise protocol conversion. OAuth/managed accounts, custom headers, free-form renderer JSON, and local plaintext
   HTTP are not opened by this entry.
6. This ADR does not relabel the existing SQL import/export as encrypted. The encrypted portable archive is defined
   independently by ADR-0030 and, per AI_RULES #5, obtains explicit approval after explaining the necessity, size,
   security, and maintenance cost of the exact cryptographic dependencies; until approved, R5.8 remains partially
   complete.

## Consequences

- Advanced users can add out-of-catalog HTTPS services for the eight managed tools, while the Beginner
  reviewed-preset boundary is unchanged.
- The eight native configurations continue to be maintained by the same compatibility layer and the same
  transaction; no second renderer adaptation table appears.
- The existing plaintext SQL archive still works under the current warning and private file permissions; the
  product does not offer it a non-existent encryption guarantee.
- The remaining item of R5.8 is ADR-0030's encrypted archive/dependency approval, which does not block shipping the
  custom Provider slice.
