# AI Manager desktop release runbook

> Status: `v0.1.0-1` published as a prerelease on 2026-09-21 with all four
> targets and the staging manifest live; installed updater round trip,
> clean-machine evidence, and the formal stable matrix pending ·
> Workflow: `.github/workflows/release.yml` · Scope: macOS Apple Silicon,
> macOS Intel, Windows x64, Linux x64

This runbook is the operational contract for an AI Manager desktop release.
Owner authorization for identity, repository creation, version 0.1.0, and
the updater-key ceremony was recorded on 2026-08-20. The product-owned R2
binary channel was approved and provisioned on 2026-08-26. ADR-0018 was revised
on 2026-09-20 to a single product channel on `dl.aimanager.tools`. ADR-0024
fixes the Linux x64 AppImage/deb, updater-signing, and safe terminal boundary.
The owner decided on 2026-09-20 that Windows ships unsigned, so certificate
procurement is no longer a release prerequisite and the release workflow
carries no Windows signing configuration at all; tag creation and publication
remain separate decisions.

This document never stores secret values. Section 5 lists the configuration
names the workflows read; where each credential is held and who may approve its
use are recorded outside the repository.

## 1. Current release state

The first prerelease has shipped. What is still missing before a stable release
is evidence that an *installed* build updates itself, and clean-machine
validation on native Windows and Linux hardware:

- `origin` is the `OnlistTeam/ai-manager` repository; `upstream` remains the
  CC Switch remote.
- `main` is pushed. The product repository carries one tag, `v0.1.0-1`;
  inherited upstream tags remain local and are never pushed.
- package, Cargo, Tauri bundle, repository, and updater public-key identity are
  product-owned and aligned at version 0.1.0-1. A prerelease identifier must be
  numeric; see section 7.
- stable builds read one product path,
  `https://dl.aimanager.tools/ai-manager/latest.json`, provisioned on Cloudflare
  R2. ADR-0018 was revised on 2026-09-20 to drop the never-provisioned second
  channel; the update channel is now a single point of failure by accepted
  decision. Only verified release artifacts may enter the public prefix. That
  stable manifest does not exist yet, by design: no stable release has been
  promoted.
- ADR-0013 registers the updater only behind three narrow product commands.
  `channelReady` is true only for the signed HTTPS channel and a binary compiled
  by the protected workflow with `AI_MANAGER_UPDATE_CHANNEL_ENABLED=1` plus an
  exact `stable` or `staging` channel. Stable builds use only the stable path;
  prerelease builds use only
  `https://dl.aimanager.tools/ai-manager/staging/latest.json`. Local/dev builds
  retain the reserved URLs for contract validation but remain `unconfigured`
  and make no updater request. Startup checks and downloads in the background,
  while installation remains an explicit user restart action.
- the permanent updater private key is held outside Git; the key and its
  password are stored only as `release` environment secrets.
- Apple Developer Program membership, the exact ONLIST Developer ID Application
  identity, App Store Connect notarization key, and protected CI inputs are ready.
- production root manifests are updated only after a GitHub release is promoted
  to stable. Prerelease SemVer builds are compiled for the isolated staging
  channel and may update only `ai-manager/staging/latest.json`; they never read
  or replace either stable root manifest. The staging manifest is live and
  serves the four `v0.1.0-1` platform keys.
- the fixed manual fallback `https://aimanager.tools/download` is reserved in
  the native boundary and reachable. It shares a domain with the update
  channel, so a user who cannot reach one generally cannot reach the other.
- the published macOS arm64 and x64 artifacts passed Developer ID signing,
  App and DMG notarization/stapling, Gatekeeper, read-only mount, and exact-PID
  launch smoke in the protected workflow. Native Intel clean-machine validation
  is still required; the x64 build has so far only run through Rosetta on
  Apple Silicon.
- the protected workflow builds Linux x64 on Ubuntu 22.04 as an AppImage and
  deb, verifies both x86-64 package structures, confirms the in-place AppImage
  updater signature accompanies the installer, and publishes checksums. Linux
  has no cross-distribution platform signature equivalent to Developer ID;
  minisign authorizes automatic updates and SHA-256 records protect
  manual-download integrity. No clean-distribution install, terminal, or
  uninstall evidence exists yet.
