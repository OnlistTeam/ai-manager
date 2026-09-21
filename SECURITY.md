# Security Policy

## Supported versions

Security fixes are provided for the latest release published on the
[Releases page](https://github.com/OnlistTeam/ai-manager/releases). Local
development builds and older releases are not supported.

## Reporting a vulnerability

Do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.

Use GitHub private vulnerability reporting:
<https://github.com/OnlistTeam/ai-manager/security/advisories/new>. The report
stays private between you and the maintainers until a fix is published.

Please include:

- a description of the vulnerability;
- the untrusted source of the input and the full data path from that source to
  the affected code;
- steps to reproduce against a supported release;
- the potential impact and the affected versions.

The data path matters more than the sink. Severity is assessed by who controls
the input, not by which API the value eventually reaches.

Response targets:

- acknowledgement within 48 hours;
- initial assessment within 7 days;
- fix for critical issues within 14 days.

## Disclosure

We follow coordinated disclosure: the reporter submits privately, we confirm
and fix, a patched release is published, and the vulnerability is disclosed
afterwards. Reporters are credited in the release notes unless they prefer to
stay anonymous. For eligible issues the maintainers may request a CVE through a
GitHub Security Advisory once a fix is available. Severity is scored with CVSS
(v3.1 or v4.0), and the vector reflects any required user interaction or prior
local access.

## Threat model

AI Manager is a local desktop application that manages configuration for AI
coding tools on the user's own machine. There is no project-operated cloud
backend, no multi-user model, and no privilege separation from the user who
runs it.

The bundled WebView renderer is inside the trust boundary: the frontend is
bundled at build time, the content security policy limits scripts to bundled
assets (`script-src 'self'`), there is no `eval` or `new Function`, and no
user-controlled string reaches an HTML sink as content. Reports whose only path
to the IPC surface is direct invocation from DevTools or a locally modified
frontend are out of scope, because someone in that position already controls
the machine. If any of the facts above stops being true, that is itself a
security issue and is always in scope.

### In scope

Inputs that genuinely cross a trust boundary:

- inbound requests to the local routing proxy, including from other hosts when
  it is bound to a non-loopback address;
- upstream API responses processed by the local routing proxy;
- imported files: SQL import/export, provider and MCP configuration import,
  Skill archives and catalogs;
- remote data the renderer displays or acts on (model pricing, provider
  avatars, Skill catalogs, version feeds);
- live tool configuration files on disk that a third party can write;
- any path by which credentials reach logs, exports, or shared configuration
  snippets;
- the build, release, signing, and updater pipeline.

A confused-deputy case, where an untrusted source controls the path or content
of a file operation the product performs with the user's permissions, is in
scope. So is any integration carrying executable content (MCP servers,
terminal launches) that arrives through import and is enabled without an
explicit, accurately presented user decision.

### Out of scope

- ordinary file operations whose path and content the user chose through the
  local UI, with no untrusted input involved;
- user-authored integrations executing by design (an MCP server or terminal
  command the user typed and enabled);
- denial of service against the user's own local instance;
- automated scanner output without a demonstrated exploitation path on a
  supported release.

## Security updates

Security fixes ship as signed patch releases through the same verified release
and updater channel as every other release. What "signed" means differs by
platform: macOS artifacts carry a Developer ID signature and Apple
notarization, while Windows and Linux have no equivalent platform signature, so
their integrity evidence is the published SHA-256 plus the minisign signature
the updater verifies. Never install a build whose evidence for its own platform
does not match what the [release runbook](docs/development/RELEASE_RUNBOOK.md)
describes.
