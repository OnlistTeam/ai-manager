# ADR-0010: Pinned-Version Operations Must Preserve Installation Ownership

- Status: Accepted
- Date: 2026-08-24

## Context

The CC Switch v3.20 lifecycle only expresses "install latest" and "update to latest"; it has no
historical version catalog or downgrade entry. AI Manager can already recognize npm, pnpm, bun, Volta,
Homebrew, official installer, and unknown sources; if it ran a bare `npm install -g` for every
old-version install, it would create a second CLI copy alongside an official-installer or Homebrew
installation, and PATH would then decide by accident which copy the user runs.

The automatic network policy can already append one `registry.npmmirror.com` retry after a
recoverable transfer failure on an official npm-compatible download, and can inject the user's
explicitly configured local proxy into the child process. Pinned-version queries and installs must
reuse the same policy, must not modify the user's global npm configuration, and must not use a
registry switch to mask authentication, permission, or package-metadata errors.

## Decision

1. Open `canManageVersion` for tools with a definite npm package mapping; currently Claude Code, Codex,
   OpenCode, Gemini CLI, Grok Build, OpenClaw, Pi, Kimi Code, and DeepSeek Harness, with Hermes kept
   off. Kimi's native installation continues to use the official `kimi upgrade`; only copies proven to
   belong to a package manager may pin an npm version.
2. The backend obtains the version catalog by invoking a structured
   `npm view <package> dist-tags versions --json` argv, returning at most the 200 most recent versions
   that pass length and character validation, plus at most 32 publisher-defined safe dist-tags.
   Ordinary install and update automatically use the recommended latest stable version and do not
   require the user to enter the version picker first; the recommendation normally follows `latest`.
   For Grok, where `latest` has been proven to lag far behind the published stable version, the
   recommendation is the highest stable version and npm automatic operations use the equivalent stable
   version range; tags such as `alpha`, `beta`, and `next`, along with historical versions, are used
   only for explicit compatibility switches and rollbacks.
3. The primary request explicitly uses the official npm registry in the current child process only
   and must not silently inherit a stale global mirror configuration; only recoverable download
   failures such as DNS, connection, TLS, timeout, and 403/regional restriction from the public
   registry append a single community-mirror retry for that child process alone. 401/407, permission
   errors, 404, nonexistent versions, and malformed successful responses fail immediately without
   switching mirrors; neither path modifies the user's global npm configuration, and the local proxy
   enters only the child process environment.
4. Pinned-version installation runs as a separate `changeVersion` operation, continuing to use the
   per-tool mutex, streamed bounded logs, redaction, failure classification, and post-completion
   re-detection; the re-detected version must exactly match the target, otherwise the operation ends
   as a verification failure.
5. An uninstalled tool may have a pinned version installed by the npm found on PATH. An installed tool
   only invokes the package manager that owns the current entry point: npm, pnpm, bun, and Volta are
   each anchored to the package manager executable next to the detected entry point.
6. Official installer, Homebrew, and unknown sources always reject pinned-version operations, and the
   UI explains why. There is no automatic crossover to npm, and "the command returned success" is not
   treated as proof that the default entry point has switched.
7. The frontend only submits versions that were shown in the backend catalog, but the backend still
   validates the version string independently; the package name is always determined by the ToolId
   registry, and the renderer cannot submit a package name, command, registry, or executable path.

## Consequences

- npm-compatible tools such as Claude Code can be installed or downgraded to an explicit version, and
  users on restricted networks get the same mirror and proxy recovery path as ordinary updates.
- An official Claude Code installation keeps using the official updater; to switch to the npm channel,
  the user must explicitly uninstall the original ownership first and then install, and is never
  silently migrated in a single "downgrade".
- The version catalog depends on the local npm; when npm is absent, both the network and the mirror
  fail, or the registry offers no historical list, a real failure is reported rather than a
  fabricated empty catalog or a blind registry switch.
