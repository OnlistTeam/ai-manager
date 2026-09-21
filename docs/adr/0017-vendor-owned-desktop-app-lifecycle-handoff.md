# ADR-0017: Desktop App lifecycle uses vendor packages and system-owned handoff

- Status: Accepted
- Date: 2026-08-27
- Supersedes: ADR-0009 decision 5 only

## Context

ADR-0009 intentionally limited ChatGPT / Codex and Claude Desktop to inventory,
launch, and fixed vendor landing pages. That prevented AI Manager from pretending
that a CLI package was a desktop App, but it also left ordinary users to identify
their operating system and CPU architecture on a download page.

The vendors now publish stable official package entry points:

- OpenAI publishes an Apple Silicon DMG and Store-signed Windows x64 / Arm64 MSIX
  packages. The Windows app also supports Microsoft Store / `winget` deployment.
- Anthropic publishes a Universal macOS PKG and Windows x64 / Arm64 MSIX packages.
- Both desktop apps own their normal automatic update behavior.

The packages are large, proprietary artifacts. Mirroring them on the product's
own release storage, silently installing them, or inventing a historical-version
catalog would make AI Manager an unsupported distribution and update authority.

## Decision

1. Keep `DesktopAppId` separate from the eight-CLI `ToolId` registry. Desktop App
   state and actions remain capability-driven and never reuse CLI lifecycle state.
2. Native code owns a fixed allow-list of vendor URLs and chooses the target from
   the current OS and architecture. The renderer submits only `DesktopAppId`; it
   never submits a URL, path, package name, architecture, or command.
3. On supported macOS and Windows targets, the install and recovery-update action
   opens the architecture-correct package on the vendor's official HTTPS host.
   The browser downloads it and the visible macOS disk-image/package flow or
   Windows App Installer performs the interactive, signature-enforcing install.
   AI Manager does not click through UAC, run a silent install, or report success
   before a later inventory refresh observes the fixed native identity.
4. An installed App keeps its vendor-managed automatic updater. “Download latest
   installer” is a visible recovery path, not an independent update detector and
   not proof that a newer version exists.
5. Uninstall is a system-owned handoff. macOS reveals the fixed inspected bundle in
   Finder; Windows opens Installed Apps. AI Manager does not recursively delete an
   App bundle, package registration, settings, conversations, caches, helpers, or
   virtualization services.
6. Version rollback stays unavailable. Neither vendor exposes a supported public
   historical-version catalog for ordinary users. A numerically older package is
   never sourced from a third party or accepted as a renderer-provided path.
7. Ordinary connectivity failures may use the browser/system proxy. Explicit
   vendor region or account restrictions are not bypassed. No proprietary package
   is copied to the product's own release storage or a community registry.
8. Unsupported platform/architecture combinations fall back to the fixed official
   landing page and say so honestly; they do not receive a guessed binary.

## Consequences

- Users no longer need to choose x64 versus Arm64 for supported Windows packages,
  and Claude's Universal macOS package covers both Intel and Apple Silicon.
- Installation remains visible, cancellable, and verified by the operating system.
- The UI can offer install, latest-installer recovery, and safe uninstall handoff
  without claiming that AI Manager owns the vendor updater.
- A future in-app downloader would require a separate ADR covering resumable large
  files, disk quotas, redirect allow-lists, package identity/signature verification,
  cleanup, cancellation, Operation Manager integration, and proxy semantics.

## Primary references

- OpenAI ChatGPT desktop app: <https://developers.openai.com/codex/app>
- OpenAI Windows deployment: <https://developers.openai.com/codex/enterprise/windows-deployment>
- OpenAI managed updates: <https://developers.openai.com/codex/enterprise/manage-app-updates>
- Claude Desktop installation: <https://support.claude.com/en/articles/10065433-install-claude-desktop>
- Claude Windows deployment: <https://support.claude.com/en/articles/12622703-deploy-claude-desktop-for-windows>
- Claude macOS deployment: <https://support.claude.com/en/articles/12611117-deploy-claude-desktop-for-macos>
