# ADR-0049: Privacy protection for proxied requests

- Status: accepted
- Date: 2026-09-26 (settings revised the same day: three choices instead of one switch)

## Context

When a tool is taken over by the local routing proxy (ADR-0007), every request
passes through AI Manager before it reaches the provider. Those requests carry
far more than the prompt: whole files read by the agent, command output, `.env`
contents, git configuration, pasted logs. Keys, passwords, private keys, email
addresses, phone numbers, ID numbers and bank card numbers regularly end up in
them without the user noticing, and so do names the user would rather keep to
themselves: people, customers, project codenames, internal host names. The
provider never needs the real values to do its job; the tool on the user's
machine does.

Because the proxy already sees and rewrites both directions of this traffic,
it can keep the real values on the machine: send a stand-in, and put the real
value back when the stand-in comes back in the reply.

Constraints that shape the design:

- Conversation history is resent on every turn, and providers cache prompt
  prefixes. A masked body that changes between requests for the same history
  would break both the model's view of the conversation and the cache.
- Replies stream. A stand-in can be cut in two across stream events, and tool
  call arguments are JSON text inside JSON, so a restored value must be
  escaped for that context.
- Some strings must reach the provider byte for byte: signatures, encrypted
  reasoning, identifiers, enums, media, and reasoning text the provider signs
  and verifies when it is sent back.
- The user is not a security engineer. Choices must be few, named by what they
  hide, and safe by default.

## Decision

1. **Three settings, stored in the product settings KV table** through
   `SettingsStore` (`load_privacy_protection` / `save_privacy_protection`),
   outside `ProductSettings` so they have a single write path:

   | Setting | KV key | Default |
   | --- | --- | --- |
   | Hide keys and passwords | `aimgr.privacy.maskSecrets` (`"true"`/`"false"`) | on |
   | Hide personal information | `aimgr.privacy.maskPersonal` (`"true"`/`"false"`) | off |
   | Hide these words | `aimgr.privacy.words` (JSON array of strings) | empty |

   A missing, empty or unrecognised value reads as its default. Personal
   information is off by default because test data (sample emails, card
   numbers that pass Luhn, generated ID numbers) trips it far more often than
   keys trip the key detectors, and a false positive changes what the model
   sees. Backup restore and archive import keep all three, like the other
   per-machine preferences (`SettingsStore::preserve` / `reinstate`). No
   schema change. The earlier single `aimgr.privacyProtection` key is not
   read any more.
2. **Stable, keyed placeholders.** A detected value becomes
   `{{KIND_xxxxxxxx}}`: the kind (`PRIVATE_KEY`, `API_KEY`, `TOKEN`,
   `PASSWORD`, `SECRET`, `EMAIL`, `ID_NUMBER`, `PHONE`, `BANK_CARD`, `WORD`)
   and the first 40 bits of HMAC-SHA256(install key, value) in lowercase
   base32. The same value always gets the same placeholder. The 32-byte
   install key lives in `privacy-protection.key` (owner-only, hex) in the
   product data directory, is created on first start, never enters the
   database, backups, logs or IPC, and survives restarts. If the file cannot
   be written the proxy uses a key for that run only rather than turning
   protection off.
3. **Bounded memory for the way back.** A placeholder-to-value map holds at
   most 4,096 entries; each masked request refreshes the entries it uses, and
   when the map is full the least recently used half is dropped. A dropped
   value is masked again, to the same placeholder, the next time it is sent.
   Values recognised only by their context (`password=…`, `user:…@host`) of
   at least six characters are also remembered (up to 256) and masked
   verbatim wherever they appear later, so a value the model repeats without
   the surrounding assignment does not go out in clear on the next turn.
