# ADR-0026: Optional Anonymous Telemetry Stays Identity-Free, First-Party, and Explicitly Consented

- Status: Proposed — awaiting product decisions on whether to collect, the receiving endpoint, and retention periods
- Date: 2026-08-29

## Context

Design spec §60 allows anonymous collection of app version, OS, feature usage, Crash Code, install success, and
install failure error codes, while forbidding API Keys, Prompts, chat/configuration content, paths, and source
code. Roadmap item R5.7 wants to use it to validate the effect of the install/update network silent fallback on
real networks, but explicitly marks it "optional, needs product decision, ask by default + can be turned off". The
current product has no telemetry SDK, receiving endpoint, privacy retention period, or user consent state; sending
events directly while these conditions are missing would violate the local-first trust promise.

## Proposed Decision

1. The default state is `unknown` with zero requests. Consent is a separate, explicit "allow / do not allow"
   question, never bundled into another flow; the off switch and Settings remain permanently visible. `unknown`,
   `denied`, restoring an old backup, database recovery mode, and automated tests must never send.
2. Do not create persistent user/device/install IDs; do not collect IP/geolocation, account, language, time zone,
   Provider, model, endpoint, tool configuration, paths, command arguments, or free text. The server must not
   write source IPs into application logs or analytics tables; the handling period of edge access logs must be
   listed separately in the privacy statement.
3. Fixed event allowlist:
   - `tool_lifecycle_finished`: action category, stable ToolId, success/failure, stable error code, whether a
     connectivity fallback was attempted, whether the fallback succeeded;
   - `app_update_finished`: check/download/install stage, success/failure, and stable error code;
   - `feature_used`: only page-level features enumerated at compile time;
   - `crash_observed`: stable Crash Code, without panic text or stack traces.
4. Each batch attaches only app version, OS, CPU architecture, and schema version. Random event IDs are used only
   for network retry deduplication and kept at most 7 days; no cross-event correlation. The client queue has
   tightened permissions, at most 256 entries/128 KiB, first-in-first-out, and is deleted immediately when
   telemetry is turned off.
5. Reuse the existing Rust `reqwest`; no third-party Analytics SDK. The only receiving endpoint must be a
   product-owned first-party HTTPS origin approved by the owner; the renderer does not submit the endpoint, and
   remote config cannot change it. Failures are silently dropped or retried within bounds and never block install,
   update, exit, or startup.
6. Validate field by field in native before sending; unknown events/fields fail closed. Logs record only counts
   and stable results, never payloads. The product provides a static explanation of "which fields this machine
   will send" and an action to clear the queue, but does not expose internal network debugging information to
   beginners.
7. The server stores only allowlisted fields; raw events are suggested at 30 days and daily aggregates at 180
   days, deleted on expiry; no resale, ad profiling, or joining with account data. Concrete periods and processors
   must enter the privacy policy before release.

## Rejected Alternatives

- Third-party full-featured Analytics SDK: adds identity, automatic collection, and supply-chain surface,
  exceeding §60.
- On by default with an opt-out: no valid consent, and requests would occur before the user sees the switch.
- Inferring region from locale, time zone, or IP: violates the roadmap's region-neutral guardrail.
- Uploading error text/stack traces: may contain paths, configuration, or keys; stable error codes are sufficient.

## Pending Owner Decision

1. **Whether to enable telemetry:** recommended "enable the optional capability, but default unknown/zero
   requests"; long-term not doing it is also an option.
2. **First-party receiving endpoint:** a deployed, controlled HTTPS endpoint with minimized write permission must
   be provided; the repository must not invent a domain and treat that as the service existing.
3. **Retention periods and privacy text:** recommended raw 30 days, daily aggregates 180 days, with explicit
   edge-log handling.
4. **Whether ToolId is allowed:** recommended yes, because it is "Feature Used" and can distinguish different
   install chains; if rejected, only the action category is kept and analytical value drops noticeably.

Until the above is approved, no consent UI, queue, or network request is added, and R5.7 remains blocked on
product decision.
