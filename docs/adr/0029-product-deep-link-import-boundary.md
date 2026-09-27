# ADR-0029: AI Manager Deep Links — Own Scheme, CC Switch-Compatible Format, Credentials Accepted

- Status: Accepted (owner-directed, 2026-09-20); implemented 2026-09-20. Decision 3 reversed
  (owner-directed) and reimplemented 2026-09-23. Decision 4 withdrawn (owner-directed) and the
  paste path removed 2026-09-27.
- Date: 2026-08-29, revised 2026-09-20, revised 2026-09-23, revised 2026-09-27
- Supersedes the 2026-08-29 proposal, which reserved a different scheme and rejected every
  credential-bearing link on every path.

## Context

Phase 7 removed the inherited `ccswitch://` product registration, and commit `ee223288`
(2026-09-18) deleted the inherited parser and its tests along with it. The product therefore
started from nothing: no scheme, no parser, no importer. The old parser survives only in git
history, where it is useful as a record of the wire format and of nothing else.

The upstream format is:

```
ccswitch://v1/import?resource=provider&app=claude&name=<name>&apiKey=<key>&configFormat=json&config=<base64>
ccswitch://v1/import?resource=mcp&apps=claude,codex&config=<base64>&enabled=true
ccswitch://v1/import?resource=prompt&app=claude&name=<name>&content=<base64>&description=<text>
ccswitch://v1/import?resource=skill&repo=<owner/repo>&branch=<branch>&skills_path=<path>&directory=<dir>
```

Two facts drive this decision, and they pull in opposite directions.

**The format's value is real.** Third-party API vendors already publish one-click import links in
this exact shape. A product-owned format that nobody emits has to be promoted from zero; a
format-compatible one costs a cooperating vendor a single string change.

**The format's payload is credentials.** `apiKey` sits in the query string, and the base64 `config`
carries `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, and MCP `--api-key` arguments.
A custom URI scheme cannot prove its caller's origin, and Windows and Linux hand the URL to a new
process as argv, where it can surface in process lists, crash reports, and system logs. Stripping
the credential fields would keep the shape but remove the reason the links exist.

### What the 2026-09-20 split cost, and why decision 3 was reversed

The 2026-09-20 revision resolved that tension by giving the two paths different capabilities: a
paste could carry a key, a link opened through the registered scheme could not. Three months of
the format's actual usage showed that this did not produce a safer one-click import — it produced
no one-click import at all:

- A service link with no key cannot be stored (a saved endpoint without its key has no shape in
  this product), so it is blocked as `credentialRequired`. A service link *with* a key was blocked
  as `credentialBlocked`. **Every** provider link opened from the registered scheme was therefore
  refused on one ground or the other; the capability that decision 3 appeared to preserve did not
  exist.
- The refusal was also invisible. On the argv path the error is swallowed in `lib.rs` and the
  window is not even focused, so a vendor's "import" button appeared to do nothing at all — worse
  for the user than having no button.
- A cooperating vendor's only remaining route was "copy this link, now go find Settings → Backup
  and restore → Add from link, now paste". That is more steps than typing the key by hand, which
  the product has always allowed.

The owner directed (2026-09-23) that both paths accept the same fields, matching CC Switch and the
vendors already shipping these links.

**The argv exposure is real and is accepted, not resolved.** On Windows and Linux the OS hands the
URL to a new process as argv, where a key can surface in process lists, crash reports and system
logs; macOS delivers it as an `open-url` event instead. Nothing in the 2026-09-23 revision makes
that untrue. What the product relies on instead is the confirmation dialog: an external link
cannot write anything without an explicit confirmation that names the target tool, the exact
change, and the fact that the link carries a credential — and the dialog says the link came from
outside AI Manager. Anyone revisiting this decision should weigh that trade-off again rather than
assume the risk was engineered away.

### Why the paste path was removed (2026-09-27)

After the 2026-09-23 revision the registered scheme carries everything a paste could, so the
Settings card "Add from link" kept only one capability of its own: accepting `ccswitch://` links
and bare `v1/import?...` queries. Using it meant right-clicking a vendor's import button, copying
the link address, and finding the card in Settings. That is no shorter than copying the key into
the ordinary add-endpoint form, and upstream CC Switch has no such field. The owner directed that
the card, its command, and the second origin be removed.

