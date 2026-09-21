# ADR-0009: Keep desktop applications separate from CLI lifecycle

- Status: Superseded in part by ADR-0017
- Date: 2026-08-24

## Context

CC Switch exposes Claude Desktop as a configuration surface, while AI Manager's
product `ToolId` registry describes ten command-line tools. A desktop
application and its related CLI can share some configuration without sharing an
installation, version, updater, signature, or launch mechanism. Treating them as
one lifecycle produced misleading update actions; treating a downloaded CLI
package as a desktop app also conflicts with platform signing and regional
distribution constraints.

The official products use platform-owned installation channels. OpenAI publishes
desktop downloads and a Microsoft Store identity; Anthropic publishes separate
desktop installers and Linux packages. Their availability and update behavior can
change independently from npm-installed CLIs.

## Decision

1. Introduce a separate, stable `DesktopAppId` registry for `codex-app`,
   `claude-desktop`, `cursor`, `zcode`, and `cherry-studio`. Do not add these desktop
   identities to the ten-CLI `ToolId` registry.
   `codex-app` is a compatibility wire ID for OpenAI's desktop surface: current
   product copy calls it ChatGPT / Codex, while already installed signed builds may
   still use the `Codex` bundle and package identity.
2. Expose only a safe read model: identity, name, installed state, bounded local and
   optional official-latest versions, optional related CLI, configuration relationship,
   launch capability, and platform. Bundle paths, package-family names, registered app
   IDs, update-source URLs, and commands never cross product IPC.
3. Detect only fixed native identities:
   - macOS: the current `ChatGPT.app` identity plus the signed `Codex.app`
     compatibility identity, and `Claude.app`, in system or user Applications,
     each with its expected bundle executable; `Cursor.app` must identify as
     `com.todesktop.230313mzl4w4u92`, `ZCode.app` as `dev.zcode.app`, and
     `Cherry Studio.app` as `com.kangfenmao.CherryStudio`;
   - Windows: the official `OpenAI.Codex` / `Claude` MSIX package names and the
     operating system's registered StartApps identity;
   - Linux: the official `chatgpt` / `claude-desktop` executable names in bounded
     standard locations. Cursor, ZCode, and Cherry Studio remain unsupported on
     Windows and Linux until audited installed-system identities are available;
     their fixed official download pages remain accessible.
4. Launch through `CommandSpec` and the native allowlist. Renderer input is only a
   `DesktopAppId`; the backend re-inspects the current native identity immediately
   before handoff. Dynamic Windows values stay in environment variables and are
   validated before use, never interpolated into PowerShell source.
5. Keep installation and updates with each vendor's signed official channel. The
   original inventory-only release opened a fixed vendor landing page and did not
   download, install, update, replace, or mirror either proprietary desktop app.
   ADR-0017 later replaces only this handoff detail with architecture-specific,
   vendor-owned package URLs and system-owned interactive installation. The stable
   `DesktopAppId`, native URL ownership, no-mirror rule, and separation from CLI
   lifecycle remain unchanged.
   AI Manager may read OpenAI's official Codex App updater metadata only after a
   local installation is confirmed. The read is proxy-aware, limited to six seconds
   and 512 KiB, and accepts only strict numeric versions from the fixed macOS appcast
   or Windows Store-version JSON identity. `updateAvailable` is returned only when
   that version is provably newer. A timeout, malformed response, or unreachable
   source leaves the latest version unknown and must never be presented as “current”.
   This exception is update awareness, not ownership of the update operation.
6. Distinguish an ordinary connectivity failure from an explicit vendor region or
   account restriction. Users may retry the official page with their browser or
   system proxy, but AI Manager does not bypass a vendor restriction or silently
   switch to a third-party distribution channel.
7. Show the relationship explicitly: OpenAI's ChatGPT / Codex desktop surface
   shares Codex configuration with Codex CLI, while Claude Desktop and Claude Code
   are separate configuration surfaces. Cursor, ZCode, and Cherry Studio are
   standalone desktop applications with no invented related `ToolId`. These facts
   drive copy only; they do not merge state.

## Consequences

- Users can see and safely open installed desktop apps, or reach the fixed official
  download page, without confusing either action with CLI packages or CLI updates.
- An outdated Codex App can show a proven official update without claiming that an
  unreachable update source means the local version is current.
- Regional download failures and unsigned/unnotarized artifacts cannot be hidden
  behind an automatic desktop-app install fallback.
- Windows and Linux plans have unit evidence, but their final launch behavior still
  requires signed-runner or physical-machine release evidence.
- Adding another desktop app requires one centralized native identity entry, wire
  schema coverage, four-language copy, and platform evidence; UI action logic stays
  status-driven.

## Primary references

- OpenAI desktop app documentation: <https://learn.chatgpt.com/docs/app>
- OpenAI Windows app documentation: <https://learn.chatgpt.com/docs/windows/windows-app>
- OpenAI Linux app documentation: <https://learn.chatgpt.com/docs/linux/linux-app>
- Anthropic desktop download page: <https://claude.com/download>
- Anthropic desktop installation documentation: <https://support.claude.com/en/articles/10065433-install-claude-desktop>
- OpenAI Codex App update feed: <https://persistent.oaistatic.com/codex-app-prod/appcast.xml>
- Cursor official download page: <https://www.cursor.com/downloads>
- ZCode official page: <https://zcode.z.ai/cn>
- Cherry Studio official download page: <https://cherryai.com.cn/download>
