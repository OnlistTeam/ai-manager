# ADR-0013: Enable a Fail-Closed Background Update State Machine Through the Product API

- Status: Accepted; decision item 6 amended 2026-09-20
- Date: 2026-08-26
- Extended by: ADR-0018

## Context

The product spec §61 requires the application itself to use the Tauri Updater and requires update
packages to be signed. Previously, to isolate the inherited CC Switch release identity, the product
shell did not register the updater plugin at all and merely returned "channel not configured" in the
frontend. That boundary prevented accidental connections to upstream, but it also meant that even
once the product release address was approved, the app could not offer a VS Code-like background
download and restart-when-ready experience.

The product already has a permanent Tauri updater public key and a protected private key; what
remains is the choice of a public endpoint, the first release, and three-platform N → N+1 evidence on
real machines. Implementing the update experience must not bypass these external release gates.

## Decision

1. Register the Tauri updater plugin, but allow only the Rust application layer to use it. The
   renderer gets no updater, process restart, or general opener capability, and does not call plugin
   commands directly.
2. Add three narrow product APIs: start/reuse the background check, read state, and install the
   verified update and restart. The standard data flow remains
   `UI → Entity Hook → src/native → Command → AppUpdateManager → Tauri Updater`.
3. `AppUpdateManager` sets `channelReady` to true only when the configuration satisfies all of the
   following:
   - `AI_MANAGER_UPDATE_CHANNEL_ENABLED=1` was explicitly set at build time by the protected
     production release workflow;
   - at least one endpoint;
   - all endpoints are HTTPS;
   - the product updater public key is non-empty;
   - the three dangerous switches (insecure transport, invalid certificate, invalid hostname) are all
     off.
     Without explicit enablement by a release build, with empty endpoints, or with any trust condition
     unmet, the state is fixed at `unconfigured`: no check request is created and neither install nor
     restart is possible. The repository may reserve the approved public URL, but ordinary
     local/development builds never access it.
4. The first mount of the product root starts exactly one shared background check without blocking
   the splash screen or main UI. The state machine is `idle → checking → downloading → ready`, and may
   also enter `upToDate / failed / unconfigured`. Multiple observers and the 500 ms state poll reuse
   the same backend task; failed and up-to-date are terminal states that only an explicit manual check
   by the user can restart, never automatic retries from polling or concurrent downloads.
5. The Tauri Updater completes minisign verification before `download()` returns. Only verified bytes
   and the corresponding `Update` metadata are held in process memory; the settings page shows
   "Restart and update" only after `ready`. An install failure keeps the verified content for retry.
   The user-configured credential-free local HTTP/HTTPS download proxy is used for both the manifest
   and the signed update package; proxies with credentials, remote addresses, or protocols the updater
   does not currently support are not handed to the updater, avoiding exposure of protected legacy
   proxy values through dependency logs.
6. Platform signing and updater signing remain two independent trust chains. macOS still requires
   Developer ID signing, notarization, and stapling; Windows still requires Authenticode signing and a
   timestamp. The public endpoint, first release, and N → N+1 acceptance still pass through the owner
   checkpoint per `RELEASE_RUNBOOK.md`; this document does not replace release authorization.

> Amended 2026-09-20: the owner decided that Windows ships without a platform code signature, so the
> Windows half of item 6 no longer holds. macOS Developer ID signing, notarization, and stapling are
> unchanged, and minisign remains mandatory on every platform: the two trust chains stay independent,
> and dropping the Windows platform signature does not weaken the updater's own verification.

## Consequences

- Test builds can fully verify the update UI, state, and safety boundaries while making zero update
  network requests when the release channel is not explicitly enabled.
- Once the endpoint is approved, only release configuration and signed artifacts are needed; upstream
  updater commands need not be exposed to the renderer.
- Downloaded content is currently kept in memory; quitting the app discards it and the next launch
  downloads again. If a future installer size makes the memory footprint unacceptable, a separate ADR
  can switch to a restricted temporary file inside the product AppData, with proof of permissions,
  cleanup, and restart recovery.
- The public URL is reserved but the root manifest has not been published; ordinary development
  builds remain "feature implemented, release channel not enabled".