- Windows ships unsigned by owner decision (2026-09-20), matching the upstream
  project, which also publishes an unsigned installer. Users will meet a
  SmartScreen "unknown publisher" prompt; the published SHA-256 is the only
  integrity evidence on that platform, so it must appear on the download page.
  The release workflow therefore holds no certificate, no timestamp endpoint,
  and no Tauri `signCommand`, and the release contract fails closed if any of
  them reappears. The MSI is now built and extraction-verified on a native
  Windows runner in the protected workflow; what remains is an installed
  updater round trip and owner clean-machine evidence.
  Native Intel and Linux clean-machine validation are still required even
  though the x64 macOS package passed Rosetta launch smoke and Linux CI
  enforces its package contract.

For cross-host QA only, `scripts/capture-windows-qa-signing-input.mjs` may be
used as a temporary Tauri `signCommand` to retain the exact marker-patched EXE
before NSIS compression, along with the NSIS plugins, temporary uninstaller
stub, and final installer presented to that command. The script writes explicit
`not-verified-qa-capture` evidence and adds no signature. The protected release
workflow never configures a `signCommand`, for capture or for anything else.
`scripts/inspect-pe-security-directory.mjs` can then record whether those exact
PE files contain an embedded certificate table; for this product the expected
answer is that they do not.

For native installer mechanics, dispatch
`.github/workflows/windows-installer-qa.yml` manually. It builds the same
unsigned MSI the release workflow publishes, extracts it, verifies the exact
marker-patched payload, then performs a controlled per-user install,
ten-second launch smoke, and uninstall. It uploads audit logs
only; it never uploads the QA MSI. Evidence explicitly records
`PublicTrust=false` and cannot satisfy the updater round-trip or owner
clean-machine gates. A successful run proves native MSI mechanics only, and
does not become evidence until the exact commit's workflow run has actually
completed.

`pnpm check:release -- --strict` now requires the exact ordered product-owned
stable endpoint pair and a build channel derived from the release kind, with no
endpoint transition exemption. A dispatch still fails closed when any protected
credential the remaining signing chains need is missing.

The protected build workflow and the product-owned distribution sync workflow
must both be present on the default branch before the first release. A
`release` environment exists with updater, Apple, and Cloudflare inputs, but no
required reviewer is configured yet.

Describe a local bundle as signed, notarized, or trusted only when its retained
evidence proves that exact file. Never describe a local test bundle as official
or release-ready.

## 2. Non-negotiable release invariants

1. Dispatch only from the repository default branch.
2. The input tag must be a v-prefixed SemVer exactly matching `package.json`.
3. Strict release configuration must have zero transition items.
4. The target matrix is exactly macOS arm64, macOS x64, Windows x64, and Linux
   x64. Linux publishes only AppImage and deb.
5. macOS applications and DMGs must be Developer ID signed, notarized,
   stapled, and accepted by `codesign`, `spctl`, and `stapler`.
6. Windows publishes an unsigned MSI. The release workflow must contain no
   certificate material, no signing provider selection, and no Tauri
   `signCommand`; the MSI must ship with a SHA-256 record, which is its only
   integrity evidence. Reintroducing platform signing is an owner decision
   (section 3), not a workflow edit.
7. Tauri updater signatures are independent of platform signatures. All four
   updater artifacts must pass minisign verification with the public key
   tracked in `tauri.conf.json`.
8. Source `latest.json` contains exactly four platform keys and only private
   product-repository URLs. Each distribution copy changes only those URLs to
   its exact product-owned release prefix and preserves every signature.
9. Signing material is available only through the protected `release`
   environment and is removed from runner storage in `always()` cleanup steps.
10. A GitHub release is created only after protected publication approval.
11. Linux AppImage and deb each carry an exact SHA-256 record; the AppImage
    updater tarball must contain exactly the same AppImage bytes. These checks
    do not constitute an operating-system publisher signature.
12. A formal release is not approved until the N → N+1 installed updater test
    passes on one clean machine for every target.

The packaging tool is pinned to `@tauri-apps/cli@2.11.4`. Tauri patches an
installer-type marker into the Windows executable so its updater can
distinguish MSI and NSIS installations. The older 2.8 bundler located that
marker through PE symbols and failed after the release profile stripped them;
2.11.4 searches the fixed marker bytes and preserves the existing size
optimization. A Windows build that logs a missing `__TAURI_BUNDLE_TYPE` marker
is invalid even if an installer file was produced.
Tauri owns the Windows release ordering: patch the main executable with the
MSI marker, then embed it in the MSI. After bundling, Tauri restores that clean
source executable, so CI administratively extracts the MSI and verifies the
packaged copy rather than the restored build-tree copy.
The corresponding JavaScript Tauri API, updater, dialog, and process bridge
packages are exact-pinned to the same minor releases as their locked Rust
counterparts; release builds must not bypass the CLI mismatch check.

