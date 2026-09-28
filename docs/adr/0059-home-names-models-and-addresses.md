# ADR-0059: Home names a model by its own name, and an address by its site

- Status: accepted
- Date: 2026-09-27
- Amended: 2026-09-28, decisions 2 and 4 (a catalogue model with a 1M
  window is written with Claude Code's `[1m]`)
- Extends: ADR-0055 (Home chooses the model and effort). Decisions 1 and 3
  are display only; decision 4 changes what a Home pick writes.

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
2. **The id written to the file is the endpoint's.** A row writes the id the
   endpoint listed, `[1m]` included, and adds nothing but the context marker
   of decision 4. That is what the endpoint routes on and what gives Claude
   Code its context size.
3. **A host-like name is marked by its site.** `initials` first reduces a
   dotted host name to the label a person calls the site, so `api.onlist.net`
   becomes `onlist` and then `ON`. A port is ignored. `example.co.uk` gives
   `example`. Plain names, `localhost` and IP addresses are marked as before.

4. **A long window is written with the tool's marker.** The owner picked
   `claude-opus-5-5` on Home and Claude Code ran it with a 200K window.
   Claude Code reads the window only from a `[1m]` at the end of the model
   name; without it every model gets 200K and is compacted long before a 1M
   model needs it. Claude Code drops the marker before the request and asks
   for the long window instead, so the endpoint sees the plain id (Onlist
   ADR-0129 observed the request; a Claude Code session on
   `anthropic/claude-opus-5-5[1m]` against Onlist runs with 1M).

   Onlist's catalogue gives every model its window (`context_length`, 1M for
   `anthropic/claude-opus-5.5`, 200K for `claude-haiku-4.5`), but in the
   OpenRouter shape, without the marker. Its ADR-0129 adds a Claude Code
   shape with `[1m]` already on the id; on 2026-09-28 neither `onlist.io`
   nor `api.onlist.net` served it yet, and other gateways never will.

   So the catalogue's window is now read along with the id: OpenRouter's
   `context_length` (at the top or under `top_provider`), Anthropic's
   `max_input_tokens`, Gemini's `inputTokenLimit`. The domain table gives
   each tool an optional context marker, a suffix and the smallest window it
   stands for; Claude Code's is `[1m]` from 1,000,000 tokens, and the other
   tools have none. It reaches the renderer with the choice, like the
   official list. A catalogue model whose window reaches it is offered with
   the suffix, which the row's note then shows as `1M`. An id that already
   ends in a marker, a model whose window the catalogue does not give, and
   every model of a tool without a marker are offered as listed. magpie
   does the same.

   There is no separate choice between the short and the long window: a
   model that has 1M runs with 1M, as it does in Claude Code's own `/model`
   list. A short window can still be typed into the filter.

## Consequences

- One model looks the same on every row and in every endpoint's list,
  whichever spelling its endpoint uses.
- Two catalogue entries that differ only in their namespace, such as
  `openai/gpt-5` and `azure/gpt-5`, share a name. The note tells them apart.
- A dotted id such as OpenRouter's `anthropic/claude-opus-5.5[1m]` still
  shows as `claude-opus-5.5`, and Claude Code still reads it as Opus 5.
  Onlist serves the hyphen form for this reason. Rewriting another gateway's
  ids is not this product's call, because the gateway routes on them.
- A model picked before decision 4 stays in the file without the marker. The
  list shows it as the model in use, apart from the same model with `1M` in
  its note, until the latter is picked.
- The window is read only by Home's model list. The endpoint edit form and
  the model test keep the catalogue's ids as listed, because the test sends
  its request directly and an endpoint need not accept the marker.
