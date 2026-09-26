# ADR-0049: Privacy protection for proxied requests

- Status: accepted
- Date: 2026-09-26

## Context

When a tool is taken over by the local routing proxy (ADR-0007), every request
passes through AI Manager before it reaches the provider. Those requests carry
far more than the prompt: whole files read by the agent, command output, `.env`
contents, git configuration, pasted logs. Keys, passwords, private keys, email
addresses, phone numbers, ID numbers and bank card numbers regularly end up in
them without the user noticing. The provider never needs the real values to do
its job; the tool on the user's machine does.

Because the proxy already sees and rewrites both directions of this traffic,
it can keep the real values on the machine: send a stand-in, and put the real
value back when the stand-in comes back in the reply.

Constraints that shape the design:

- Conversation history is resent on every turn, and providers cache prompt
  prefixes. A stand-in that changes between requests would break both the
  model's view of the conversation and the cache.
- Replies stream. A stand-in can be cut in two across stream events, and tool
  call arguments are JSON text inside JSON, so a restored value must be
  escaped for that context.
- Some strings must reach the provider byte for byte: signatures, encrypted
  reasoning, identifiers, enums, media, and reasoning text the provider signs
  and verifies when it is sent back.
- The user is not a security engineer. Asking them to pick categories of data
  would be a decision they cannot make well.

## Decision

1. **One switch, on by default.** `privacyProtection` covers secrets and
   personal data together. It is stored as `aimgr.privacyProtection` in the
   product settings KV table through `SettingsStore`
   (`load_privacy_protection` / `save_privacy_protection`), outside
   `ProductSettings` so it has a single write path. A missing or unreadable
   value means on. Backup restore and archive import keep it, like the other
   per-machine preferences (`SettingsStore::preserve` / `reinstate`). No
   schema change.
2. **Stable, keyed placeholders.** A detected value becomes
   `{{KIND_xxxxxxxx}}`: the kind (`PRIVATE_KEY`, `API_KEY`, `TOKEN`,
   `PASSWORD`, `SECRET`, `EMAIL`, `ID_NUMBER`, `PHONE`, `BANK_CARD`) and the
   first 40 bits of HMAC-SHA256(install key, value) in lowercase base32. The
   same value always gets the same placeholder, so history and prompt caches
   stay stable. The 32-byte install key lives in
   `privacy-protection.key` (owner-only, hex) in the product data directory,
   is created on first start, never enters the database, backups, logs or IPC,
   and survives restarts. If the file cannot be written the proxy uses a key
   for that run only rather than turning protection off.
3. **Bounded memory for the way back.** A placeholder-to-value map holds at
   most 4,096 entries; each masked request refreshes the entries it uses, and
   when the map is full the least recently used half is dropped. A dropped
   value is masked again, to the same placeholder, the next time it is sent.
   Values recognised only by their context (`password=…`, `user:…@host`) are
   also remembered (up to 256, at least six characters) and masked verbatim
   wherever they appear later, so a value the model repeats without the
   surrounding assignment does not go out in clear on the next turn.
4. **What is detected**, only inside JSON string values, never re-masking an
   existing placeholder, every pattern bounded so a match inside a longer
   token does not count:
   - Secrets: PEM private key blocks; `sk-` keys (including `sk-ant-`,
     `sk-proj-`, `sk-or-`); `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`,
     `github_pat_`, `glpat-`; `AIza`; `xox?-`; `sk_live_`/`rk_live_`;
     `AKIA`/`ASIA`; `hf_`, `gsk_`, `xai-`, `npm_`, `pypi-`, `dop_v1_`;
     `SG.x.y`; JWTs; the password in `scheme://user:password@host`; values
     of assignments whose name contains password, passwd, secret, token,
     api_key, access_key, private_key or credential (value at least eight
     characters with letters and digits, not a template such as `${VAR}`,
     `<password>` or `****`, not a call or a dotted reference).
   - Personal data: email addresses (not example domains, reserved test
     domains, no-reply senders, `git@` remotes or `@2x` image names);
     Chinese mainland ID numbers with a valid checksum; Chinese mobile
     numbers with optional `+86`; bank card numbers that pass the Luhn check.
5. **What is never touched.** Strings under `signature`, `encrypted_content`,
   `data`, `thoughtSignature`/`thought_signature`, `model`, `id`,
   `tool_use_id`, `call_id`, `item_id`, `type`, `role`, `name`,
   `previous_response_id`, `prompt_cache_key`, `media_type`/`mime_type`/
   `mimeType`, `url`, `image_url`, `file_id`/`fileUri`/`file_uri`,
   `reasoning_effort`/`effort`, `stop_reason`/`finish_reason`, `status`,
   `object`, `event`, and any `data:` URL. Signed `thinking` text is neither
   masked nor restored: it only ever contains what the model wrote, and it
   must go back exactly as the provider issued it or the signature check
   fails.
