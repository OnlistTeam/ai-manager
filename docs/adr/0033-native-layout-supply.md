# ADR-0033: Native Layout Supply Replaces "Migrate to npm"

- Status: Accepted
- Date: 2026-09-05
- Supersedes: ADR-0015 decisions 4 and 5 (explicit npm migration of the Claude Code native install and the launcher staging/recovery log)
- Extends: ADR-0010 (specified-version operations preserve install ownership), ADR-0015 decisions 1–3 and 6 (no mirroring of vendor private binaries, npm official first + community mirror fallback, renderer submits only stable ToolId)

## Context

Real-world direct-connection measurements taken on a network where the official channels are
unreachable:

| Fact                                                                                         | Result                                                                                                       |
| -------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `downloads.claude.ai` / `claude.ai` / `chatgpt.com`                                          | Direct connection times out (fails at the 10-second connect timeout)                                         |
| `claude update` gives up on its own under this network                                       | Retries 3 times, reports `Failed to fetch version from …/claude-code-releases/latest` after 93 seconds total |
| Pulling the 199 MB platform package from `registry.npmjs.org` / `registry.npmmirror.com`     | Both about 12 MB/s, done in a dozen seconds                                                                  |
| `registry.npmjs.org` / `registry.npmmirror.com`                                              | Reachable                                                                                                    |
| `raw.githubusercontent.com` / GitHub Release assets                                          | Reachable                                                                                                    |
| `package/claude` inside `@anthropic-ai/claude-code-darwin-arm64@2.1.261`                     | Mach-O arm64, codesign TeamIdentifier `Q6L2SF6YDW`, `--version` prints `2.1.261 (Claude Code)`               |
| SHA256 of that file vs. the file `claude install 2.1.261` downloads from the official source | Identical                                                                                                    |
| Official native layout                                                                       | `~/.local/share/claude/versions/<ver>` (single executable) + `~/.local/bin/claude` symlink                   |
| The binary's own `claude install <ver>` run offline                                          | Fails after a 10-second timeout; it needs to reach `downloads.claude.ai` itself                              |
| What Codex `codex update` (standalone install) actually runs                                 | `curl -fsSL https://chatgpt.com/codex/install.sh \| CODEX_NON_INTERACTIVE=1 sh`                              |
| `scripts/install/install.sh` in the GitHub repository vs. the online install.sh              | Identical content; `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` forces GitHub Releases                    |
| Codex standalone install layout                                                              | `~/.local/bin/codex` → `~/.codex/packages/standalone/releases/<ver>/codex`                                   |

ADR-0015 decisions 4/5 gave Claude Code installs owned by the official installer a mirror recovery
path through a one-time, explicit, reversible "migrate to npm". In practice it had three problems:
users had to understand and choose a new install channel; after migration, ownership of `claude`
changed from the official installer to npm, so subsequent `claude update` was no longer the official
self-upgrade; and the staged launcher + private recovery log + startup-time recovery formed a whole
state machine that existed only for this one action.

Measurements prove that the platform package Anthropic publishes on the npm registry contains the
very same native binary the official installer downloads, and that the file carries Anthropic's
Developer ID code signature. The product can therefore, after the official channel fails, fetch the
same official program from the registry and write it into the official layout without changing
install ownership.

## Decision

1. **Native layout supply** (`compat::ccswitch::native_supply` + `adapters::native_supply`): updating
   a native Claude Code install first runs `claude update` through the anchored entry; on failure
   without cancellation, the product fetches `dist.tarball` of
   `@anthropic-ai/claude-code-<platform>@<target>` from the npm registry, extracts only
   `package/claude`, writes it to `~/.local/share/claude/versions/<target>` (write `.partial` first,
   then rename), and atomically switches `~/.local/bin/claude` through a temporary symlink in the
   same directory. Old version files are kept, consistent with the official installer's behavior.
   1b. **Probe the official channel first; don't let it bang its head against the wall** (revised
   2026-09-06): before doing anything, send one HEAD request to `official_update_probe_url(id)`
   with the product's own HTTP client, capped at 10 seconds. **Only a transport-layer connection
   failure counts as unreachable** — any HTTP response (including 404/5xx) counts as reachable,
   because skipping the tool's own update command is a costly decision and is only made when
   conclusive. If the probe fails, skip `claude update` and supply directly; if the probe succeeds,
   self-upgrade first as before, and on failure still fall back to this decision's supply. Supply
   writes files, so `native_supply_follows` is confirmed once before and once after the probe (the
   user may press cancel during those 10 seconds). No new log key is added: the message supply
   itself records — "official update channel unavailable, fetching the same official program from
   the software download source" — holds equally on this path.
