# ADR-0031: Features Always Available, Complex Information Progressively Disclosed at the Point of Use

- Status: Accepted
- Date: 2026-08-30
- Supersedes: the navigation visibility decisions in ADR-0006 / ADR-0007 / ADR-0008 / ADR-0014, and the Advanced Mode visibility decision in ADR-0023 decision 2

> 2026-09-05: ADR-0032 once folded the secondary areas under "More tools"; on the same day ADR-0034 changed this to
> a five-item sidebar with the secondary areas becoming tabs of their parent pages. Neither introduces a mode gate.

## Context

The product once used `advancedMode` to simultaneously control page entries, Provider custom endpoints, and
technical details. In practice this forced users to first understand an abstract "mode" before completing the most
common tasks: choosing software, viewing its API endpoints, adding or switching endpoints. As a result the Base URL
was invisible in the primary "add endpoint" flow. Meanwhile large blocks of runtime context, headers, paths, and
health details occupied the first screen and increased scanning cost.

Upstream CC Switch's Provider capability remains the compatibility focus: the tools' native configuration shapes,
the Provider switch transaction, and the reviewed presets continue to be reused, but the product does not need to
replicate upstream's full information density.

## Decision

1. Remove the user-visible Advanced Mode switch and the route visibility gate. All managed capabilities are
   directly available. `advancedMode` is kept only as a wire-format compatibility field for old databases and old
   builds; Application forcibly normalizes it to `true` on read and write, and the stored value no longer changes
   the product's capability set.
2. Complexity is instead disclosed progressively where the feature lives rather than by a global mode: request
   headers, multi-routing, runtime context, backup paths, and ready/info health signals are collapsed by default;
   primary status, primary actions, and errors are always shown directly.
3. The primary path of the API endpoints page is fixed to "choose tool → view endpoints → add/switch". A
   non-empty list uses a compact heading, endpoint count, add and launch actions, and no longer repeats a
   full-screen summary card before the list.
4. "Add API endpoint" shows Name, Base URL, API Key, and Model directly by default. Provider presets are kept as a
   secondary quick-fill entry and not deleted: they carry each tool's reviewed protocol shape and default model
   and continue to help the product follow CC Switch upgrades. The compatibility recovery flow can still open the
   preset catalog directly.
5. The native typed boundary is unchanged. Custom HTTPS endpoints continue to go through
   `ProviderCustomCreateDraft`; the renderer gains no arbitrary raw configuration or protocol conversion
   capability, and tool live configuration is still written by the existing compatibility layer and transaction.

## Consequences

- Users can find the Base URL or use the shipped pages without first judging whether they are "advanced".
- Presets change from a mandatory choice into an optional accelerator, reducing cognitive load while keeping the
  knowledge layer aligned with upstream Provider evolution.
- Old databases need no migration and old builds can still parse settings; new builds no longer re-hide
  capabilities because of an old `false` value.
- Any complex field added later must first decide its default collapse level and must not reintroduce a global
  product mode.
