# ADR-0059: Home names a model by its own name, and an address by its site

- Status: accepted
- Date: 2026-09-27
- Extends: ADR-0055 (Home chooses the model and effort). Display only; what
  ADR-0055 writes to the tools' files is unchanged.

## Context

The owner saw two rows on Home that looked like two different services:

| Row | Mark | Model |
|---|---|---|
| Claude Code | `ON` | `anthropic/claude-opus-5-5` |
| Codex CLI | `AP` | `gpt-5.6-sol` |

Both rows are connected to the same Onlist server. Neither difference comes
from the server:

- **The model** on the pill is the value in the tool's own file. The Claude
  Code value was picked from the Onlist catalogue, whose ids carry the lab
  (`author/model`, as on OpenRouter); the Codex value was typed by hand long
  ago. Claude Code reads both spellings the same way: its model parser drops
  everything up to the last `/` and treats a trailing `[1m]` as the context
  size (checked in Claude Code 2.1.280).
- **The mark** is the initials of the endpoint's name. The Claude Code row
  uses a saved endpoint named `onList`. The Codex address was set outside the
  app, so its name is the host `api.onlist.net`, and the initials of a host
  are nearly always `AP` or `WW`.

This gets worse, not better, as gateways follow OpenRouter's Anthropic-shape
catalogue, which Onlist now serves too (Onlist ADR-0129). A request with the
`anthropic-version` header, which this product already sends for a Claude
Code endpoint, gets ids like `anthropic/openai/gpt-5.6-sol[1m]`. The
`anthropic/` wrapper lets Claude Code accept a non-Claude model, and `[1m]`
gives it the one-million-token window.

magpie shows the catalogue's display name and keeps the raw id for the file.
This product's catalogue carries ids only, so the name is taken from the id.

## Decision

1. **A model reads as Claude Code reads it.** `describeModel` takes the id
   after its last `/` as the name. The namespace right above it, usually the
   lab, and a `[1m]` or `[2m]` marker (shown as `1M` or `2M`) go into the row's
   note. The pill shows the name, plus the context size when there is one.
   The whole id stays in the tooltip on the pill and on each row. The filter
   also matches it, so a pasted id still finds its row.
2. **The id written to the file does not change.** A row still writes exactly
   the id the endpoint listed, `[1m]` included. That is what the endpoint
   routes on and what gives Claude Code its context size.
3. **A host-like name is marked by its site.** `initials` first reduces a
   dotted host name to the label a person calls the site, so `api.onlist.net`
   becomes `onlist` and then `ON`. A port is ignored. `example.co.uk` gives
   `example`. Plain names, `localhost` and IP addresses are marked as before.

## Consequences

- One model looks the same on every row and in every endpoint's list,
  whichever spelling its endpoint uses.
- Two catalogue entries that differ only in their namespace, such as
  `openai/gpt-5` and `azure/gpt-5`, share a name. The note tells them apart.
- A dotted id such as OpenRouter's `anthropic/claude-opus-5.5[1m]` still
  shows as `claude-opus-5.5`, and Claude Code still reads it as Opus 5.
  Onlist serves the hyphen form for this reason. Rewriting another gateway's
  ids is not this product's call, because the gateway routes on them.
