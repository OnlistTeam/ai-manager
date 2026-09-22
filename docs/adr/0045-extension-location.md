# ADR-0045: Naming the file an extension list writes to

- Status: accepted
- Date: 2026-09-22
- Relaxes one clause of the rule in `domain/extension.rs`. Follows the
  precedent set by ADR-0042.

## Context

Two complaints about the Extensions page, and they turned out to be the same
one:

1. "Skills can already edit the file and open its folder — why can't MCP?"
2. "Can't each entry show its file path?"

The first was nearly true. Skills carry three actions on each card: open
location, copy to tool, edit document. MCP and global instructions had exactly
one action, `ExtensionLocationButton`, tucked into the page header next to the
help toggle, and it could only reveal the file in Finder — never open it. Two
surfaces of the same page behaved like two different products.

The second ran into a deliberate rule. `domain/extension.rs` opens with:

> **This type deliberately has no field that could hold a config payload** —
> no `server`, no `content`, no paths. Spec §36 requires "do not show JSON on
> first entry", and deleting those fields from the wire format is far more
> reliable than asking the frontend to "be careful not to display them".

That rule is about payloads. A path is not a payload, and the reason for the
rule — don't confront a non-technical user with raw config — is not served by
hiding *which file* is about to change. The services page already names
`~/.config/zsh/secrets.zsh:9` on the effective-connection card (ADR-0042) and
`ProviderRuntimeResource` has carried a `path` since it shipped. Withholding it
here was an inconsistency, not a protection.

## Decision

**Name the file once, above the list, not on every card.** The shapes differ
and the UI should follow them rather than average them:

- A Skill is a directory of its own, so its path is per entry and its actions
  belong on its card. That is what already exists; nothing moves.
- MCP servers and global instructions all live in the one JSON or TOML the
  tool itself reads. Their path is a property of the scope, so
  `ExtensionLocationRow` states it once above the list, with copy, "Open file",
  and "Show in file manager" beside it.

Repeating one identical path under N cards would be the same sentence N times,
which is the cognitive load the complaint was about, not a cure for it.

**Add `ExtensionLocation` rather than a field on `Extension`.** The no-paths
rule on `Extension` stays exactly as written; the new type is a separate,
read-only answer to a separate question. It carries the kind, a display path
with `$HOME` abbreviated to `~`, and whether the file exists yet. No contents
cross the boundary, and the renderer sends back the scope and kind — never
this string — so the open command still cannot be pointed at an arbitrary path.

**Say when the file is not there yet.** A tool that has never written its
config is the ordinary first-run state. "Open file" is disabled with one line
explaining why, rather than handing an editor a path that does not exist.
"Show in file manager" stays enabled, because the backend falls back to the
nearest existing folder, which is how someone finds out where the file *would*
go.

## Consequences

- `ExtensionLocationButton` is replaced by `ExtensionLocationRow`; the header
  loses a button and the list gains a header line, which is a net reduction in
  things to hunt for.
- Two new commands, `app_extension_location_describe` and
  `app_extension_location_open`. `app_extension_location_reveal` stays as a
  thin alias so nothing else had to change in one go.
- `$HOME` is abbreviated, so the account name never appears in a screenshot of
  this row.

## Alternatives rejected

- **A path field on `Extension`.** It would put the same string on every card
  and relax the rule for every consumer of the type, including ones with no
  reason to have it.
- **Giving MCP entries per-row edit buttons like Skills have.** They would all
  open the same file. Three buttons that do one thing is worse than one button
  that says what it does.
- **Leaving it alone and explaining the asymmetry in the help text.** The
  asymmetry was not principled; it was just where the work had stopped.