DMG layout creation is non-interactive: the workflow installs only the three
exact wheels in `scripts/requirements-dmgbuild.txt` with `--require-hashes` and
`--only-binary=:all:`, then uses `dmgbuild` to write the window metadata and
1x/2x background directly. Finder and AppleScript are not part of the release
path. The completed image is Developer ID signed only after assembly.

AI Manager is distributed directly rather than through the Mac App Store. The
full product installs and updates standalone CLI tools, launches managed
processes, reads and rewrites configuration outside its own container, and
ships its own signed updater, none of which fits the App Store sandbox and
self-contained-app requirements (App Store Review Guideline 2.4.5). A Store
listing would need a separately designed reduced edition with its own review
plan, and it must never weaken the direct edition.

## 3. Owner checkpoints

Completed owner checkpoints:

- permanent bundle identifier `tools.aimanager.desktop`;
- product repository `OnlistTeam/ai-manager`;
- first product version `0.1.0`;
- permanent Tauri updater key pair;
- ONLIST Developer ID identity and App Store Connect notarization credential.
- public Cloudflare R2 channel at `dl.aimanager.tools/ai-manager`.

Obtain a new explicit decision before:

- reintroducing Windows platform code signing in any form, which reverses the
  2026-09-20 unsigned-Windows decision and adds a paid, identity-verified
  procurement dependency;
- re-introducing any second distribution source, which ADR-0018 item 7 now
  requires a new ADR for;
- promoting any build to a stable, non-prerelease release. The first
  prerelease, `v0.1.0-1`, was published on 2026-09-21 under the owner
  authorization recorded above.

The identifier migration is implemented in the same commit as the stable
identity. It never changes or deletes the inherited source directory.

The owner has authorized credential research and preparation. Any paid order,
certificate-services agreement, or identity-document submission remains an
action-time human checkpoint.

## 4. Repository and environment protection

Repository setup status:

1. **Complete:** product repository created without generated files.
2. **Complete:** `origin` added; `upstream` retained.
3. **Complete:** `main` pushed explicitly; the one product tag `v0.1.0-1` is
   pushed and inherited upstream tags stay local.
4. **Complete:** branch protection on `main` requires a linear history, forbids
   force pushes and deletions, requires conversation resolution, and requires
   the `Frontend Checks` and `Backend Checks` jobs to pass on an up-to-date
   branch. `backend-windows-wsl2` is deliberately not required; it is too slow
   to gate every change. Administrators are exempt so the owner can still push
   directly.
5. Assign only owner-approved product maintainers or teams as required
   reviewers. Do not reuse inherited upstream CODEOWNERS entries.
6. **Complete:** private vulnerability reporting is enabled, along with secret
   scanning, push protection, and Dependabot security updates.
7. **Complete:** environment named exactly `release` exists.
8. **Partial:** protected-branch-only deployment policy is configured; required
   reviewers remain unset pending an owner-approved person/team and supported
   repository protection.
9. **Complete for updater:** the two updater secrets exist only at environment
   scope. Add later platform secrets/variables there.

The workflow gives repository contents read-only permission by default, and the
publication job adds `contents: write` only after the build and assembly jobs
succeed. No build job requests an OIDC token.

## 5. Required release configuration

Names below are public configuration names, not values. Never paste secret
values into source, issues, logs, runbooks, or ordinary workflow artifacts.

### 5.1 Common updater secrets

| Kind   | Name                                 | Meaning                                        |
| ------ | ------------------------------------ | ---------------------------------------------- |
| Secret | `TAURI_SIGNING_PRIVATE_KEY`          | Password-protected product updater private key |
| Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Private-key password                           |

The corresponding public key is committed in
`src-tauri/tauri.conf.json`. The private key never enters Git.

### 5.2 Distribution inputs

Primary Cloudflare R2:

