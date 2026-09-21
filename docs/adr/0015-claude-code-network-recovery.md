# ADR-0015: Claude Code Restricted-Network Recovery Does Not Mirror Vendor Proprietary Binaries

- Status: Accepted (decisions 4 and 5 superseded by ADR-0033)
- Date: 2026-08-26
- Superseded-by: ADR-0033 (decisions 4 and 5: native installations are no longer migrated to npm; native layout supply is used instead)

## Context

The official Claude Code installer and native self-update access Anthropic's download domains; some
networks can reach the npm registry normally but cannot reach the installer or the native release
bucket. Copying vendor binaries straight to the product's own release storage would make AI Manager
additionally responsible for third-party artifact synchronization, signature verification, licensing,
and emergency withdrawal, and would create an update protocol independent of Anthropic's release
channel.

Anthropic's current `@anthropic-ai/claude-code` npm main package declares per-platform optional
dependencies and, after installation, places the same native binary from the matching platform
package at the CLI entry point; Node.js does not stay resident. AI Manager's existing download policy
already runs npm operations with the official registry first, appends a community mirror for that
child process only after a recoverable network failure, 403, or regional restriction, and does not
modify the user's global npm configuration. 401/407, permission errors, 404, and nonexistent versions
still fail as-is. Testing confirmed that the macOS arm64/x64, Windows x64/arm64, and Linux x64/arm64
platform packages for the same version have all been synced to the fallback registry.

## Decision

1. The product's own release storage continues to host only AI Manager's own signed update
   artifacts; it does not mirror third-party proprietary installers such as Claude Code, Codex App,
   or Claude Desktop.
2. When Claude Code is not installed, the official installer remains the first choice; npm is tried
   only after the official channel fails, and the community mirror is attempted automatically after
   the official npm registry hits a network error/timeout, 403, or regional restriction. A 403/regional
   restriction from the vendor installer itself is not mistaken for a signal that mirroring third-party
   binaries is allowed.
3. npm-owned installations continue to anchor to the original package manager for updates and
   automatically benefit from the same mirror recovery.
4. **(Superseded by ADR-0033)** Claude Code owned by the official installer does not silently cross
   over to npm. The version management UI shows the installation source and offers an explicit
   "Migrate to npm" action, executed only after user confirmation.
5. **(Superseded by ADR-0033)** Migration first completes the npm installation while the current
   native launcher is still usable, then stages the launcher and re-detects the default entry point;
   it commits only after confirming the new npm entry point runs. On verification failure the launcher
   is restored and the npm installation is undone on a best-effort basis. Before staging, a private
   recovery record is atomically written to the product AppData; if the process exits unexpectedly
   after staging, the next native launch restores the original launcher before the renderer's first
   tool scan. The recorded path must re-pass the home-boundary and same-directory naming checks;
   ambiguous states such as both entry points existing at once fail closed, and neither copy is ever
   overwritten.
6. The renderer can only submit a stable ToolId and a fixed migration enum, not a package name,
   registry, command, or path. Real paths, staging names, and installation-source determination all
   stay at the native boundary.

## Consequences

- Users on restricted networks do not need to understand which alternative download source applies
  or run `claude update` manually; the npm path falls back automatically.
- An existing native installation does not gain an extra CLI copy chosen by PATH by accident after
  one failed update; cross-channel migration is always visible and reversible.
- AI Manager does not become a redistributor of Anthropic's proprietary binaries and can keep
  following the official npm package structure and versions directly.
- If multiple Claude Code copies already exist on the machine, no usable npm is available, or
  staging/verification/rollback fails, the migration fails closed, keeps the process log, and asks the
  user to deal with the existing installation rather than reporting false success.