4. **What is detected**, only inside JSON string values, never re-masking an
   existing placeholder, every pattern bounded so a match inside a longer
   token does not count:
   - Keys and passwords: PEM private key blocks; `sk-` keys (including
     `sk-ant-`, `sk-proj-`, `sk-or-`); `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`,
     `github_pat_`, `glpat-`; `AIza`; `xox?-`; `sk_live_`/`rk_live_`;
     `AKIA`/`ASIA`; `hf_`, `gsk_`, `xai-`, `npm_`, `pypi-`, `dop_v1_`;
     `SG.x.y`; JWTs; the password in `scheme://user:password@host`; values
     of assignments whose name contains password, passwd, secret, token,
     api_key, access_key, private_key or credential (value at least eight
     characters with letters and digits, not a template such as `${VAR}`,
     `<password>` or `****`, not a call or a dotted reference); and the
     remembered context-recognised values of item 3.
   - Personal information: email addresses (not example domains, reserved
     test domains, no-reply senders, `git@` remotes or `@2x` image names);
     Chinese mainland ID numbers with a valid checksum; Chinese mobile
     numbers with optional `+86`; bank card numbers that pass the Luhn check.
   - Words: the user's list. The field accepts commas (`,`, `，`, `、`) and
     line breaks between words; each word is trimmed, words under two
     characters and repeats are dropped, and more than 100 words or a word
     over 128 characters is refused with `error.privacy.tooManyWords` /
     `error.privacy.wordTooLong`. Matching is case-sensitive and whole-token:
     where a word starts or ends with a letter, digit or `_`, the neighbouring
     character must not be one too, so `acme` is not found in `acmeish` or
     `my_acme` but is in `acme-db`. Chinese, Japanese and Korean characters
     have no word boundaries, so they never block a match: `张三` is found in
     `张三丰`, `acme` in `acme公司`.
   - On overlap the first detector wins, in this order: keys and passwords,
     remembered values, personal information, words. Remembered values and
     words are tried longest first, then in byte order, so overlapping
     entries resolve the same way whatever order they were learned or typed.
5. **What is never touched.** Strings under `signature`, `encrypted_content`,
   `data`, `thoughtSignature`/`thought_signature`, `model`, `id`,
   `tool_use_id`, `call_id`, `item_id`, `type`, `role`, `name`,
   `previous_response_id`, `prompt_cache_key`, `media_type`/`mime_type`/
   `mimeType`, `url`, `image_url`, `file_id`/`fileUri`/`file_uri`,
   `reasoning_effort`/`effort`, `stop_reason`/`finish_reason`, `status`,
   `object`, `event`, and any `data:` URL, whichever settings are on. Signed
   `thinking` text is neither masked nor restored: it only ever contains what
   the model wrote, and it must go back exactly as the provider issued it or
   the signature check fails.
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
     right after a setting is turned off, so a request masked just before
     still comes back whole.
7. **Prompt-cache safety.** The masked body is a pure, deterministic function
   of (request body, settings, install key, remembered context-recognised
   values). `Engine::mask_value` works in two passes: it first reads the
   whole body for context-recognised values (walking exactly the strings it
   will mask, including decoded tool-call arguments), merges them with the
   remembered ones, and only then masks every string with that one fixed
   list. So:
   - the same history masks to the same bytes on every turn, and a masked
     turn is a byte-identical prefix of the next turn;
   - a reply the tool received with real values and sends back as history is
     masked again to exactly the placeholders the provider wrote (keys and
     personal data by their patterns, words by the list, passwords by the
     context still present in the history);
   - message order cannot change the result: a value named in a later
     message is also masked in an earlier one, on the first request already;
   - after a restart (empty map, same install key) the same history masks to
     the same bytes whenever the context that identified a value is in the
     same request; values remembered from unrelated requests do not change
     it.

   **The one case that still rewrites earlier bytes**, kept on purpose: a
   value that first went out bare and is recognised only later. Turn 1 says
   "try Tr0ub4dor9x"; turn 3 says `DB_PASSWORD=Tr0ub4dor9x`; from turn 3 on,
   turn 1's text is masked too, so the cached prefix from turn 1 misses once.
   Hiding the value from then on is worth one cache miss. Its mirror image is
   the same case seen from memory: a value recognised only in another request
   and remembered is hidden in this conversation until a restart forgets it
   (or it drops out of the 256-value memory), and then goes out bare again
   until its context is seen again.

   Changing a setting or the word list changes the masked bytes once, as
   expected; the cache warms up again on the next turn.

   Tests (`proxy/privacy/cache_tests.rs`): identical bytes for the same
   history with all three settings on; turn N as a byte-identical prefix of
   turn N+1; a restored reply re-masked to the provider's placeholders;
   whole-body learning independent of message order; identical bytes after a
   restart; the one remaining case above, in both directions; a changed
   setting changing the bytes once and then staying stable; each setting
   masking only its own category; the never-touched keys unchanged with
   everything on.