| Kind     | Name                    | Meaning                               |
| -------- | ----------------------- | ------------------------------------- |
| Secret   | `CLOUDFLARE_API_TOKEN`  | Prefix-scoped R2 object-write token   |
| Variable | `CLOUDFLARE_ACCOUNT_ID` | Product Cloudflare account identifier |

Independent S3-compatible stable backup:

| Kind     | Name                          | Meaning                                      |
| -------- | ----------------------------- | -------------------------------------------- |
| Secret   | `BACKUP_S3_ACCESS_KEY_ID`     | Prefix-scoped backup object-store access key |
| Secret   | `BACKUP_S3_SECRET_ACCESS_KEY` | Backup object-store secret                   |
| Variable | `BACKUP_S3_BUCKET`            | Backup bucket name                           |
| Variable | `BACKUP_S3_ENDPOINT`          | Independent credential-free HTTPS API origin |
| Variable | `BACKUP_S3_REGION`            | Provider region required by AWS CLI signing  |

The stable sync refuses an obvious Cloudflare/R2 API endpoint for the backup.
The credentials may write only the `ai-manager/` prefix and never receive the
Tauri signing private key. Staging publication needs only the primary R2 path;
stable publication fails closed until both providers are configured.

### 5.3 macOS common inputs

| Kind     | Name                         | Meaning                                               |
| -------- | ---------------------------- | ----------------------------------------------------- |
| Secret   | `APPLE_CERTIFICATE`          | Base64-encoded Developer ID Application `.p12`        |
| Secret   | `APPLE_CERTIFICATE_PASSWORD` | `.p12` password                                       |
| Secret   | `APPLE_TEAM_ID`              | Expected Apple team identifier                        |
| Variable | `APPLE_SIGNER_SUBJECT`       | Certificate subject text between the type and team id |
| Variable | `APPLE_NOTARIZATION_METHOD`  | `apple-id` or `app-store-connect`                     |

The workflow requires exactly one valid Developer ID Application identity and
matches `Developer ID Application: <subject> (<team>)` exactly.

For `APPLE_NOTARIZATION_METHOD=apple-id`:

| Kind   | Name             |
| ------ | ---------------- |
| Secret | `APPLE_ID`       |
| Secret | `APPLE_PASSWORD` |

`APPLE_PASSWORD` is an app-specific password, not an interactive account
password.

For `APPLE_NOTARIZATION_METHOD=app-store-connect`:

| Kind   | Name                    |
| ------ | ----------------------- |
| Secret | `APPLE_API_ISSUER`      |
| Secret | `APPLE_API_KEY`         |
| Secret | `APPLE_API_PRIVATE_KEY` |

The private `.p8` contents are written to a fixed temporary runner path with
mode-restricted creation and deleted in the final cleanup step.

### 5.4 Windows platform signing

None. Windows x64 reads only the common updater secrets in section 5.1. There
is no certificate secret, no publisher subject, no timestamp endpoint, and no
cloud signing identity, because the product does not sign Windows artifacts.

The cost of that decision is the SmartScreen "unknown publisher" prompt on
first run, which upstream CC Switch also carries. The compensating control is
the published SHA-256 record next to the MSI, plus the minisign signature that
authorizes every automatic update.

Reversing this needs an owner decision (section 3) and a fresh design, not a
workflow patch. A future signing identity would have to be a public-trust OV or
EV code-signing identity whose private key stays in compliant hardware or a
managed HSM, would have to fail closed on missing inputs, and would have to
prove the approved publisher subject, public chain, SHA-256 digest, and RFC
3161 timestamp on disposable `.exe` and `.msi` files before any release used
it.

## 6. Updater key ceremony

Perform this only after the owner authorizes the permanent updater identity.

1. Work in a protected directory outside the repository on a trusted machine.
2. Use `pnpm tauri signer generate` interactively so the password is not
   exposed in shell history or the process list.
3. Store the private key and password in the organization's approved secret
   manager, with a separately controlled recovery copy.
4. Put CI copies only in the protected `release` environment secrets.
5. Commit only the generated public-key string to `tauri.conf.json`.
6. Sign a disposable fixture with the private key.
7. Decode the Tauri `.sig` and verify it with minisign and the committed public
   key.
8. Record the public-key fingerprint and ceremony date in the private release
   register, not the private key or password.
9. Delete every disposable fixture and unapproved copy.

Losing the private key prevents future updates to installed builds. Leaking it
allows an attacker to sign updater payloads. Treat recovery and rotation as a
release-critical design decision.

