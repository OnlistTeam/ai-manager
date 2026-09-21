# Phase 0 Baseline

- Date: 2026-08-18
- Upstream: CC Switch v3.19.2, commit `fd14f9c4` (`upstream/main`)
- Host: macOS on Apple Silicon

## Toolchain

| Tool | Version                                                                    |
| ---- | -------------------------------------------------------------------------- |
| Node | 20 or newer (CI uses 20)                                                   |
| pnpm | 10.12.3 (pinned by `packageManager`)                                       |
| Rust | 1.95 (pinned by `rust-toolchain.toml`; minimal profile + rustfmt + clippy) |

## Baseline results (no product code modified)

| Check                                    | Result                                                                          |
| ---------------------------------------- | ------------------------------------------------------------------------------- |
| `pnpm install --frozen-lockfile`         | passed                                                                          |
| `pnpm typecheck`                         | passed                                                                          |
| `pnpm test:unit` (Vitest)                | passed, 131 files / 987 tests                                                   |
| `pnpm build:renderer` (Vite)             | passed (chunk-size warnings inherited from upstream)                            |
| `cargo test` (src-tauri)                 | passed, 2679 tests / 5 ignored (see the note below)                             |
| Upstream CI (`.github/workflows/ci.yml`) | already gates typecheck / format / Vitest / cargo fmt / clippy / test; retained |

## Local environment notes

- **Run `cargo test` with `NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost`
  and with `ALL_PROXY`/`all_proxy` unset, whenever a system-wide proxy is
  configured.** `reqwest` 0.12 reads the operating-system proxy settings, so the
  proxy module's requests to its local mock upstream are routed through the
  external proxy, and `proxy::hyper_client::bytes_with_limit_aborts…` and
  `proxy::server::alpha_search_routes…` fail with a spurious 502.

  `NO_PROXY` alone is not enough. `ALL_PROXY` is consulted separately and is not
  narrowed by the no-proxy list, so with a SOCKS proxy exported
  `services::speedtest::tests::batch_probe_preserves_order_and_sends_no_auth_headers`
  still fails, and it fails intermittently enough to look like a flake. The full
  invocation is:

  ```bash
  env -u ALL_PROXY -u all_proxy \
    NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo test
  ```

- If `rustup`, `pnpm`, or `git` need a proxy to reach the network, set it in
  the shell for those commands only; never commit proxy settings.
- Running a bare `rustc --version` while the pinned toolchain is not installed
  makes rustup download it silently and hang without network access. Run
  `rustup toolchain list` first and confirm 1.95 is installed.

## Not executed

- `pnpm tauri build` (full application bundle and DMG): slow. Phase 0 used the
  renderer build plus a compiling `cargo test` as build evidence; full
  packaging is verified by the release workflow.
- Windows build: the baseline host was macOS, so the Windows baseline is
  verified by CI or on a Windows machine.