8. **Where the code lives.** The engine is a product-owned module inside the
   proxy, `src-tauri/src/proxy/privacy/` (`rules.rs` for the compiled
   settings, `words.rs` for the word matcher), because it works on the
   proxy's own request and response types. The product reaches it only
   through `compat/ccswitch/proxy_privacy.rs` (`configure` at startup,
   `apply` after a change); the application service
   `application/privacy_protection.rs` reads the settings and the key at
   startup, applies a patch, stores it and hands the proxy what was read
   back. The renderer sees `{ maskSecrets, maskPersonal, words }` through
   `app_privacy_protection_get` and sends a partial patch through
   `app_privacy_protection_set` (unknown fields refused), which returns the
   full stored state. The settings page has a Privacy section
   (`src/pages/settings/PrivacyCard.tsx`) with two switches and the words
   field saved on blur or Enter; the live routing card on the home page and
   the local routing page shows one status line with a link to that section
   (`src/features/routing-privacy/PrivacyStatusLine.tsx`).
9. **Nothing sensitive is logged.** The proxy logs only how many values it
   replaced in a request, at debug level. No value, placeholder, word or key
   is logged or sent to the renderer; `Debug` for the settings prints only
   how many words there are. The existing request logs are unchanged.

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

- Positive: keys and passwords in proxied requests stay on the machine by
  default, without the user choosing anything; personal information and the
  user's own words are one switch or one field away. Tools keep working
  because every reply gets the real values back.
- Positive: the masked history is stable across turns and restarts, so the
  provider's prompt cache and the model's view of the conversation are not
  disturbed, apart from the one documented case.
- Only proxied traffic is covered. A tool that talks to its provider directly,
  without takeover, is not protected, and the settings have no effect on it;
  the settings section says so.
- False positives are harmless to the result: whatever was replaced is put
  back on the way to the tool, so a false positive only changes what the model
  sees (a placeholder instead of, say, a test card number).
- Detection is pattern based. A secret with no recognisable shape and no
  telling name (`hunter2` on its own line) is sent as is unless the user adds
  it to the word list.
- A short or common word in the list (a first name that is also an ordinary
  word) is replaced everywhere it stands alone; the model then sees a
  placeholder there. The user chose the word, so this is expected.
- Some edge cases pass a placeholder through to the tool: a reply with a
  content encoding the proxy cannot rewrite, a placeholder whose value was
  dropped from the map while the reply was still streaming, a split
  placeholder whose two halves are interleaved with another block's events,
  or two different values colliding on 40 bits. Each is rare and fails
  visibly (the tool sees `{{KIND_…}}`), never by leaking a value.
- Non-SSE streamed JSON (a Gemini `streamGenerateContent` reply without
  `alt=sse`) is buffered whole before it is restored, so it arrives at once
  instead of progressively while any value is being protected.
- Masking costs two passes of bounded regular expressions over the string
  values of each request (the learning pass runs only the key and password
  detectors), plus one substring search per word; replies are only parsed
  while at least one value is held.
- When the inherited `src-tauri/src/proxy/` directory is refreshed from
  upstream by per-file checkout, the three call sites above must be
  re-applied; `proxy/privacy/` itself is product-owned and not overwritten.

## Alternatives rejected

- **One switch for everything.** It forced a choice between keeping keys out
  of requests and keeping test data readable for the model; separating
  personal information lets the safe part stay on for everyone.
- **A per-detector list of switches.** Choices the user cannot evaluate add
  weight without adding safety; two categories named by what they hide, plus
  the user's own words, are the whole feature.
- **Case-insensitive or regular-expression words.** Case-insensitive matching
  would hide ordinary words that happen to share a name's spelling;
  expressions are a tool for engineers, not for this audience.
- **Learning context-recognised values while masking, in message order.** A
  value named in a later message would stay bare in an earlier one on the
  first request and be masked there on the next, rewriting history and
  missing the cache.
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
- **Storing the settings in `ProductSettings`.** The full-replace settings
  save and the dedicated command would then both write the same keys from two
  cached copies.
- **Persisting the remembered context-recognised values.** They are the very
  values being protected; keeping them only in memory is what makes the
  restart case above exist, and it is the right trade.