2. **Trust anchors**: the `dist.integrity` returned by the registry (only `sha512-` accepted) is
   verified before writing to disk; on macOS the new file must pass
   `/usr/bin/codesign --verify --strict` with `TeamIdentifier=Q6L2SF6YDW`; then the tool's own
   `--version` re-verifies the target version. Failure at any step deletes the new file and leaves
   the launcher untouched. Linux relies only on integrity and `--version`. The official Windows
   layout is unverified, `target_for` returns `None`, and the caller fails closed and preserves the
   original `claude update` error.
3. **Network policy follows ADR-0010/0015**: packument and tarball go to `registry.npmjs.org` first;
   only when `community_mirror_can_help` judges a recoverable network failure, 403/regional
   restriction, or timeout, and the download policy is Automatic, is `registry.npmmirror.com`
   retried once for this operation. The HTTP client is the same one used by the product's other
   official endpoint queries, automatically carries the user-configured local proxy, and does not
   modify any global npm configuration.
4. **No mirroring of vendor binaries**: the product's own release storage continues to host only AI
   Manager's own signed updates; supply reads directly from the packages Anthropic publishes on the
   public registry.
5. **Remove "migrate to npm"**: `ToolInstallMigration`, `LifecycleRequest::MigrateInstallation`,
   `OperationKind::MigrateInstallation`, the launcher staging log and startup-time recovery, the
   `migration` field in the version catalog, the frontend `useMigrateToolInstallation`, and the
   corresponding copy are deleted wholesale, with no compatibility branch kept.
6. **Codex standalone installs enter native ownership**: the real binary under
   `~/.codex/packages/standalone/` is classified as `InstallSource::Native`; the update plan is
   `codex update` (`CODEX_NON_INTERACTIVE=1`) and, on failure, running the repository's
   `scripts/install/install.sh` with `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` forced to go
   through GitHub Releases; when not installed, npm follows a failed official script; uninstall
   removes the launcher and `~/.codex/packages/standalone`. Codex gets no registry supply: its
   official script itself works on restricted networks.
7. **The product's copy of the install script adds curl time limits**:
   `--connect-timeout 10 --max-time 120`, so that blocked domains fail within seconds and move on to
   the next option instead of waiting for the 900-second task timeout. Upstream `commands/misc.rs`
   is unchanged.
8. **New Rust dependency `tar = "0.4"`**: used only to read a single entry from the npm tgz.
   `tar 0.4.44` already exists in `Cargo.lock` as part of the tauri dependency tree, so no new
   transitive dependency is introduced; `flate2`, `sha2`, `base64`, `tempfile`, and
   `reqwest(stream)` are all existing dependencies.

## Consequences

- Users on restricted networks can click "Update" and have an officially installed Claude Code updated to the
  target version; install ownership, `claude update`, and the official uninstall layout are all unchanged, and
  users do not need to understand or choose a "channel".
- In that scenario the wait drops from "93 seconds of fruitless self-upgrade retries + download" to
  "10-second probe + download"; on a healthy network there is one extra HEAD round trip and the flow is otherwise
  unchanged.
- The product no longer maintains a launcher staging state machine; the only failure path is "new file deleted,
  launcher untouched".
- The supply-chain risk boundary is explicit: triple verification of registry integrity + Apple code signature +
  version re-check, stopping on any failure.
- Known limitations: Windows and Linux musl are not covered (fail closed, recorded as technical debt in
  `docs/ARCHITECTURE.md`); version-history rollback to an older native version still goes through the official
  update channel and is not yet wired to supply; Codex standalone installs on Windows have only the single
  `codex update` step.
