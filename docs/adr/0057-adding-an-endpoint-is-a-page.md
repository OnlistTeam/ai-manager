# ADR-0057: Adding an endpoint is a page

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0046 (the 2026-09-22 amendment's "edit the address and it
  becomes custom", and the allowlist), design-spec §34.
- Amended by: ADR-0060 (the subscription card reads whether the tool is
  signed in).
- Amended by: ADR-0061 (the subscription card signs in from AI Manager).

## Context

"Add endpoint" opened a dialog whose services sat in a dropdown. The dialog was
small, each row named a default model, which read as the one model the service
could be used with, and the only way to a custom address was to pick a service
first and then overwrite its address.

magpie shows the same choice as a page of cards grouped by kind of service,
with Custom as its own card. We follow that layout. One difference from magpie
decides what the page can hold: magpie's gateway translates between
protocols, so one list serves every agent. An endpoint here is written straight
into the tool's own config, and nothing translates.

## Decision

1. **A page, not a dialog.** "Add endpoint" swaps the tab strip and the tool
   picker for a breadcrumb ("Endpoints › Add an endpoint for Claude Code").
   It is page state, not a route, and leaving the tab or switching tool lands
   back on the list, as does clicking API Endpoints in the sidebar again. Custom comes first, then Subscription, Model makers,
   Relays and Local, as compact one-line cards showing each service's address,
   never a model. Search filters by name or address and falls back to Custom.
   The speed test moved here and tags each card with its latency.

2. **Each tool lists only what speaks its protocol.** Claude Code gets services
   with an Anthropic endpoint, Codex those with a Responses endpoint, OpenCode,
   OpenClaw, Hermes and Pi those with chat completions, and Gemini CLI only
   Google and onList. A service a tool cannot use is left out, not shown
   disabled. Presets carry a `kind` (`vendor`, `relay`, `local`) for grouping.
   Services upstream does not carry are described once, by protocol, in
   `scripts/provider-preset-policy.mjs`, and the generator builds each tool's
   template from the endpoint it speaks. Where upstream already ships the same
   service for a tool, its template wins.

3. **Subscription is one card, for the tool's own sign-in.** magpie drives
   other products' CLIs to use Cursor, Copilot, Devin and Antigravity
   subscriptions; nothing here can. So the card is the entry that leaves the
   tool on its own account: Claude subscription, ChatGPT subscription, Google
   account, SuperGrok. `app_provider_restore_tool_login` puts back upstream's
   seeded official record through `ensure_official_seed_by_id` and is
   idempotent. The card never signs anyone in; its dialog says that happens in
   the tool and names how. Other tools have no subscription group.

4. **Local servers are the one loopback exception.** Ollama and LM Studio
   presets have `kind: local`, may point at `localhost` over plain HTTP, and may
   be saved without a key: native writes the placeholder `local`, because the
   tools refuse an empty key field and a local server ignores it. A `local`
   preset must point at a loopback host; every other kind stays HTTPS-only. The
   custom create path takes the same rule, which also fixes a form that
   accepted `http://localhost` only for the backend to refuse it.

5. **A preset's address is read-only; Custom is its own entry.** This reverses
   the 2026-09-22 amendment to ADR-0046, where editing the address turned the
   form into a custom one. The address stays on screen, so a key is never saved
   against an address nobody saw, but a different address is now the Custom
   card's job. The security rule is where it was: a preset submit sends only
   `preset_id`, a custom one goes through the custom path.

## Catalogue changes (2026-09-27)

Every address below was checked against the vendor's docs and answered 401 or
403 without a key; model ids come from each vendor's model list.

- **Added:** Kimi (international), Kimi Code, StepFun and StepFun (China),
  Qwen and Qwen (China) (Alibaba's own Model Studio), Mistral, Groq, Ollama
  Cloud, xAI for the chat tools, OpenCode Go and OpenCode Zen as relays, and
  Ollama and LM Studio as local servers.
- **Renamed:** upstream's region naming is replaced by ours, the plain name
  for the international platform and "(China)" for the mainland one. Its
  "Kimi" and "MiniMax" are the China platforms; its "Kimi For Coding" is Kimi
  Code's China host (`api.kimi.com`, per Kimi's own docs), now "Kimi Code
  (China)".
- **Removed:** Together AI, Novita and Nvidia, inference clouds reselling
  other labs' models. The relay group is onList, OpenRouter and OpenCode.
- **Left out on purpose:** xAI for Claude Code (its Anthropic compatibility is
  documented as deprecated); Ollama Cloud for Codex (its Responses API is
  stateless and does not replay the freeform tool calls Codex edits files
  with); Mistral and Groq for Claude Code (no Anthropic endpoint); magpie's
  TypeSafe Jev (a routing decision API, not a model endpoint); StepFun's Step
  Plan host (the same key, billed to the plan instead) and Alibaba's Coding
  Plan hosts (plan-only `sk-sp-` keys). Both are one custom entry away.
- **Not changed here, found while checking:** upstream presets still name
  `deepseek-v4-pro`, which DeepSeek now routes to `deepseek-flash`, and point
  MiniMax (China) at `minimaxi.com`, which still answers but whose docs moved
  to `minimax.cn`. Those templates come from upstream and are for a sync to
  settle.

## Consequences

- The picker component and its gallery are deleted; the add page and two
  dialogs replace them.
- The catalogue grows from 78 to 144 presets; Claude Code has 21 to choose
  from, Gemini CLI 2.
- Card logos not already in the tree (Groq, LM Studio, Z.ai, Mistral) come
  from Lobe Icons (MIT), credited in `THIRD_PARTY_NOTICES.md`.
- Whether a Kimi Code key from one region works on the other region's host is
  undocumented; both hosts are offered so the user picks their own.
