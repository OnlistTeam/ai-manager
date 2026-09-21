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
