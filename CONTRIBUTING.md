# Contributing to AI Manager

Thank you for helping improve AI Manager. This guide covers the development
setup, the checks every change must pass, and the conventions the repository
follows. Read [AI_RULES.md](AI_RULES.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before writing code; the rules in
those files are enforced in review and, where possible, by CI.

## Before you start

- Search open and closed issues before filing a new one, and use the issue
  templates.
- Discuss a large feature in an issue before opening the pull request.
- Never put credentials, configuration secrets, private logs, or personal data
  in an issue, pull request, or commit.
- Report security vulnerabilities privately as described in
  [SECURITY.md](SECURITY.md).

## Development setup

Requirements:

- Node.js 20 or newer (CI uses 20; `.node-version` records the local version)
- pnpm 10.12.3, the version pinned by `packageManager` in `package.json`
- the Rust toolchain pinned by `rust-toolchain.toml`
- the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for
  your platform

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm dev            # desktop application (tauri dev)
pnpm dev:renderer   # renderer only, in the browser
```

## Required checks

Run the checks that cover your change, and the complete set before a
release-related review. CI runs all of them.

```bash
pnpm typecheck
pnpm lint
pnpm test:unit
pnpm format:check
pnpm check:boundaries
pnpm check:release
pnpm test:release
pnpm build:renderer

cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo test
```

The `NO_PROXY` variables matter whenever a system-wide HTTP proxy is
configured: `reqwest` honours the system proxy, which routes the proxy module's
requests to its local mock upstream through the external proxy and produces
spurious failures. See
[docs/development/BASELINE.md](docs/development/BASELINE.md).

`pnpm check:release -- --strict` is reserved for the protected release
workflow. Do not weaken it, or any other check, to make a pull request green.

## Architecture boundaries

The full rules are in [AI_RULES.md](AI_RULES.md); the ones most often hit in
review are:

- Data flows only along
  `UI -> hook -> src/native client -> Tauri command -> application -> domain -> adapter/repository`.
- Only `src/native/` may call Tauri `invoke`, `listen`, or `emit`.
- Only the repositories layer touches SQLite.
- Upstream code is used only through the `src-tauri/src/compat/ccswitch/`
  compatibility layer; it is never rewritten, moved, or merged wholesale (see
  [ADR-0001](docs/adr/0001-upstream-strategy.md)).
- Per-tool behaviour is driven by `ToolCapabilities`, not by
  `if tool === "claude"` conditionals.
- Shell commands run only on the Rust side through an argv-based `CommandSpec`
  and an allowlist; never build a command string.
- New dependencies, core dependency upgrades, and database schema changes need
  a written rationale or an ADR in `docs/adr/` before the code.
- Keep each change to one feature or phase. Fix violations in the code you
  touch and record the rest as technical debt instead of refactoring unrelated
  modules.

## Pull requests

1. Branch from `main` and keep one concern per pull request.
2. Add focused tests that fail without the change. Do not delete existing
   tests.
3. Keep React components under 250 lines and hooks under 200 lines unless an
   existing exception applies.
4. Update all four locale files for user-facing text:
   `src/i18n/locales/en.json`, `zh.json`, `zh-TW.json`, and `ja.json`.
5. Include screenshots or recordings for visible UI changes.
6. Never commit signing keys, API keys, tokens, private account data, or
   unredacted diagnostic output.

## Commits

Use small commits with
[Conventional Commits](https://www.conventionalcommits.org/) messages: `feat`,
`fix`, `chore`, `refactor`, `docs`, `test`, optionally with a scope.

```text
feat(services): add a provider action
fix(updater): reject a malformed manifest
docs: clarify the release gate
```

## Decisions and documentation

Architecture decisions live in [docs/adr/](docs/adr/). Add a new ADR when a
change alters a boundary, a data format, a dependency, or a product rule, and
mark superseded decisions instead of editing history. The product
specification is [docs/product/design-spec.md](docs/product/design-spec.md);
where it conflicts with other documents, it wins.

AI-assisted contributions are welcome. The author remains responsible for
understanding, testing, and reviewing every changed line.

## Licensing of contributions

AI Manager is distributed under the
[GNU AGPL-3.0-or-later](LICENSE). By opening a pull request you agree that your
contribution is licensed under those same terms. There is no CLA and no
copyright assignment: you keep the copyright to what you write.

Parts of this repository are derived from upstream work under the MIT License;
see [LICENSE-MIT](licenses/LICENSE-MIT) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the attribution, and
[ADR-0001](docs/adr/0001-upstream-strategy.md) for how that code is kept in
sync. Do not remove upstream copyright headers. AGPL-licensed changes cannot be
relicensed back under MIT, so a fix that belongs to the upstream project has to
be written there independently rather than copied out of this repository.

Do not paste code from sources whose license you have not checked, and do not
add a dependency under terms incompatible with the AGPL (for example SSPL,
BUSL, or any non-commercial or source-available-only license). New dependencies
need their rationale documented before the code, as described above.
