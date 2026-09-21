# ADR-0005: Expand the Product CLI Registry to Ten Lifecycle Tools

- Status: Accepted
- Date: 2026-08-23
- Supersedes: the scope decision in ADR-0004 that "long-tail CLIs do not enter the Domain registry"

## Context

ADR-0004 narrowed the product registry to Claude Code, Codex, OpenCode, and Gemini CLI for the
first non-technical-user flow. The product positioning has since been adjusted to cover CC Switch's
set of commonly used tools; the upstream unified version detection and lifecycle engine actually
maintains eight CLIs: Claude Code, Codex CLI, Gemini CLI, Grok Build, OpenCode, OpenClaw, Hermes,
and Pi. On 2026-08-30 the product further added Moonshot AI's Kimi Code and DeepSeek's DeepSeek
Harness (DSH). Both have a verifiable official CLI lifecycle but are not in CC Switch's configuration
`AppType` enum.

Claude Desktop, Codex App, ZCode, and Cherry Studio are not four more CLIs that can be independently
maintained via npm. Claude Desktop is a separate configuration surface; Codex App shares `~/.codex`
with Codex CLI; ZCode and Cherry Studio are standalone desktop applications. Disguising them as the
same set of install, update, and uninstall actions would misrepresent state and action ownership.

## Decision

1. The product `ToolId` registry is expanded to the ten CLIs above, continuing to use stable strings and capability-driven UI.
2. The facade maintains the product `ToolId`, the upstream configuration `AppType`, and the actual
   CLI executable name separately. Grok Build's `AppType` is `grokbuild` while its executable name is
   `grok`; the two must no longer share one ambiguous mapping. Kimi Code and DSH have an explicitly
   empty `AppType`; they cannot borrow another tool's provider configuration format.
3. The lifecycle prefers upstream's validated installation sources and order; the product layer
   continues to execute with array arguments, allowlists, anchored package managers, operation locks,
   streamed redacted logs, and failure classification, and does not restore upstream's string-based
   shell entry point.
4. A capability is opened only for actions whose installation source can be safely attributed. When
   uninstall ownership cannot be reliably determined, it is better not to show automatic uninstall
   than to guess paths.
5. The relationship between Claude Desktop, Codex App, ZCode, Cherry Studio and the CLIs is presented
   separately as "usage surfaces"; they get no fake `ToolId` and do not share incorrect lifecycle state.

## Consequences

- Positive: the tools page covers CC Switch's real eight-CLI baseline plus two lifecycle tools with
  official evidence; the worldwide network auto-fallback, logging, and recovery strategy covers all
  npm-compatible installation paths.
- Positive: Grok's configuration identifier and binary identifier are no longer confused; App/CLI
  state can stay truthful.
- Negative: with a larger registry, every exhaustive match, icon, i18n entry, path, and command
  allowlist needs synchronized tests; some upstream advanced configuration capabilities still require
  separate adaptation before their capability can be opened.
