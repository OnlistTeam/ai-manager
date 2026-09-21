# ADR-0022: Trusted Transport Fallback for the Skills GitHub Primary Source

- Status: Accepted
- Date: 2026-08-28
- Related: ADR-0002, ADR-0015, ADR-0016

## Context

The Skills catalog, installation, update checks, and updates all obtain GitHub repository snapshots
through `SkillService::download_repo`. Repository coordinates and `SKILL.md` remain the content
identity, but `github.com` is the only transport path; when that domain suffers DNS, connection,
TLS, timeout, rate-limit, or server-side failures, the catalog turns into an empty list and users
can only guess whether they need to configure a proxy.

The existing npm download has already established a region-neutral recovery paradigm: official
source first, a single in-process fallback only for recoverable network failures, no changes to the
user's global configuration, and honest disclosure after a successful fallback. Skills need the
same semantics, but must not introduce arbitrary mirror domains, weaken repository identity
validation, or use a mirror to mask permission and not-found errors.

jsDelivr's official documentation states that its `/gh/` endpoint fetches the specified repository
version from GitHub and stores it permanently; the public Data API also defines per-file paths,
sizes, and SHA-256 for GitHub packages. The official implementation of that API obtains a single
snapshot manifest from `+private-json` in the same CDN namespace:

- [jsDelivr GitHub CDN syntax](https://github.com/jsdelivr/jsdelivr#github)
- [jsDelivr Data API](https://github.com/jsdelivr/data.jsdelivr.com)
- [Data API snapshot manifest implementation](https://github.com/jsdelivr/data.jsdelivr.com/blob/master/src/remote-services/JsDelivrRemoteService.js)

## Decision

1. The GitHub archive remains the only primary source. Only transient transport errors while
   sending the request or streaming the response, or HTTP `408`, `429`, `5xx`, enter a single
   jsDelivr fallback stage. `401`, `403`, `404`, `407`, other client errors, and ZIP parsing, path,
   size, or content errors fail as is; `404` keeps only the existing
   `configured → main → master` branch probing and does not trigger the mirror.
2. The fallback domain is fixed to the HTTPS GitHub channel of `cdn.jsdelivr.net`. owner,
   repository, and branch are still validated through the same GitHub coordinate checks first and
   then encoded with the structured URL builder; the renderer, the database, and user input cannot
   submit mirror URLs.
3. The mirror does not serve repository ZIPs, so native first reads the manifest from
   `.../+private-json`, which belongs to the same CDN snapshot namespace as the files, and then
   reads the files concurrently. The separately cached Data API of a moving branch must not be
   mixed with CDN files, otherwise the two caches' differing timestamps would assemble a valid
   repository into an inconsistent snapshot. The manifest response is at most 16 MiB and at most
   10,000 files, declared plus actual content is at most 128 MiB in total, and concurrency is fixed
   at a maximum of 8. Paths must be safe relative ordinary file paths; symlinks are not materialized.
4. Every file must simultaneously satisfy HTTP success, actual length equal to the manifest length,
   and the Base64 of the actual SHA-256 equal to the manifest hash before it is written to the
   temporary directory. Failure of any file discards the entire temporary tree and never proceeds
   to the scan, install, or update steps.
5. The fallback continues to use the product's existing global outbound client and therefore honors
   the constrained local download proxy; it does not log the proxy address, credentials, file
   bodies, or full request URLs. Private GitHub repositories do not go through the mirror, and no
   GitHub credentials are ever sent to jsDelivr.
6. As long as any repository in a single catalog result was obtained via the mirror, the product
   projection carries `mirrorUsed=true`. After the result appears, the UI explains that this load
   went through a trusted transport mirror and that the GitHub repository remains the authoritative
   source; the message does not appear when the first GitHub hop succeeds. When both the primary
   source and the mirror fail, the error guidance offers both retry and configuring the local
   download proxy in Settings as next steps.

## Alternatives

- **Replace GitHub URLs with a generic proxy prefix.** The proxy domain and redirect targets are
  unauditable, and there is no per-file integrity manifest. Rejected.
- **Try the mirror on any GitHub failure.** Would disguise private-repository permissions, wrong
  branches, and non-existent repositories as network recovery problems. Rejected.
- **Download recursively through the GitHub raw API.** Still depends on the same failure domain and
  needs more API requests and rate-limit handling; it cannot form an independent recovery path.
  Rejected.
- **Use the separately cached public Data API manifest together with CDN files of a moving
  branch.** On real networks the manifest and files can belong to different commits; even with
  per-file hash verification this only turns a normal fallback into a stable failure. Mixing is
  rejected; instead read the snapshot manifest from the same namespace as the files.
- **Silently use the mirror without disclosure.** Content identity is unchanged, but a change of
  transport party is a fact users should know, and it would distort network diagnostics. Rejected.

## Consequences

- Positive: on restricted or unstable networks, public Skills repositories retain catalog, install,
  and update capability; global users see no extra requests or regionalized experience when GitHub
  is healthy.
- Positive: the mirror is only a verifiable transport; repository coordinates, branch, Skill
  identity, install transaction, and GitHub documentation links are all unchanged.
- Negative: the mirror's file-manifest mode issues more requests than a ZIP and temporarily stores
  verified content within the 128 MiB cap; large repositories fail closed and guide the user to
  configure a proxy to reach the primary source directly.
- Negative: jsDelivr's public GitHub channel does not apply to private repositories; permission
  errors do not fall back, which is a deliberate security boundary rather than a missing feature.
- Negative: the snapshot manifest is an internal dependency of jsDelivr's public Data API rather
  than a separately promised public API; if upstream removes that endpoint the fallback fails closed
  and guides toward a proxy, and the replacement contract must be re-audited.
