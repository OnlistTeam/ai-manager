# ADR-0007: Advanced Routing Opens Only a Controlled Local Routing Boundary

- Status: Accepted
- Date: 2026-08-24

> 2026-08-30: ADR-0031 supersedes this document's Advanced Mode navigation gate; the controlled local routing and IPC safety boundaries are unchanged.

## Context

CC Switch already has a local proxy takeover, provider hot-switching, health records, and an automatic
failover data plane, but its raw command surface also includes listen addresses, circuit-breaker
parameters, request logs, error bodies, full provider records, live-config backups, and reset controls.
Restoring those commands directly would expand the product IPC and could send credentials, paths, or
request content to the renderer. Meanwhile, tool installation and updates on restricted networks
suffer from two distinct failure classes: the npm registry being unreachable, and official non-npm
download channels such as Claude's native updater being unreachable; a single registry-mirror policy
cannot solve both.

## Decision

1. Advanced Mode gains a top-level Local Routing entry that connects only Claude Code, Codex, Gemini
   CLI, and Grok Build, the tools for which the inherited engine already has a complete data plane.
   The backend re-validates Advanced Mode rather than relying solely on the frontend hiding navigation.
2. The product registers only seven routing commands: read the aggregate overview, toggle per-tool
   takeover, toggle automatic failover, add/remove queue entries, provider hot-switch, and
   stop-all-and-restore. All write operations share a product-level async lock.
3. Enabling takeover requires user confirmation; enabling failover requires takeover to be active and
   switches to a qualified P1 before persisting the toggle. An empty queue may auto-fill P1 with the
   current provider; removing the active provider or the last qualified provider fails closed. Stopping
   all routing requires a second confirmation and reuses upstream's ability to restore the live
   configuration.
4. The Domain wire format contains only running state, loopback endpoint, aggregate counts, provider
   ID/name, queue priority, and a health summary. Credentials, provider notes, raw
   requests/responses/errors, file paths, live configuration, and backups never cross the product IPC.
5. Install/update networking keeps a separate, small boundary: an npm attempt may perform one
   process-level mirror retry after the user explicitly opts in; non-npm official channels only reuse
   the user-configured download proxy. Proxies newly written by the product are limited to
   credential-free localhost/IPv4/IPv6 loopback addresses, an explicit port, and HTTP(S)/SOCKS5(H)
   schemes. Inherited sensitive or remote proxies remain usable but are projected only as a protected
   state.

## Consequences

The 2026-09-20 UI follow-up uses the shared software scope tabs to show one
supported routing target at a time, alongside the global gateway summary. Targets
come from the native routing capability projection, not from an endpoint-provider
list: endpoint management does not imply proxy takeover support. Switching a tab
does not change live configuration. Tabs are locked during mutations/confirmation
so a pending action cannot appear to target a different tool.

The gateway summary is a compact text row, not a hero card: inactive routing
shows only its status; running routing adds the local address and a stop/restore
action. Request counters remain available in a collapsed statistics disclosure,
including nonzero history after stopping. A stopped gateway with outstanding
takeovers retains a visible restore warning and action. Backup/restore details
remain in the explicit takeover and stop confirmation dialogs.

- Positive: reuses CC Switch's mature data plane while keeping the product API a small, auditable,
  secret-free projection.
- Positive: users behind an unreachable channel can handle npm registry and native official
  update-channel problems separately, without changing global npm configuration or installation
  ownership.
- Positive: the dangerous boundaries of takeover, failover, and restore have explicit confirmation,
  write locks, ordering constraints, and read-back results.
- Negative: the first version provides no arbitrary listen/circuit-breaker parameters, per-request
  logs, diagnostic reset, or per-provider proxy, and does not fabricate nonexistent upstream routing
  capabilities for the other four CLIs.