6. **Restoring replies.** A router layer restores placeholders in every reply
   the proxy sends to a tool, whichever handler produced it:
   - JSON bodies are parsed and every string is restored; values inside
     `arguments` / `partial_json` (JSON text) are JSON-escaped. A body
     without `{{` is passed on byte for byte.
   - SSE streams are restored event by event for Anthropic Messages, OpenAI
     Chat Completions, OpenAI Responses and Gemini. When a text delta ends
     with something that could be the start of a placeholder, the event is
     held back with that tail removed; the tail is joined to the next delta
     of the same block and restored whole there. If the next event belongs to
     anything else, the held event is released unchanged. Untouched events are
     forwarded byte for byte.
   - Unknown placeholders are left as they are. Replies are restored even
     right after the switch is turned off, so a request masked just before
     still comes back whole.
7. **Where the code lives.** The engine is a product-owned module inside the
   proxy, `src-tauri/src/proxy/privacy/`, because it works on the proxy's own
   request and response types. The product reaches it only through
   `compat/ccswitch/proxy_privacy.rs`; the application service
   `application/privacy_protection.rs` reads the switch and the key at
   startup and applies changes. The renderer sees only
   `{ enabled: boolean }` through `app_privacy_protection_get` /
   `app_privacy_protection_set`, and a self-contained
   `PrivacyProtectionSwitch` component (`src/features/routing-privacy/`).
8. **Nothing sensitive is logged.** The proxy logs only how many values it
   replaced in a request, at debug level. No value, placeholder or key is
   logged or sent to the renderer, and the existing request logs are
   unchanged.

Inherited upstream files changed, each by one call site:

- `src-tauri/src/proxy/mod.rs`: `pub(crate) mod privacy;`
- `src-tauri/src/proxy/forwarder.rs`: `mask_request_body` on the final
  outbound body, after the private-parameter filter and local overrides.
- `src-tauri/src/proxy/server.rs`: the `restore_response` router layer.

Product-owned files also touched: `lib.rs` (command registration and the
startup call), `commands/app_routing_api.rs`, `compat/ccswitch/settings.rs`,
`compat/ccswitch/backup.rs`, `compat/ccswitch/backup/transfer.rs`. No
dependency was added: HMAC-SHA256 is built on the existing `sha2`, the key
comes from OS randomness through the existing `uuid` v4 generator (two UUIDs,
244 random bits).

## Consequences

- Positive: secrets and personal data in proxied requests stay on the machine
  by default, without the user choosing anything, and tools keep working
  because every reply gets the real values back.
- Positive: placeholders are stable across turns and restarts, so the
  provider's prompt cache and the model's view of the conversation are not
  disturbed.
- Only proxied traffic is covered. A tool that talks to its provider directly,
  without takeover, is not protected, and the switch has no effect on it.
- False positives are harmless to the result: whatever was replaced is put
  back on the way to the tool, so a false positive only changes what the model
  sees (a placeholder instead of, say, a test card number).
- Detection is pattern based. A secret with no recognisable shape and no
  telling name (`hunter2` on its own line) is sent as is.
- Some edge cases pass a placeholder through to the tool: a reply with a
  content encoding the proxy cannot rewrite, a placeholder whose value was
  dropped from the map while the reply was still streaming, a split
  placeholder whose two halves are interleaved with another block's events,
  or two different values colliding on 40 bits. Each is rare and fails
  visibly (the tool sees `{{KIND_…}}`), never by leaking a value.
- Non-SSE streamed JSON (a Gemini `streamGenerateContent` reply without
  `alt=sse`) is buffered whole before it is restored, so it arrives at once
  instead of progressively while any value is being protected.
- Masking costs a pass of bounded regular expressions over the string values
  of each request; replies are only parsed while at least one value is held.
- When the inherited `src-tauri/src/proxy/` directory is refreshed from
  upstream by per-file checkout, the three call sites above must be
  re-applied; `proxy/privacy/` itself is product-owned and not overwritten.

## Alternatives rejected

- **Separate switches for secrets and personal data.** A choice the user
  cannot evaluate adds weight without adding safety; one switch, on by
  default, is the whole feature.
- **Random or counter-based placeholders.** They change between requests,
  which rewrites history the model already saw and defeats prompt caching.
- **An unkeyed hash of the value.** A provider could confirm a guessed phone
  number or ID number by hashing it; the install key prevents that.
- **Redacting without restoring.** The model would write placeholders into
  files and commands, breaking the user's work.
- **Restoring by plain text replacement on the byte stream.** It cannot see a
  placeholder split across SSE events and cannot know when a value sits
  inside JSON-encoded tool arguments and must be escaped.
- **Masking in each request handler and each response path.** The proxy has
  many handlers and translated reply shapes; one hook on the final outbound
  body and one router layer on the reply cover all of them with the smallest
  change to inherited files.
- **Masking signed reasoning text.** A model-written example value inside it
  would be altered on the next turn and the provider would reject the
  signature.
- **Storing the switch in `ProductSettings`.** The full-replace settings save
  and a dedicated command would then both write the same key from two cached
  copies.
