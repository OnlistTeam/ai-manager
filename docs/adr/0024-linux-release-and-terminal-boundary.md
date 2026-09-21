# ADR-0024: Linux x64 Ships as AppImage/deb Dual Distribution with an argv Terminal Boundary

- Status: Accepted
- Date: 2026-08-28
- Extends: ADR-0013, ADR-0018

> 2026-09-10: macOS adds a user-selectable terminal, following the shape of decision 5. The system Terminal and
> iTerm2 continue to go through AppleScript (the script is a compile-time constant; dynamic values are passed item
> by item as argv); Ghostty / kitty / WezTerm / Alacritty go through
> `open -na <App> --args [flags] /usr/bin/env -C <directory> [PATH=…] <tool-path> <argv…>` — likewise no temporary
> scripts and no `sh -c`. The candidates are a fixed allowlist; when the selected terminal is not on this machine,
> launch falls back to the system terminal instead of failing.

## Context

The product backend and ordinary CI already cover Linux, but the protected release contract originally allowed only
macOS arm64, macOS x64, and Windows x64 by design; `latest.json` also had only three updater platforms. The old
Linux-side compatibility code can open several terminals with temporary shell scripts, but the new product
architecture's `TerminalLauncher` explicitly rejects Linux, so the old script path cannot be treated as a product
capability.

Linux has no unified platform signing chain equivalent to Apple Developer ID or Windows Authenticode that covers
arbitrary AppImage/deb downloads. The Tauri updater uses a minisign-signed `.AppImage.tar.gz` for AppImage;
manually downloaded AppImage and deb packages need independent integrity records and a protected build provenance.
The first slice also has no install, uninstall, or N → N+1 update evidence on a real distribution, so the code
contract and the external release acceptance must be stated separately.

## Decision

1. The first Linux release target is fixed to `x86_64-unknown-linux-gnu`, built on the `ubuntu-22.04` runner to
   maintain an explicit glibc baseline. Public installers include only AppImage and deb; ARM64, rpm, Flatpak, AUR,
   and distribution repositories are not part of this ADR, and the release contract continues to reject drift
   toward those targets.
2. The protected workflow uses the same product updater private key to let Tauri generate the AppImage updater's
   `.AppImage.tar.gz.sig`. Only after the aggregation machine actually performs minisign verification with the
   product public key in the repository may `latest.json` include `linux-x86_64`, whose URL points fixedly at the
   AppImage updater tarball. Mirrors may only keep the signature and rewrite URLs under approved domains.
3. Manual distribution names are fixed to `AI-Manager-<version>-Linux-x64.AppImage` and
   `AI-Manager-<version>-Linux-x64.deb`, each with a SHA-256 record. The build job verifies that the AppImage is
   executable and an x86-64 ELF, verifies the deb's package name, architecture, and its `usr/bin/ai-manager`, and
   unpacks the updater tarball to compare byte by byte with the original AppImage. SHA-256 only proves download
   integrity and does not impersonate publisher identity; Linux packages carry no additional platform code-signing
   guarantee.
4. Release payloads, manifests, and distribution mirrors continue to fail closed: missing, extra, or empty files,
   wrong checksums, wrong signatures, external repository URLs, or platform aliases all block the release. The
   Linux audit file records only structure and architecture and contains no signing private key.
5. `canLaunch` opens for Linux, but an actual launch still requires a detected anchored tool path and an absolute
   working directory. The platform layer only tries the fixed system paths of `xdg-terminal-exec`,
   `gnome-terminal`, `konsole`, and `x-terminal-emulator`; it uses the fixed
   `/usr/bin/env -C <directory> [PATH=…] <tool-path> <argv…>` as the terminal execution argv, generates no
   temporary scripts, does not call `sh -c`, and does not splice user directories or session parameters. When no
   supported terminal exists it fails explicitly and guides manual opening instead of guessing arbitrary terminal
   programs.
6. Code completion of R4.1 does not equal release completion. At least one clean, supported Linux x64 host must
   separately verify download, install/launch, terminal handoff, uninstall, data retention, and signed N → N+1
   update for both AppImage and deb; this evidence records the exact version, hash, distribution, and architecture
   just as for macOS/Windows.
7. This slice adds no Rust/JavaScript dependencies, changes no database schema, and does not reuse the old
   compatibility layer's shell launch functions.

> Amended 2026-09-21: Tauri 2.10 signs the AppImage in place and no longer produces an
> `.AppImage.tar.gz`, so the archive described in the Context and in item 2 does not exist. The Linux
> update payload is the AppImage itself, accompanied by `<name>.AppImage.sig`; the release job proves
> the signature accompanies the installer instead of unpacking a tarball and comparing it byte by byte,
> which is what item 3 previously required. Everything else about item 2 stands: the same product key
> signs it, and `latest.json` may carry `linux-x86_64` only after the aggregation machine verifies that
> signature with the repository's public key.

## Consequences

- The protected release and the two product distribution sources can now express four exact updater platforms and
  simultaneously offer Linux AppImage/deb downloads.
- Linux tool launch enters the same typed Application → Platform boundary as macOS/Windows, with dynamic values
  kept as argv.
- The trust statement for Linux packages is not overstated: automatic updates are authorized by minisign, manual
  packages get integrity/provenance evidence from checksums and the protected build, but there is no fictional
  OS-level publisher signature.
- Linux ARM64, rpm, and real clean-machine release/update evidence remain explicit follow-up items.
