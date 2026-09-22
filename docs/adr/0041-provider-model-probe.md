# ADR-0041: Provider Model Probe

- Status: Accepted
- Date: 2026-09-21
- Extends: ADR-0039

## Context

Every outbound provider probe in this product is a bare `GET` on the saved base
URL. It carries no path, no credential and no body, it never reads the response,
and it treats any HTTP status as reachable. That design is deliberate and pinned
by tests: `services/stream_check.rs` asserts no credential header is ever sent,
`services/speedtest.rs` asserts the same for the batch path, and both
`ProviderTestResult` and `ProviderEndpointTestResult` deliberately carry no
message and no URL so nothing about the address can leak back through IPC.

The cost of that design is that the four most common real failures — a wrong
key, an expired key, a model name the service does not serve, and an exhausted
balance — all report "address reached". The check proves the host is up. It does
not prove the service is usable.

Proving usability requires exactly the three things every existing probe
refuses to do: send the credential, read the response body, and return a display
string to the renderer.

## Decision

1. Outbound provider probes are split into two classes with different rules.

   **Reachability probes** (`app_provider_test`, `app_provider_test_all`,
   preflight, failover, endpoint and preset speed tests) keep every existing
   constraint unchanged. They stay free, credential-free and body-free because
   they run unattended: across every saved service during a health check, in
   front of every tool launch, and concurrently across failover candidates.

   **Model probes** (`app_provider_models_list`, `app_provider_model_probe`) may
   send the saved credential, read the response body, and return the model's
   reply. They only ever run from an explicit user click on one named service.

2. Sending the saved key to the address the user configured is not a new
   exposure. ADR-0039 already established that saved keys are visible and
   copyable on the card and in the edit form, and `domain/provider.rs` documents
   that the key is projected verbatim to the renderer. The key travels over TLS
   to the endpoint the user chose, which is the endpoint their CLI already sends
   it to on every request.

3. The log boundary does not move. On this path the URL, the key, the prompt and
   the reply never enter a log, a `Debug` rendering, or an error's technical
   message. Logs carry the provider ID, the wire protocol, the latency and the
   HTTP status. `ModelProbeOutcome` and `ModelProbeReply` implement `Debug` by
   hand and render the reply as `<redacted>`, following `Provider` and
   `ProviderEndpointCandidate`.

4. The wire protocol is derived from a total `ToolId` table, not from a user
   choice and not from scattered conditionals. A new tool cannot compile without
   declaring its protocol.

5. Generated images cross IPC as base64 and render as a `data:` URL. The
   application CSP is `img-src 'self' data:`, so a remote image URL could not
   render anyway, and a remote URL returned by an upstream service is a tracking
   vector pointed at the user's machine. `response_format` is always `b64_json`.

6. A model probe spends the user's money. The cost is stated in place next to
   the send button rather than behind a confirmation dialog, because spec §100
   forbids gating an action behind a mode prompt. Requests are bounded: one
   image, `max_tokens` 300, a prompt of at most 200 characters, and no
   streaming.

7. The reachability probe keeps its place inside the model-probe dialog as the
   first diagnostic line. It is what distinguishes "the address is dead" from
   "the address answered and rejected your key", and it still writes to the same
   connectivity cache, so the home health check and the failover flow are
   unaffected.

## Verification and Limits

Adapter tests cover path joining against bases that do and do not end in a
version segment (including `/v10` and `/v1beta`, which must not match `/v1`),
the three authentication header shapes, the Anthropic bearer retry, catalogue
parsing and filtering, the text/image name heuristic including the negative
cases (`gpt-4o`, `qwen-vl` and other image-consuming models must resolve to
text), reply extraction with the `reasoning_content` fallback, and the response
size cap. A loopback server test asserts the inverse of the reachability
invariant: this path must send the credential header, while the outcome's
`Debug` rendering must contain neither the key nor the reply.

The heuristic is a guess. It is presented as an overridable default, never as a
fact, because most aggregator `/v1/models` responses carry only an id. Services
that do not implement a model catalogue degrade to a free-text model field
rather than blocking the test. Image generation is offered only on the
OpenAI-compatible protocol; the Anthropic and Gemini endpoints have no image
endpoint, so the control is absent rather than present and guaranteed to fail.

A successful model probe proves the service answered one request at one moment.
It is not a quota check, not a rate-limit check, and not evidence that a tool's
live session is using this service.

