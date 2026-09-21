# ADR-0012: Pin Exactly the Tauri Bundler That Supports the Byte Marker

- Status: Accepted; decision item 5 amended 2026-09-20
- Date: 2026-08-25

## Context

AI Manager's Windows updater needs Tauri to write the MSI or NSIS installer type into the main
executable. The current Rust runtime already reserves the fixed `__TAURI_BUNDLE_TYPE_VAR_UNK` bytes in
the release binary, but the project's `tauri-cli 2.8.x` still locates them via a PE symbol.
`strip = "symbols"` in `[profile.release]` removes the symbol this legacy lookup depends on, so the
cross-platform NSIS build produces a file yet explicitly warns that the updater may be unable to
determine the installer type.

Tauri CLI 2.11 switched to searching for and replacing the fixed bytes directly, no longer relying on
the PE symbol. The local Windows x64 cross-build on 2026-08-25 confirmed both facts: the old CLI
cannot find the symbol, yet the produced binary still contains the unmodified fixed marker bytes. The
problem is a mismatch between the bundler and the current Tauri runtime, not a fault in business code,
the Windows SDK, or NSIS.

## Decision

1. Pin `@tauri-apps/cli` exactly to `2.11.4` and commit the corresponding pnpm lockfile; releases do
   not resolve the bundler through a floating caret range.
2. The new CLI rejects minor-version drift between the Rust and JavaScript Tauri bridges. Align the
   JS API exactly to the currently pinned Rust minor: `@tauri-apps/api@2.10.1`,
   `plugin-updater@2.10.1`, `plugin-dialog@2.6.0`, `plugin-process@2.3.1`. Do not upgrade Rust
   `tauri`, Rust plugins, or business APIs; any later runtime minor upgrade must be audited and
   regression-tested separately.
3. Keep `strip = "symbols"`. The new CLI works by byte search and can patch correctly after symbols
   are stripped, preserving the current binary-size optimization.
4. The release contract test pins the CLI version; any occurrence of
   `__TAURI_BUNDLE_TYPE variable not found` in Windows QA/production build logs is treated as a
   failure and must not be ignored just because the installer file exists.
5. Windows production releases must let Tauri bundle in the order "write MSI marker → invoke the
   custom sign command to sign the EXE → embed into the MSI → sign the final MSI". Signing an
   unmarked EXE first and then handing it to the bundler is forbidden, because the subsequent byte
   modification would break Authenticode. CI must extract the EXE actually shipped inside the MSI,
   re-verify the signature, publisher, timestamp, and MSI marker, and complete N → N+1 validation of
   the installed updater; this ADR does not promote cross-compiled NSIS to production release evidence.

> Amended 2026-09-20: the owner decided that Windows ships without a platform code signature, so the
> signing half of item 5 is void; the release workflow configures no sign command at all. The bundling
> order, the MSI marker, the administrative extraction of the EXE actually shipped inside the MSI, the
> marker re-verification, and the N → N+1 installed-updater validation all stand unchanged.

## Consequences

- The current Mac can produce Windows x64 internal QA installers ahead of time and catch platform
  compilation problems, and the installer type marker is correctly recognized by the updater.
- Release tooling will not silently drift after a routine `pnpm install`, and the existing release
  size optimization is not abandoned to fix old tool behavior.
- Minor-version alignment between the CLI and the JS/Rust bridges is protected by the release
  contract; future runtime upgrades must re-run the three-platform build, signing, and updater
  regression.
- The Windows runner verifies only the patched EXE extracted from the MSI, and no longer mistakes the
  original unsigned EXE restored after Tauri bundling for the file users actually receive.
