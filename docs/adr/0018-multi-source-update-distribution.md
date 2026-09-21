# ADR-0018: Signed Updates from One Product Channel, an Isolated RC Channel, and Bounded Retries

- Status: Accepted (revised 2026-09-20, owner-directed)
- Date: 2026-08-28, revised 2026-09-20
- Extends: ADR-0013

## Context

ADR-0013 established the fail-closed product update state machine and an independent minisign trust
root. The 2026-08-28 revision of this ADR then required stable builds to read an ordered pair of
endpoints on two independent object stores, so that AI Manager would not become the single point of
failure in the tool-management chain.

That second channel was never provisioned. For more than three weeks it was the stated blocker for
the first release while delivering nothing, because a fallback that does not exist provides no
redundancy — it only adds a second hostname to every contract, test, and review.

The owner ended this on 2026-09-20: **one product channel, on the product's own domain.** The
upstream project this fork is based on ships a single distribution host and has done so across every
release; the redundancy argument did not survive contact with the cost of standing up and paying for
an independent provider before the product has any users.

The product also moved to a single domain, `aimanager.tools`. The endpoints named in the
2026-08-28 revision of this ADR are no longer product endpoints.

Reducing the number of sources must not lower the update trust bar. The manifest is not an
authorization credential: whichever host it came from, the artifact must be verified by the Tauri
Updater against the same product minisign public key before installation. A distribution host never
holds the signing private key, never rewrites signatures, and never becomes a release authority.

## Decision

1. Stable builds read exactly one HTTPS endpoint:
   `https://dl.aimanager.tools/ai-manager/latest.json`. The root manifest is always published last,
   after every immutable version artifact has been uploaded and verified.
2. Prerelease builds are compiled to the `staging` channel and read only
   `https://dl.aimanager.tools/ai-manager/staging/latest.json`. Stable builds never read staging, and
   RCs never read the stable root manifest. The channel comes from the `AI_MANAGER_UPDATE_CHANNEL`
   compile-time variable of the protected workflow; ordinary local builds still have update
   networking disabled.
3. The Tauri configuration pins the product public key and the single stable endpoint. The
   Application layer may override the Updater builder's endpoint with the fixed staging address only
   when the compile-time channel is `staging`. Any unknown channel, HTTP, credential-bearing URL,
   empty public key, or dangerous TLS switch puts the channel into `unconfigured`.
4. Automatic retries are capped at three attempts (including the first) with bounded backoff. Only
   transient network failures — connection, DNS, TLS, timeout, 408, 429, 5xx — are retried; 401, 403,
   404, 407, invalid manifest, platform mismatch, version anomalies, and minisign or encoding errors
   fail immediately. With a single endpoint there is no priority rotation: a retry re-fetches the same
   manifest, and a manifest that parses but references an unreachable artifact fails the action rather
   than silently sourcing the artifact elsewhere.
5. Every automatic retry stays within the single update action the user triggered or that was
   authorized at startup; there are no background scheduled requests. The state wire format shows the
   current attempt number and the cap; after all attempts fail the state stays in the `failed`
   terminal state and the user may retry explicitly.
6. On final failure, a fixed product download-page action is offered. The renderer never submits a
   URL; a narrow native command opens only `https://aimanager.tools/download`, so no general opener is
   re-exposed to the product frontend.
7. If a mirror is ever added back, it may only copy the complete payload already verified and
   published by the protected release, and any artifact it references must still pass the same
   minisign public key. A bad signature must never trigger "switch source and keep installing"; it
   must abort immediately and preserve the error state. Adding such a mirror requires a new ADR, not a
   configuration change.

## Consequences

- The first release is no longer gated on provisioning a second object store, a second DNS name, and a
  second upload credential. That was the intended effect.
- **Accepted risk**: the update channel is now a genuine single point of failure. If
  `dl.aimanager.tools` is unreachable on a user's network, in-app update fails for them. It fails
  safely — the state machine reports `failed` and offers the download page — but it does fail, and the
  download page is on the same domain. Users on a restricted network may be unable to reach either.
  Mitigating this later means a mirror under item 7, or a second domain, and both are new decisions.
- Retry logic gets simpler and honest: with nothing to rotate to, "retry" means exactly one thing.
- Distribution remains an untrusted cache. Installation authorization continues to come solely from
  the product minisign private key and the macOS platform signing chain.
- Windows builds ship unsigned by owner decision, so on that platform minisign and the published
  SHA-256 are the only integrity evidence the user has. That raises, rather than lowers, the
  importance of the manifest-publication order in item 1.
