# ADR-0020: Open Hermes Changes Only After the Python Tool Manager Proves Ownership

- Status: Accepted
- Date: 2026-08-28
- Extends: ADR-0010, ADR-0019

## Context

The current official Hermes Agent installer clones the official repository, creates a standalone venv, and writes a
launcher into the user's bin directory; although it uses uv to create the environment, it is not a `uv tool install`.
Historical versions or third-party guides may also have led users to install the PyPI package `hermes-agent` via
uv tool or pipx. All three channels can expose a `hermes` of the same name, and looking only at
`~/.local/bin/hermes`, the real Python path, or "uv exists on this machine" cannot prove who has the right to
upgrade or remove it.

Version management also has channel-specific semantics. uv's `tool upgrade` preserves the version constraint from
install time, so for an exact pin that was once installed from the version catalog it may exit 0 without upgrading
the main package; pipx's upgrade removes the old version constraint. Degrading both into a bare `pip install` would
bypass the isolated environment, change ownership, and potentially pollute the system Python.

## Decision

1. Hermes has no npm package mapping, so a Node manager label produced by the generic path heuristic does not count
   as ownership; a real Homebrew formula still keeps the Brew owner, and the remaining unmanaged/path-only results
   can be refined further. Only when the launcher hit by the default command-line lookup already exists and is
   runnable do we perform a one-off, read-only uv/pipx ownership proof; the official checkout/venv layout is never
   disguised as a uv tool.
2. The manager itself must be an absolute, existing file on the GUI's effective PATH, and after entering
   `AllowedProgram::Uv` or `AllowedProgram::Pipx` it is executed with array argv, a short timeout, and stdin closed.
   The probe does not go online, does not read arbitrary shell output, and does not accept paths or package names
   supplied by the renderer.
3. uv evidence comes from `uv tool list --show-paths`. The same `hermes-agent` receipt must list the `hermes`
   entrypoint, the currently exposed path, and the tool environment directory at the same time; the current
   launcher must also be the same file as the `hermes` inside that environment, or a bounded copy with identical
   content. An entrypoint path with the same name alone is not enough to prove a script that later overwrote the
   receipt.
4. pipx evidence comes from the versioned `pipx list --json`, and the local/global inventories are checked
   separately. The main package must normalize to `hermes-agent`, be exposed, have no suffix, and contain the
   `hermes` app; `pipx environment --value PIPX_BIN_DIR` (global scope for global) must equal the current launcher
   directory, and the corresponding venv app in the metadata must also be the same file as the current launcher or
   a bounded identical copy. Unknown JSON, corrupt metadata, non-zero exits, and timeouts do not count as evidence.
5. If uv and pipx both claim the same launcher, two different manager binaries both claim ownership, or the pipx
   local/global scopes conflict, the result stays `unmanaged`.
   Only unique evidence is saved as the internal `UvTool` / `Pipx` source together with the absolute manager path
   used for this proof; the path never enters the renderer or the history protocol. Every update, change-version,
   and uninstall re-probes and never reuses an old UI snapshot.
6. When attributed to uv: update uses `uv tool install hermes-agent@latest` to replace the old exact constraint, a
   specific version uses `uv tool install hermes-agent==<validated-version>`, and removal uses
   `uv tool uninstall hermes-agent`. When attributed to pipx: update uses `pipx upgrade hermes-agent`, a specific
   version uses `pipx install --force hermes-agent==<validated-version>`, and removal uses
   `pipx uninstall hermes-agent`; global scope is preserved as is. The fixed package name comes from the native
   registry, and the version still passes ADR-0010's length and character validation.
7. The version catalog queries the fixed HTTPS PyPI JSON endpoint only when uv/pipx ownership holds. Response
   status, type, total byte count, and every release key are validated within bounds; `info.version` is the
   publisher's latest, and `releases` only provides selectable exact versions. The request reuses the product HTTP
   client and explicit proxy; no unverified community mirrors are invented for Python packages.
8. `ToolInstallSource` gains `uv` / `pipx`, and Schema v19 atomically rebuilds the source CHECK on the
   version-event table while preserving existing rows and sort indexes. Both sources can enter verified version
   history and rollback; the official installer, unknown scripts, and insufficient evidence still keep uninstall,
   change-version, and automatic rollback closed, leaving only Hermes's own ordinary update.

## Consequences

- Hermes installed via uv tool and pipx gains same-owner upgrades, exact version switching, uninstall, and
  verifiable rollback; the system Python and the official checkout are not mistakenly deleted or overwritten across
  channels.
- Coincidentally identical paths, a manager that merely happens to exist, stale receipts, concurrent conflicts, or
  unrecognized new output formats never widen permissions; users can still run and use the official
  `hermes update`, but AI Manager does not falsely claim it can uninstall safely.
- Ownership probing adds a small amount of local subprocess overhead and cannot see custom UV/PIPX environment
  variables that are set only in interactive shells; this is a deliberate fail-closed trade-off, and any new format
  added later must first come with new fixtures and an ADR revision.
