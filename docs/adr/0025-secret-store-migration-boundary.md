# ADR-0025: SecretStore Migration and Portable Key Boundary

- Status: Proposed — awaiting product owner approval of the schema and dependency
- Date: 2026-08-29

## Context

Provider API Keys are currently stored as plaintext JSON in `providers.settings_config`. The renderer already only
writes and never reads keys, and logs are redacted, but these boundaries cannot substitute for protection of data
at rest. Design spec §19 and roadmap item R5.1 require migrating product-held keys into macOS Keychain and Windows
Credential Manager; Linux has entered the official release matrix, so the degraded semantics of Secret Service can
no longer be sidestepped with "platform not yet released".

The upstream Provider service still works with a complete configuration JSON, and the live configuration of some
managed tools must also contain the plaintext key in the vendor's format. SecretStore can only protect AI Manager's
own database at rest and cannot falsely promise that third-party tools' configuration files are also encrypted by
the OS Keychain. SQLite transactions and the OS credential store do not share atomic commits either, so a naive
"delete from the database first, then write to the Keychain" loses keys on crash, while "write to the Keychain
first and never clean up the database" cannot complete the migration goal.

## Proposed Decision

1. Add a Domain `SecretRef` and an Adapter `SecretStore`. Application/Domain handle only opaque references and
   `present/missing/unavailable` states; the renderer, logs, error context, and backup manifests never receive key
   values or OS credential-store account names.
2. Use the v1 backends of `keyring = 4.1.6`: macOS Keychain, Windows Credential Manager, Linux
   Secret Service. The service name is fixed to `tools.aimanager.desktop.provider-secret`, and the account uses a
   random opaque ID containing no Provider name, endpoint, tool name, or user identifier.
3. Add the next available database migration to establish key references and a two-phase migration journal; do
   not pre-reserve a specific schema number, to avoid conflicting with the Usage byte-cursor migration that is
   already pending approval. Each migration executes: write to the OS store → read back and verify byte by byte →
   write the reference and mark committed in the same SQLite transaction → re-check that the Provider can be
   materialized → finally clean up the old entry. Failure at any step preserves the original plaintext and a
   retryable state, and "OS store unavailable" is never treated as a successful migration.
4. After migration, the Repository returns Providers without keys; only native Application use cases briefly
   materialize the key right before actually writing live configuration, testing a connection, or executing a
   managed request. The temporary value does not enter `Debug`, Operation output, IPC, or persistent caches, and
   is released in the narrowest scope.
5. Creating/updating a Provider must first write and verify the OS store, then commit the key-free Provider data;
   on failure the newly created orphan credential is deleted. Deleting a Provider commits the business deletion
   first, then cleans up the credential on a best-effort basis; a failed cleanup enters a bounded journal retry
   and must not roll back an already deleted Provider.
6. When Linux has no usable Secret Service, existing unmigrated plaintext configuration may still run read-only
   with an explicit warning; adding or rewriting keys is forbidden, as is silently falling back to new plaintext
   storage. When the macOS/Windows Keychain is locked or denies access, the same fail-closed write semantics
   apply.
7. Ordinary database backups, automatic recovery copies, and cloud sync snapshots carry only references, not OS
   keys. After cross-device restore, references are projected as `missing` and the user re-enters them. R5.8's
   "portable archive carrying keys" must use an independent format with password KDF + AEAD and an independent
   cryptographic dependency approval; database references or the existing ZIP AES API must not be passed off as
   encrypted export.
8. Before the migration ships it must cover crash points, repeated startups, Keychain denial, Linux without a
   session bus, orphan cleanup, backup restore, live writes, and data retention after uninstall; official
   installers on all four platforms still need their own real-machine evidence.

## Dependency Approval Request

- **Why needed:** hand-writing Security.framework, Credential Manager, and Secret Service FFI would add three
  unsafe platform implementations and would struggle to correctly cover session locking, access denial, and Linux
  D-Bus; no existing dependency provides a unified OS credential-store abstraction.
- **Proposed dependency:** `keyring = "=4.1.6"`, MIT OR Apache-2.0, Rust 1.88+; the project's current Rust 1.95
  satisfies this. The crate itself unpacks to about 236 KiB; the default v1 feature selects the Apple, Windows, or
  zbus Secret Service backend by target platform and does not enter the renderer bundle.
- **Security impact:** this is the key trust boundary, and supply-chain risk is higher than for an ordinary
  utility library. After approval, the first lockfile change must audit the platform backends, default features,
  install scripts, and licenses item by item and run `cargo deny`/the existing equivalent audit gates; the example
  CLI, the in-memory database fallback, and the volatile mode of Linux kernel keyutils must not be enabled.
- **Maintenance impact:** pin the patch version; upgrades must independently re-verify backend and migration
  compatibility. OS store failures need stable error codes and a recoverable journal, and platform error text must
  not be exposed directly to the UI.

## Pending Owner Approval

1. Whether to accept the `keyring =4.1.6` dependency above;
2. Whether to authorize the new database migration and two-phase journal;
3. Whether to accept the degradation policy of "old values read-only, no new plaintext" when Linux Secret Service
   is unavailable;
4. Whether to accept that ordinary backups do not carry keys by default and cross-device restore requires
   re-entry.

Until all four points are approved together, R5.1 remains blocked and the existing plaintext limitation continues
to be shown truthfully.
