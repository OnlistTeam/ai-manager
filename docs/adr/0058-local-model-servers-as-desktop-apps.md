# ADR-0058: LM Studio and Ollama as desktop apps, and Windows apps found by their installer

- Status: accepted
- Date: 2026-09-27
- Extends: ADR-0009 (desktop app identities), ADR-0017 (vendor-owned
  install and uninstall), ADR-0027 (evidence before an identity enters the
  table).

## Context

ADR-0057 put LM Studio and Ollama on the Add endpoint page as local servers.
An endpoint pointing at `localhost` only answers while the app is running on
this computer, so a user picking one needs the app first. The Tools page is
where this product installs and opens things, and the owner asked for both
apps to be there.

Both are desktop apps with a stable identity, so they fit the desktop apps
section beside Cursor and Cherry Studio. They are not AI coding tools in the
sense of ADR-0027's R3.3, and the owner accepted that on 2026-09-27: they are
the other half of the local endpoints this product already offers.

On Windows the section only knew MSIX packages. Neither app ships one; both
use their own setup program. Cursor, ZCode and Cherry Studio have the same
gap and show as undetectable there.

## Official package forensics (2026-09-27)

Packages came from each vendor's own download entry: `lmstudio.ai/download`
and `ollama.com/download`, which redirects to the GitHub release. The Ollama
DMG was fetched through a GitHub download accelerator because the direct
route ran at 30 KB/s; its SHA-256 equals the digest GitHub publishes for the
release asset, so the file is the vendor's. Hashes identify the sample and do
not freeze a version.

| Sample                       | SHA-256                                                            | Verified identity                                                                                                                                                                                                                                        |
| ---------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| LM Studio 0.4.25+1 macOS DMG | `8bab857c3d2900e14ab46435f0d1471de068f165e761bb9c177093e8a25e1774` | `LM Studio.app`, bundle ID `ai.elementlabs.lmstudio`, executable `LM Studio`, arm64 only (no Intel build is offered), minimum macOS 12.0; Developer ID `Element Labs Inc (D65G88RHWN)`. `codesign --verify --deep --strict` and `stapler validate` pass. |
| Ollama 0.34.4 macOS DMG      | `3a32f8574920d432c39a480ab499d901c9a7c3ff0e823626d7bcf3632b1e3c4f` | `Ollama.app`, bundle ID `com.electron.ollama`, executable `Ollama`, universal (x86_64 + arm64), minimum macOS 14.0; Developer ID `Infra Technologies, Inc (3MU9H2V9Y9)`. `codesign --verify --deep --strict` and `stapler validate` pass.                |

Windows evidence: see "Windows install evidence" below.

## Decision

1. **Two standalone desktop apps.** `DesktopAppId` gains `lm-studio` and
   `ollama`, with no related CLI and `standaloneApplication`, like Cherry
   Studio. They get what every standalone app gets: inventory, local version,
   open, the vendor's download page in the browser, and the system uninstall
   handoff. Updates stay with the vendor; nothing is mirrored or downloaded on
   the user's behalf.
2. **macOS** uses the existing bundle check with the identities above.
3. **Windows apps installed by a setup program** get a second identity kind
   next to MSIX packages. An entry names three fixed values: the uninstall
   entry's key name, the executable relative to the install root that entry
   records, and the common name on that executable's Authenticode
   certificate. One constant PowerShell script reads the entry from the
   per-user hive and then the machine hive (both registry views), and counts
   the app as installed only when the executable exists and its signature is
   valid and from that signer. The values reach the script as environment
   variables, never in argv. Opening runs the same lookup again and starts
   only the executable it verified; no path crosses IPC. Uninstalling opens
   Windows' Installed apps settings, as for MSIX apps.
4. **Nothing is guessed.** No PATH, process, Start menu or folder-name search.
   An app whose Windows identity was not read from a real install stays
   undetectable there.
5. **Whether the server is running** is not shown. That would probe a port on
   every visit, and is a separate decision.

## Windows install evidence

To be recorded from a real install on the LAN test machine: the uninstall
key, `InstallLocation`, `DisplayVersion`, executable, and signer of each app.

## Consequences

Cursor, ZCode and Cherry Studio can use the installer identity on Windows as
soon as each has its own install evidence; nothing else is needed.