### Rotation of 2026-09-21

The key generated for `v0.1.0-1` existed only as a `release` environment
secret. GitHub secrets cannot be read back, so recreating the repository would
have destroyed it, and step 3 above — a separately controlled recovery copy —
had not actually been carried out. It was rotated rather than extracted:
reading a signing key out through a workflow would have written it to CI logs
and artifacts, which is worse than the problem.

The consequence is recorded here because it is not reversible: `v0.1.0-1`
carries the retired public key `68E633826E335767` and **cannot be updated in
place**. Any machine running it has to install a later build by hand. Builds
from `v0.1.0-2` onward carry `3FB8A3FCA75D2E44`.

The replacement exists in two controlled locations outside this repository as
well as in the environment secret, which is what step 3 always required.

## 7. Version and identity preparation

Before the release commit:

- update `package.json`, `src-tauri/Cargo.toml`, and
  `src-tauri/tauri.conf.json` to the same approved version;
- update lockfiles through the normal package/Cargo tools;
- confirm product name, package name, binary name, bundle identifier,
  repository URL, the continued absence of the inherited deep link, installer
  names, updater endpoint, and updater public key;
- confirm prerelease SemVer builds compile only the staging channel and cannot
  alter either stable root manifest;
- confirm the old product AppData migration is idempotent and non-destructive;
- confirm `~/.cc-switch` remains read-only import input;
- run `pnpm check:release -- --strict` and require zero findings;
- inspect `git diff` for private material and upstream release URLs.

Never change an identifier merely to make a check pass. The identifier is an
installed-product identity and data-location boundary.

## 8. Pre-dispatch verification

Run from a clean checkout of the intended release commit:

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm format:check
pnpm check:boundaries
pnpm check:release -- --strict
pnpm test:release
pnpm test:unit
pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost \
  cargo test --manifest-path src-tauri/Cargo.toml
