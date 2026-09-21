# ADR-0027: Add Cursor to the Catalog as a Standalone Desktop App, Enabling the Forensically Verified macOS Identity First

- Status: Accepted — Cursor explicitly approved on 2026-08-30; Windows/Linux detection continues to fail closed
- Date: 2026-08-29

## Context

R3.3 requires that "more desktop apps with AI coding semantics" be evaluated ADR by ADR while keeping the inventory
read-only. When research on this proposal began, `DesktopAppId` mainly served the CLI↔App relationship of
ChatGPT / Codex and Claude Desktop; standalone apps have since switched to a nullable `relatedTool` and
`standaloneApplication`, so Cursor does not need to fake a managed CLI identity.

As of 2026-08-29, Cursor's official download page explicitly offers macOS, Windows, and Linux packages and exposes
fixed download entries that differ by architecture/install scope, making it a better first extension than
candidates without a stable desktop identity. The official page does not define the macOS bundle ID/Team ID,
Windows signer/uninstall identity/default path, or Linux desktop entry/executable path as a stable public contract,
so this ADR additionally extracted candidate identities from the current 3.17.21 official packages. The official
Windsurf documentation entry had at that point already redirected to Devin Desktop content, showing that writing
fixed identities based solely on an old product name is highly prone to false positives.

## 2026-08-29 Official Package Forensics

All packages were resolved from the `api2.cursor.sh/updates/download/golden/.../3.17` entry of Cursor's official
download page; no third-party download sites were used. Hashes only prove this forensic sample and do not freeze
the vendor's current version or CDN path into a product update contract.

| Platform/Sample          | SHA-256                                                            | Verified identity                                                                                                                                                                                                                                                                                                                           |
| ------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS Universal          | `87d4b0a390e76d9013c7b5e3f70e332254ceea317ce663eca1dd022818ebfc40` | `Cursor.app` 3.17.21, minimum macOS 12.0, main binary contains both `x86_64`/`arm64`; bundle ID `com.todesktop.230313mzl4w4u92`, executable name `Cursor`; Developer ID `Hilary Stout (VDXQ22DGB9)`, Team ID `VDXQ22DGB9`. Both `codesign --verify --deep --strict` and `stapler validate` pass.                                            |
| Windows x64 User Setup   | `6d454037719fe845ba04ceb0544c3fa6e80d46a059caab94418f09b731f67ccf` | PE32 GUI Inno Setup 6.4.0.1; embedded Authenticode PKCS#7 table, signedData digest is SHA-256, signer cert serial `0x330004D1276C8601040BB792C600000004D127`, subject `Anysphere, Inc.`, issuer `Microsoft ID Verified CS EOC CA 03`. macOS can only verify structure/certificate chain and cannot substitute for Windows `WinVerifyTrust`. |
| Windows x64 System Setup | `140697101b6e19868b72824f81174481e603cca85b25bf6eedb11f3cd0d77f35` | Same installer type, signer serial, subject, and issuer chain as User Setup; different install scope. The final install root, uninstall registry entry, StartApps identity, and installed `Cursor.exe` signer must still be read on a Windows runner and cannot be inferred from Inno Setup conventions.                                    |
| Linux x64 deb            | `711474041a7a3b206754fe430f0882a0ecc618d819860787c737c102dc563d0c` | Debian package `cursor` 3.17.21-1787622916, `amd64`, Maintainer `Cursor <hi@cursor.com>`; ELF x86-64 main binary `/usr/share/cursor/cursor`, desktop `Exec` at the same path, icon `co.anysphere.cursor`, URL handler `cursor:`; postinst creates `/usr/bin/cursor -> /usr/share/cursor/bin/cursor`.                                        |

The in-package `product.json` also gives `applicationName=cursor`, `dataFolderName=.cursor`,
`win32AppUserModelId=Anysphere.Cursor`, `win32RegValueName=Cursor`, and
`darwinBundleIdentifier=com.todesktop.230313mzl4w4u92`. These values can serve as cross-evidence for the same
version, but the Windows values do not enter the production identity table until re-verified against the
registry/StartApps on a real machine. The deb itself has no additional Debian archive signature member, so Linux
production detection must rely on both package-manager ownership and fixed file identity, and this HTTPS download
hash must not be treated as a permanent signature.

## Decision

1. Choose Cursor as the first R3.3 candidate and add a stable `DesktopAppId::Cursor`; install state must still
   never be guessed from PATH, window titles, process names, fuzzy app names, or third-party download sites.
2. Package forensics is complete; production identity must additionally combine each platform's post-install
   system records:
   - macOS: bundle name, `CFBundleIdentifier`, `CFBundleExecutable`, TeamIdentifier, and signing chain;
   - Windows x64: after a real User/System install, re-verify the installer and installed EXE with
     `WinVerifyTrust`, then read the fixed install root, uninstall registry entry, and StartApps open semantics;
   - Linux x64: deb package name/architecture, desktop entry, Exec target, fixed file manifest; an AppImage at an
     arbitrary location is explicitly marked as not auto-discoverable, and the whole disk is not scanned.
3. Forensic values enter a single native identity table and, as in ADR-0009, the fixed path/system-registered
   identity is re-verified before opening. The renderer still submits only the stable `DesktopAppId`; paths,
   download URLs, signing subjects, and system app IDs do not cross IPC.
4. Cursor uses the existing nullable `DesktopApp.relatedTool` and `standaloneApplication` configuration
   relationship. The Cursor card does not fake shared configuration with CLIs such as Codex/Claude and does not
   open Provider/MCP write capabilities.
5. Initial capabilities include only inventory, local version, safe open, the fixed official download page, and
   system uninstall handoff. Vendor auto-update stays with the vendor; no proxy downloads, no mirroring, no
   comparison of Cursor's remote version, no takeover of extensions/accounts or editor configuration.
6. Four-language copy and tests must cover standalone, not installed, incomplete identity, AppImage not
   discoverable, and identity change before opening. Official Windows behavior must wait for runner evidence;
   Linux must re-verify the postinst identity with an actual dpkg install/uninstall; macOS enters the production
   catalog with the signed, notarized sample above and re-checks bundle name, executable name, and bundle ID
   before opening.

## Current Conclusion

The owner explicitly accepted the Cursor scope on 2026-08-30. The macOS fixed identity, four-language card,
official icon, official download handoff, and pre-open re-verification have been implemented. `WinVerifyTrust`,
registry, StartApps, and actual install root evidence for Windows User/System installs is still missing; Linux has
also not completed the actual dpkg install/uninstall re-verification. Therefore these two platforms continue to
show as undetectable while keeping the fixed official download entry. Capabilities are expanded separately once
the evidence arrives; identity trustworthiness is not sacrificed for "a larger catalog count".

## Official References

- Cursor download page: <https://www.cursor.com/downloads>
- Windsurf official documentation entry (current content has changed and needs re-evaluation):
  <https://docs.windsurf.com/windsurf/getting-started>
