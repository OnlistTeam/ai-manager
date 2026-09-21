<div align="center">

<img src="src/assets/spatial/models/v5/mark.png" alt="AI Manager" width="128" height="128">

# AI Manager

### A calm desktop manager for AI coding tools

</div>

AI Manager installs, updates, configures, and repairs AI coding command-line
tools on your own computer, so you never have to touch shell profiles, `PATH`,
or hand-edited configuration files. It is a local-first desktop application
built with Tauri 2 and React 18, released under the GNU AGPL-3.0-or-later.

> [!NOTE]
> AI Manager is in release hardening. The current build is the `v0.1.0-1`
> prerelease on the
> [Releases page](https://github.com/OnlistTeam/ai-manager/releases); GitHub
> reserves "Latest release" for stable versions, so a prerelease does not
> appear there or under `/releases/latest`. Every installer passes the signing
> and verification gates in the
> [release runbook](docs/development/RELEASE_RUNBOOK.md) before it is published.
> A local build is not a release and must not be redistributed as one.

## Who it is for

People who use Claude Code, Codex, or a similar tool every day and would
rather not manage npm ownership, configuration files, and API endpoints by
hand. One place to see which tools are installed, which AI service each one
is really using, and whether that setup still works.

## What it does

Seven destinations in a flat sidebar, with Settings in the footer.

**Home** summarises what is installed, what has an update, and what needs
attention. Quick Check is built from real local signals: it rechecks saved
service addresses only when you ask, and never claims a process is healthy
when no such signal exists.

**Software** detects, installs, updates, and repairs ten command-line tools
without crossing away from the npm, pnpm, bun, or Volta owner that already
manages them. It keeps a verified version history and shows bounded, redacted
process logs instead of raw shell output. Downloads are official-first, with
one process-scoped retry on a recoverable network failure and a local proxy
option in Settings. Desktop apps are detected separately; installing and
updating those stays with each vendor's signed channel.

**API Endpoints** connects a tool to an AI service from a reviewed preset
catalogue or a custom HTTPS endpoint, then shows the effective connection:
the endpoint the tool will really use, and why, including environment
variable overrides. Your shell environment is never edited. Local Routing and
a 30 day local usage overview are secondary tabs.

**Skills** scans every supported local scope in one overview, installs from
trusted GitHub catalogues or a local ZIP, copies a Skill to another tool, and
keeps recovery copies before removal.

**MCP** adds stdio, HTTP, and SSE connections without pasting JSON, and
enables or disables them per tool. Adoption of a tool's existing servers
never rewrites live configuration.

**Global Prompts** creates, edits, imports, and switches prompts without
opening a configuration file, and reveals where each one lives.

**Sessions** browses and searches local sessions. Content loads only after
selection, and supported sessions resume in a terminal through a command line
built in the backend.

**Settings** holds language and appearance, desktop behaviour, the local
download proxy, database backups, configuration import and export, and the
fail-closed signed updater.

## Supported tools

| Command-line tools     | Desktop apps   |
| ---------------------- | -------------- |
| Claude Code            | Codex App      |
| Codex                  | Claude Desktop |
| OpenCode               | Cursor         |
| Gemini CLI             | ZCode          |
| Grok Build             | Cherry Studio  |
| OpenClaw               |                |
| Hermes                 |                |
| Pi                     |                |
| Kimi Code              |                |
| DeepSeek Harness (DSH) |                |

Per-tool behaviour is driven by a capability registry rather than scattered
conditionals, so a tool that lacks a concept (for example a "current service")
simply does not show that control.

## Install

Download the installer for your platform from the
[Releases page](https://github.com/OnlistTeam/ai-manager/releases).

| Platform            | Installer                     | Automatic updates        |
| ------------------- | ----------------------------- | ------------------------ |
| macOS Apple Silicon | Signed, notarized DMG         | Signed app archive       |
| macOS Intel         | Signed, notarized DMG         | Signed app archive       |
| Windows x64         | MSI, verify the SHA-256       | Minisign-signed MSI      |
| Linux x64           | AppImage and deb with SHA-256 | Minisign-signed AppImage |

Windows installers are not Authenticode-signed, so SmartScreen shows an unknown
publisher warning on first run; check the published SHA-256 before installing.
Linux has no cross-distribution equivalent of platform signing either. On both,
the minisign signature on updates and the published checksum are the integrity
evidence.

Every release asset ships with a checksum, and the in-app updater verifies a
minisign signature against the public key embedded in the application before
anything is installed. Linux ARM, Windows ARM, rpm, universal macOS builds, the
Mac App Store, and the Microsoft Store are outside the current release scope.

## Build from source

Prerequisites:

- Node.js 20 or newer (`.node-version` records the version used locally)
- pnpm 10.12.3, the version pinned by `packageManager` (`corepack enable`)
- the Rust toolchain pinned by `rust-toolchain.toml`
- the platform packages from the
  [Tauri 2 prerequisites guide](https://v2.tauri.app/start/prerequisites/)

```bash
pnpm install --frozen-lockfile
pnpm dev            # desktop application with hot reload (tauri dev)
pnpm dev:renderer   # renderer only, in the browser (vite)
pnpm build          # production bundle for the current platform (tauri build)
```

The design-system gallery is development-only at
`http://localhost:3000/?gallery` and is excluded from production bundles.

Before opening a pull request, run the checks listed in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Project structure

```text
src/             React renderer: app shell, pages, features, entities, shared UI,
                 the native IPC client (src/native), and locales (src/i18n)
src-tauri/src/   Rust backend: commands, application, domain, adapters,
                 repositories, platform, infrastructure, and the
                 compat/ccswitch compatibility layer
docs/            ARCHITECTURE.md, ADRs, the product design specification, and
                 the release runbook
scripts/         release, brand, and boundary tooling
tests/           Vitest suites for the renderer
```

Start with [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the layer model and
directory boundaries, [docs/adr/](docs/adr/) for the decisions behind them, and
[docs/product/design-spec.md](docs/product/design-spec.md) for the product
specification. [AI_RULES.md](AI_RULES.md) lists the non-negotiable coding rules
that apply to every contributor.

## Privacy and data safety

- AI Manager contains no analytics or telemetry SDK.
- All product data lives in the application's local data directory. An
  existing upstream database is only ever read, never modified.
- Logs and crash records are size-bounded and redact known secret forms.
- Provider credentials are still stored in plaintext inside the local database,
  local backups, and SQL configuration exports. Treat those files as sensitive;
  an OS secret store and an encrypted portable archive are planned but not
  implemented yet.

## Upstream

AI Manager began as a fork of
[CC Switch](https://github.com/farion1231/cc-switch), copyright 2025 Jason
Young, [MIT License](licenses/LICENSE-MIT). Inherited code keeps its original paths so
that upstream fixes can still be cherry-picked, and everything this project
added sits beside it in the product layers. See
[ADR-0001](docs/adr/0001-upstream-strategy.md),
[ADR-0003](docs/adr/0003-compat-layer-logical-isolation.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## License

[GNU AGPL-3.0-or-later](LICENSE). Copyright 2026 AI Manager contributors.

Portions derived from the upstream project remain under their original
[MIT License](licenses/LICENSE-MIT), copyright 2025 Jason Young. MIT permits
sublicensing, so the combined work is distributed under the AGPL with the
upstream notices preserved in full.