The cost is accepted: until a vendor publishes an `aimanager://` link, its existing `ccswitch://`
button does nothing for a user who has only AI Manager installed, and that user adds the endpoint
by hand.

## Decision

1. **The product scheme is `aimanager`.** Only `aimanager://v1/import/...` is registered
   statically. Do not reuse or co-register `ccswitch`: URL schemes are globally contended on macOS
   and Windows, so registering it would hijack the import links of users who also have CC Switch
   installed. Do not accept renderer-supplied schemes at runtime, and do not present the custom
   scheme as a verified HTTPS App Link.

   The owner accepted the known trade-off that `aimanager` is a generic string another application
   could also claim.

2. **The format is field-compatible with CC Switch.** Same `v1/import` path, same `resource`,
   `app`, `apps`, `name`, `endpoint`, `homepage`, `configFormat`, `config`, `repo`, `branch`,
   `skills_path`, `directory`, `enabled` parameter names and semantics. A vendor supports AI Manager
   by changing the scheme and nothing else. Product-specific extensions, if ever needed, use an
   `x-` prefix and must be ignorable.

3. **Links accept credentials** (revised 2026-09-23, owner-directed; this decision previously
   read "Registered-scheme links are credential-free"). `apiKey` and a credential-bearing `config`
   are accepted. The credential is still *detected* — the confirmation dialog names the field that
   carries it, and its value never reaches the renderer — but its presence does not refuse the
   link. What protects the user is decision 5's confirmation plus the dialog's statement that the
   link came from outside AI Manager. The argv exposure described in the Context section is
   accepted, not eliminated.

4. **Withdrawn 2026-09-27.** This decision added a second, paste-only path that also accepted the
   `ccswitch://` prefix and a bare `v1/import?...` query. That path is gone (see "Why the paste
   path was removed"), so the product accepts only `aimanager://`, which is decision 1's
   scheme-contention rule. With a single origin the parser takes neither an origin nor a
   compatible-scheme flag.

5. **One parser and one confirmation.** The raw URL is at most 8 KiB, parsed only
   in native, and never logged. Scheme, host, path, and query use a strict allowlist; duplicate
   keys, unknown keys, fragments, userinfo, NUL, non-UTF-8, over-long fields, and malformed base64
   are rejected. Native stores intents in an in-memory queue of at most 8 entries with a 10-minute
   expiry and hands the renderer only an opaque pending ID. The renderer fetches a safe preview
   through a strict command; every entry shows the source as an untrusted external link, the target
   tool, and the exact change that will happen. Credential values are never returned to the
   renderer — only their presence and which field carries them. Only explicit user confirmation
   invokes the existing Application mutation. Rejection or expiry produces no writes.

6. **Skill imports stay inside the trusted-source policy**: `owner/repo`, an
   optional fixed branch and directory, installed through the existing archive guardrails.

7. **Second-instance argv, first-instance cold start and macOS `open-url` use the same parser.** The single-instance
   plugin is registered first; only URLs Tauri has already filtered by the static scheme are
   handled, and the full format is validated again. Ordinary file arguments and unknown schemes
   only focus the window and are not parsed.

8. **Installers must verify scheme registration and uninstall cleanup.** macOS, Windows x64, and
   Linux x64 each cover cold and warm start, multiple links, malicious argv, cancel, confirm,
   duplicate consumption, credential-bearing acceptance, `ccswitch://` being refused, and
   app-not-installed.

## Consequences

- A cooperating vendor reaches AI Manager users with a one-character-class change. A
  non-cooperating vendor's `ccswitch://` link does not reach AI Manager at all (until 2026-09-27
  it could be pasted in Settings); those users add the endpoint by hand.
- A vendor's one-click import button now actually works: the user presses it in a browser, AI
  Manager opens, and a confirmation dialog appears. Before the 2026-09-23 revision every such link
  was refused, silently.
- **Credentials can now reach the OS command line** on Windows and Linux, where they may appear in
  process lists, crash reports, and system logs. This is the cost of the revision and it is not
  mitigated in code — only by the confirmation dialog and by the dialog naming the link's origin.
  A user who follows a malicious `aimanager://` link still has to read and confirm a dialog that
  names the tool, the change, and the presence of a credential before anything is written.