```

Also verify:

- `git status --short` is empty;
- the release commit is on the protected default branch;
- no public tag or release already uses the intended version;
- every environment secret/variable name exists for the macOS signing method
  in use and for the updater;
- the required frontend CI check runs for root Markdown, `docs/**`,
  `flatpak/**`, and `.github/**` changes, so release-policy-only changes cannot
  bypass the release contract;
- actionlint 1.7.12 passes every active workflow; CI downloads the pinned
  archive only after verifying its published SHA-256 checksum;
- private vulnerability reporting is enabled before the first public
  prerelease;
- the release notes describe actual changes and known limitations;
- real macOS Intel, Windows x64, and Linux x64 QA paths are scheduled.

Do not rerun an existing release tag merely to replace assets. Treat published
artifacts as immutable; recovery requires an explicit incident decision.

## 9. Dispatch and approvals

The workflow is manual only. In GitHub Actions:

1. Select **Protected Desktop Release** on the default branch.
2. Enter the approved v-prefixed version tag.
3. Use a prerelease SemVer tag such as `v0.1.0-1` with **prerelease**
   enabled. Use a stable SemVer tag such as `v0.1.0` only with **prerelease**
   disabled. Preflight rejects mismatched tag/publication semantics; an RC is
   never promoted in place because its binary is compiled for staging.

   **The prerelease identifier must be numeric.** SemVer permits `-rc.1`, the
   MSI bundler does not: it fails the Windows job with `optional pre-release
   identifier in app version must be numeric-only and cannot be greater than
   65535`. Number the prerelease series instead — `v0.1.0-1`, `v0.1.0-2` — and
   keep `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`
   on the same value, because preflight compares all three against the tag.
4. Start the workflow.
5. Confirm preflight passes before approving protected build jobs.
6. Approve the `release` environment only after verifying the commit SHA,
   version, provider selection, and planned signer subjects.
7. Let all four target builds and assembly complete.
8. Inspect signing/notarization evidence and the complete payload.
9. Approve the protected publication job only when every automated artifact
   check is green.

The workflow submits each macOS artifact to notarization once. It retries only
polling failures that clearly look transient; authentication, configuration,
and completed rejection states fail immediately. Every polling response and
the final service log are retained as audit artifacts.

## 10. Expected artifacts

For version `<version>`, the public release payload contains exactly:

```text
AI-Manager-<version>-macOS-arm64.dmg
AI-Manager-<version>-macOS-arm64.dmg.sha256
AI-Manager-<version>-macOS-arm64.app.tar.gz
AI-Manager-<version>-macOS-arm64.app.tar.gz.sig
AI-Manager-<version>-macOS-x64.dmg
AI-Manager-<version>-macOS-x64.dmg.sha256
AI-Manager-<version>-macOS-x64.app.tar.gz
AI-Manager-<version>-macOS-x64.app.tar.gz.sig
AI-Manager-<version>-Windows-x64.msi
AI-Manager-<version>-Windows-x64.msi.sha256
AI-Manager-<version>-Windows-x64.msi.sig
AI-Manager-<version>-Linux-x64.AppImage
AI-Manager-<version>-Linux-x64.AppImage.sha256
AI-Manager-<version>-Linux-x64.deb
AI-Manager-<version>-Linux-x64.deb.sha256
AI-Manager-<version>-Linux-x64.AppImage.sig
latest.json
```

Separate workflow audit artifacts contain macOS notarization logs, Windows
packaging evidence, and Linux package-structure evidence. They are not part
of the public updater payload.

`latest.json` must contain these keys in this order:

```text
darwin-aarch64
darwin-x86_64
windows-x86_64
linux-x86_64
```

The assembly job rejects missing, extra, malformed, checksum-mismatched, or
foreign-repository assets before publication.

### Stable and RC distribution sync

`.github/workflows/sync-r2.yml` runs for a published GitHub release, or when an
operator explicitly dispatches a backfill. It resolves the release metadata
from GitHub: prereleases target only the primary `staging/latest.json`; stable
releases target both stable providers. It downloads private release assets with
the repository token, validates the exact payload, and rewrites only updater
URLs. Tauri minisign signatures remain unchanged and are still checked against
the public key embedded in the app; both providers are untrusted delivery
caches, not signing authorities.

Versioned files are uploaded first under the provider-specific `<tag>/` prefix
with immutable cache headers. Immediately before writing a root manifest, the
workflow rechecks GitHub's latest eligible stable or prerelease release. An old
manual backfill can restore versioned files but cannot replace a root manifest.
For stable publication the backup root is written before the primary root; each
is then read back through its public domain and compared byte-for-byte with its
provider-specific manifest. Root files use a five-minute cache lifetime.
Cloudflare and backup credentials enter only their upload steps; product builds,
checkout, and signing steps never receive either distribution credential.

## 11. Automated evidence review

### macOS, for both architectures

- imported identity is exactly the approved subject/team;
- signed application passes `codesign --deep --strict` before notarization;
- app submission status is `Accepted`;
- app ticket is stapled and validates;
- updater tarball is rebuilt from the stapled app and signed afterward;
- DMG assembly used the hash-locked headless dependency set and did not invoke
  Finder or AppleScript;
- signed DMG submission status is `Accepted`;
- DMG ticket is stapled and validates;
- final app passes `codesign`, Gatekeeper `spctl`, and `stapler`;
- final DMG passes `codesign`, Gatekeeper `spctl`, and `stapler`;
- SHA-256 checksum matches the uploaded DMG.

### Windows x64

- the workflow contains no certificate, timestamp, or `signCommand` input;
- Tauri writes the MSI marker into the executable it embeds;
- the MSI is administratively extracted without installing the product;
- the packaged `ai-manager.exe`, not Tauri's restored build-tree copy, contains
  the exact MSI marker at the audited offset;
- the Tauri `.sig` is generated from the final MSI;
- SHA-256 checksum matches the uploaded MSI and is published beside it.

### Linux x64

- the AppImage is executable and identified as an x86-64 ELF;
- the deb identifies package `ai-manager` and architecture `amd64`;
- the packaged `usr/bin/ai-manager` is executable and an x86-64 ELF;
- the updater tarball has exactly one entry and extracts to bytes identical to
  the public AppImage;
- both public installers' SHA-256 records match;
- the updater tarball passes product minisign verification on the assembly
  host; no platform code-signing claim is made for AppImage or deb.

### Updater assembly

- the tracked public key decodes as a minisign public-key box;
- both macOS tarballs, the final MSI, and the Linux AppImage updater tarball
  pass minisign verification;
- signatures embedded in `latest.json` match the uploaded `.sig` files;
- every source URL points to the exact repository and version tag;
- every distributed URL points to its exact product-owned prefix, with identical
  version, notes, publication date, platform set, and signatures;
- the platform set is exact and contains no aliases.

## 12. Manual release-candidate matrix

Automated signatures are necessary but not sufficient. On a clean machine for
each target, record the OS version, architecture, installer hash, tester, date,
and result for:

- download and quarantine-preserving open;
- installer publisher/Gatekeeper presentation;
- install, first launch, close, and relaunch;
- first launch with a CC Switch setup found/absent/unreadable;
- one safe AI Tools lifecycle action;
- AI Services list, switch, and manual connection test;
- Extensions list and one reversible toggle;
- backup create/list/restore confirmation;
- dark mode, English, and Chinese;
- keyboard focus and primary navigation;
- data persistence after relaunch;
- normal uninstall behavior;
- paths with spaces and, on Windows, a non-ASCII user profile;
- bounded logs with no test secret leakage.

macOS additionally requires a native Apple Silicon machine and a real Intel
machine or equivalent approved Intel QA host. Windows requires a standard-user
install, Desktop/Start Menu shortcuts, Windows Settings uninstall, a recorded
walkthrough of the SmartScreen unknown-publisher prompt including the exact
wording a user must click through, `.cmd` shim behavior, and GUI
working-directory checks.

Linux requires both AppImage and deb paths on a clean x64 distribution: desktop
launcher behavior, executable permission, package-manager uninstall, Wayland
and X11 launch where available, a supported terminal handoff, and preservation
of the same product data directory across relaunch/update.

## 13. Installed update round trip

The first updater proof requires two signed prerelease SemVer versions. ADR-0018
fixes their isolated manifest at
`https://dl.aimanager.tools/ai-manager/staging/latest.json`, which is
provisioned and currently serves `v0.1.0-1`. GitHub's
`releases/latest/download/latest.json` excludes prereleases and is not a product
updater endpoint.

Version N is `v0.1.0-1`, already published. What this section still needs is
N+1 and the evidence below.

1. Install and launch version N from its installer on each clean target.
2. Publish a signed N+1 prerelease with the same product identity and updater
   trust root.
3. Confirm N discovers only the product-owned staging manifest and never reads
   either stable root.
4. Confirm displayed version/notes and architecture-specific URL are correct.
5. Download and install through the product updater flow.
6. Confirm the updater rejects a deliberately bad disposable signature in a
   non-production test environment.
7. For a stable candidate, block the primary domain and confirm the backup
   manifest and provider-specific artifact complete the update; restore normal
   DNS before continuing.
8. Confirm the app relaunches as N+1.
9. Confirm settings, providers, extensions, backups, and logs remain intact.
10. Confirm the CC Switch source database hash is unchanged.

Do not promote a prerelease to formal merely because `latest.json` parses.

## 14. Rollback and failed releases

- Stop publication approval if any target or signature check fails.
- Preserve audit artifacts and logs; do not rebuild over the same evidence.
- For a failed prerelease, document the failure and publish a higher fixed
  version. Do not silently replace binaries under an existing tag.
- Do not point the updater at an older version as a downgrade mechanism.
- If an installer shipped but the updater manifest is wrong, disable the
  product endpoint or mark the release unavailable according to the incident
  decision, then publish a corrected higher version.
- Restore user data only from a user-confirmed backup; release rollback must
  never delete or rewrite product or CC Switch data automatically.

## 15. Signing or key incident response

1. Freeze the `release` environment and remove publication approval.
2. Preserve workflow, GitHub, and Apple audit evidence.
3. Revoke or disable the affected platform certificate or credential.
4. Rotate environment secrets as applicable.
5. If the Tauri private key is suspected compromised, stop serving updater
   manifests immediately and begin a dedicated trust-root migration plan.
6. Do not simply replace the public key in source: installed clients trust the
   old key and need an authenticated transition release.
7. Notify the owner and record affected versions, hashes, time window, and
   remediation.
8. Resume only after a clean-key ceremony, reviewed workflow diff, and fresh
   target evidence.

## 16. Authoritative references

- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri Linux distribution](https://v2.tauri.app/distribute/)
- [ADR-0024 Linux release and terminal boundary](../adr/0024-linux-release-and-terminal-boundary.md)
- [Apple Developer ID](https://developer.apple.com/developer-id/)
- [Mac App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/#software-requirements)
- [GitHub protected environments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)
- [Microsoft SmartScreen application reputation](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/)
