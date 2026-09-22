# ADR-0046: Which services ship a preset

- Status: accepted
- Date: 2026-09-22
- Extends: ADR-0001 (upstream is cherry-picked, never merged wholesale)

## Context

The inherited catalogue shipped 521 presets across 95 services. Auditing them
by who actually runs the address:

| | services | presets |
|---|---:|---:|
| labs, clouds, and OpenRouter | 56 | 263 |
| third-party key resellers | **39** | **258** |

Half the list was relays: services that buy capacity somewhere and sell access
under their own name. `9527CODE`, `AiHubMix`, `PackyCode`, `SubRouter`,
`TheRouter`, `CherryIN`, `SoleAPI` and 32 others.

Two of them wear a first party's name without being it. `QwenCloud`
(qwencloud.com) and `千问AI平台` (qianwenai.com) are not Alibaba — Alibaba's own
platform is bailian.console.aliyun.com.

None of the 521 carried a referral or affiliate parameter; every `websiteUrl`
and `apiKeyUrl` was checked. So this is not about monetisation. It is about
what a preset *is*: the user pastes their key into an address this application
put in front of them. That is an endorsement, and for 39 hosts nobody here has
audited it is one this project cannot make. The picker was even telling them
the list was "reviewed".

## Decision

**A preset exists only for a service that is internationally known and
answerable for itself** — the company that built the model, the cloud that
hosts it, or OpenRouter, which is an aggregator but a public and accountable
one. The list lives in `scripts/provider-preset-policy.mjs` with the reasoning
beside it. 521 presets become 78.

**This is a filter, not a deletion.** The preset sources are upstream CC Switch
files that are cherry-picked, so removing entries from them would be undone by
the next sync and would conflict on every one after that. Filtering where the
catalogue is generated means the rule holds without anyone remembering it, and
a relay added upstream tomorrow never reaches a build.

**onList leads each tool's list, and is not its default.** Position and
preselection are separate: the dialog opens on `defaultPresetId`, which stays
the tool's own vendor (Anthropic for Claude Code, OpenAI for Codex, Google for
Gemini CLI). Leading the list costs nobody the default they expect. The entries
live with the other non-upstream presets in the generator rather than in the
upstream sources, for the same conflict reason, and every model id in them was
checked against `https://onlist.io/v1/models`.

The address is `https://onlist.io/v1`, or the origin for the two tools that
append their own version segment (Claude Code sends `/v1/messages`, Gemini CLI
`/v1beta/…`). Not `api.onlist.io`, which is the China acceleration line and not
the public API.

**The "reviewed" claim is gone.** The picker said compatible services came from
a "reviewed preset list". With the list now limited to widely known providers,
the accurate sentence says what it covers and where everything else goes:
a custom endpoint, which takes any address. Nobody is blocked by an absence.

## Consequences

- A user who reached a reseller through a preset must now add it as a custom
  endpoint. Their saved services are untouched: `presetId` exists only on the
  create draft and is never persisted, so a preset is a template, not a link.
- The upstream sources still carry all 95 services. A release test asserts none
  of the resellers reach the shipped catalogue, and spot-checks seven by name
  so a broken filter fails loudly rather than by a count drifting.
- Two Rust tests and one release test asserted `>= 400` presets. That number
  was pinning the old intent; they now pin the new one — no tool is left empty,
  and onList leads each list.
- Chinese first-party vendors are out too: StepFun, Volcengine, Tencent,
  Baidu Qianfan, Xiaomi MiMo, Longcat, ModelScope, SiliconFlow, PPIO and others
  are real companies, but the criterion chosen was international recognition,
  and applying it selectively would make it no criterion at all. Reversing this
  for a named vendor is one line in the allowlist.