## Amendment 2026-09-22: the probe must fail where the tool fails

A user changed a Codex service's endpoint to `https://api.onlist.net` and Codex
answered:

> unexpected status 403 Forbidden: This path is not available on the relay.,
> url: https://api.onlist.net/responses

The address was missing `/v1`. Codex builds its URL by literal concatenation
(`codex-rs/codex-api/src/provider.rs`, `url_for_path`: `format!("{base}/{path}")`
with `path` = `/responses`), so a base URL without a version segment reaches
`/responses`, and the relay in front of that host only forwards `/v1/`,
`/v1beta/` and four media prefixes. Everything else gets exactly that 403.

The defect is what this dialog did with the same configuration. Decision 4 above
put Codex on the `OpenAi` protocol, which addresses `chat/completions` **and
supplies a missing version segment**. So the probe sent
`https://api.onlist.net/v1/chat/completions`, got a 200, and reported the
service as working — while Codex could not use it at all. A test that passes
where the tool fails is worse than no test: it moves the user's suspicion away
from the one thing that was actually wrong.

Three changes follow.

**Codex gets its own wire protocol.** `ProviderWireProtocol::OpenAiResponses`
differs from `OpenAi` in the two things that decide whether the test is honest:
the path (`responses`) and the refusal to normalise the address.
`normalizes_version_segment()` is the predicate, and `text_url` is the only
caller, so there is one place where "reproduce the tool's own arithmetic" is
expressed. The catalogue and the image route keep their version segment,
because neither is a route Codex ever calls — `/v1/models` is a convenience for
finding model names, not a claim about the address the tool will use.

**A failed address may be answered with a verified alternative.** When the text
probe gets a status that means "this address does not serve this route" (404,
405, 410, and 403 because that is what a path allowlist returns), the backend
computes the other spelling of the saved base URL — with a version segment if it
had none, without if it had one — sends the same request there, and reports it
only if that request succeeded. The user sees the real failure plus a button.

Two rules keep this from becoming the guess it must not be:

- **Never suggest without verifying.** The field is not set from a heuristic, a
  status code, or a string inspection. A successful HTTP response is the only
  thing that puts an address on screen. This is why a wrong key produces no
  suggestion: it is refused at both spellings.
- **Never suggest an address the user did not already save.**
  `alternate_base_url` is a pure function of the saved base URL that adds or
  removes one trailing segment. It never reads a `Location` header or a response
  body, so an upstream service cannot use this path to walk the user onto a
  different host. A test asserts scheme, host and port are preserved for every
  shape.

This narrows decision 3 rather than contradicting it: the address still never
enters a log, and `ModelProbeOutcome`'s hand-written `Debug` renders the
suggestion as `<redacted>` like everything else. What changed is that a
*derived* address may now cross IPC outbound — which costs nothing, because the
base URL is already visible and editable in the form the renderer owns.

It also qualifies decision 6. A verification is a second billable request. It
only ever happens after a failure, so the first attempt generated nothing, and a
test pins that a successful probe sends exactly one request: the recovery path
must never be paid for on the happy path.

**Auto-completion was rejected.** Appending `/v1` when it looks missing is the
obvious fix and it is wrong: 5 of the 83 bundled Codex presets carry no version
segment at all (`api.aicoding.inc`, `api.aigocode.app`, `api.deepseek.com`,
`api.fenno.ai`, and an `aicodemirror` path mounted at `/api/codex/backend-api/codex`).
A static warning was rejected for the same evidence: a caution that is wrong for
6% of this product's own curated list is a caution users learn to dismiss, and
it adds a judgement call to a field that should not need one. The Base URL hint
now says what is actually true for every tool — copy the address exactly as the
provider's documentation writes it, because providers differ — and the dialog
does the rest by asking the server instead of guessing.

### Consequences

- `ProviderWireProtocol` gains a fourth variant, so every match over it had to
  declare an answer for Codex. That is the mechanism working as intended.
- Image generation is now offered on `OpenAiResponses` as well. The claim was
  always about the host rather than the tool: a relay that answers `responses`
  is an OpenAI-family host and usually serves `images/generations` too.
- A Codex service whose relay implements `chat/completions` but not `responses`
  will now fail this test. That is correct — Codex would fail too — where the
  previous behaviour reported success.