- Tracking the upstream format means inheriting its shape, including its weaknesses. Field-level
  validation is ours; we do not inherit upstream's parser, which no longer exists in the tree.

## Pending approval before implementation

Both items below were resolved before the feature was built.

1. `tauri-plugin-deep-link = "=2.4.10"` and the `deep-link` feature on the existing
   `tauri-plugin-single-instance`. **Approved.** The earlier draft of this ADR named 2.4.5 and
   justified it as "the version aligned with the current `tauri 2.8.2`". That premise was wrong:
   `Cargo.toml` declared `tauri = "2.8.2"` under caret semantics while `Cargo.lock` had long
   resolved to **2.10.3**, and 2.4.5 predates that by roughly a year. 2.4.10 (2026-08) requires
   `tauri ^2.10` and is the release that matches what is actually built; the existing
   single-instance 2.4.0 already accepts `tauri-plugin-deep-link ^2.4.7`, so nothing else moved.
   - Alongside it, `tauri` is now declared as `"=2.10.3"`. Under a caret requirement AI_RULES
     rule 5 ("never upgrade a core dependency on your own") was unenforceable, because the
     declaration and the lock file could drift silently. Raising the framework is now a visible
     edit rather than a side effect.
   - **Why needed:** the official plugin handles macOS bundle metadata, the Windows registry, the
     Linux desktop handler, and open-url events. The current dependencies include only
     single-instance, which cannot reliably receive macOS URL events or generate cross-platform
     registration.
   - **Size/maintenance:** nine new crates resolved (`rust-ini` and its `ordered-multimap` /
     `dlv-list` / `const-random` / `tiny-keccak` / `crunchy` chain on Linux, `windows-registry`
     and `windows-result` on Windows, plus the plugin itself); serde, url and tauri are reused.
     Pinned, to be audited with future Tauri core upgrades.
   - **Security:** the plugin only delivers the URL and cannot authenticate its source. The
     allowlist, in-memory queue, credential rules, and confirmation above are the product's.
2. Whether v1 ships all four resources or starts with `provider` and `skill` only.
   **Resolved: all four.**

## What the implementation settled beyond the decisions above

- **`enabled=true` is parsed and deliberately ignored.** Upstream switches to the imported service
  when a link asks for it. Switching, or turning a server on, changes what a tool does right now;
  an import never makes that call for the user. This matches the existing-setup import, which also
  leaves the live selection alone.
- **A service link must carry a key somewhere.** The product stores an API endpoint together with
  its key and has no shape for a keyless one, so a link with neither `apiKey` nor a key inside
  `config` is reported as blocked in the preview rather than offered as a button that can only
  fail. Under the original decision 3 this combined into a dead end — a keyless link was blocked
  here and a keyed one was blocked there, so no provider link could ever be imported from the
  registered scheme. Since the 2026-09-23 revision this rule stands alone: it rejects links that
  genuinely cannot be stored, and provider links from either path work.
- **`model` is accepted as a provider parameter.** It is not in the decision-2 field list, but the
  upstream links already carry it and the tools whose configuration _is_ a model table have no
  usable connection without one.
- **MCP servers carrying `env` or `headers` are refused.** The product's typed MCP install draft
  deliberately has no field for either, so accepting the link and dropping them would install a
  server that cannot authenticate.
  ADR-0047 later gave the draft both fields; link import still refuses them, because its preview
  cannot show and confirm a credential.
- **Credential detection reuses `platform::redact`.** Proving a tool-native blob is credential-free
  would otherwise need a second, per-tool, per-format table that drifts from the first. The
  existing detector is format-agnostic, so it also sees a key inside a TOML string or an
  `--api-key` argument. The compatibility layer cross-checks it against `api_key_slots` and the
  upstream reader, and a tool with no upstream service format fails closed.
- **One extra command.** The four commands the design named are joined by
  `app_deeplink_dismiss`, without which cancelling could not clear the native queue and the
  dialog would reappear until it expired. `app_deeplink_submit_pasted` left with the paste path
  on 2026-09-27.
- **Multi-target MCP imports report a count, not task ids.** `begin` takes the per-tool task lock,
  so several servers for one tool have to start one after another; every pair is validated and
  gated before the first one starts, and the tasks appear in the Task Center as they begin.

## Official References

- Tauri Deep Linking plugin: <https://v2.tauri.app/plugin/deep-linking/>
