# ADR-0052: Denser layout, one list with divider rows

- Status: accepted
- Date: 2026-09-26

## Context

At the default 1180 × 760 window most pages showed very little. The shell
kept an 88px band above every page title, sidebar items were 52px tall, every
control was one step taller than its label needed, and several collections
wrapped each item in its own large card: an API endpoint card with a footer
band of buttons was 210–245px tall, so the endpoints page showed one endpoint
and a half; a Global Prompt card had a 224px minimum height, so that page
showed two. The Skills and MCP pages had already moved to one list with
compact rows (ADR-0048), which showed three to four times as many items in
the same space without reading as crowded.

The goal is "show a lot on one screen, but keep it simple": the same content,
fewer boxes, less empty band, while controls stay comfortable to hit and text
stays readable.

## Decision

1. **One spacing scale, applied at the source.** The values live in the
   primitives and the shell, not in per-page overrides:

   | Where                                 | Before            | Now                            |
   | ------------------------------------- | ----------------- | ------------------------------ |
   | Shell top inset (macOS / other)       | 88–92px / 72–76px | 72px / 60px                    |
   | Shell bottom inset                    | 40–48px           | 32px                           |
   | Status bar material (macOS / other)   | 92px / 76px       | 76px / 64px                    |
   | Sidebar item height, gap under logo   | 52px, 24px        | 40px, 16px                     |
   | Page stack gap                        | 20–32px           | 16px                           |
   | Gap between page groups (Settings)    | 32px              | 24px                           |
   | Heading to its card (Settings group)  | 16px              | 8px                            |
   | `Card` padding sm / md / lg           | 12 / 16 / 24px    | 8 / 12 / 16px                  |
   | List row padding (`ListGroupRow`)     | 16px              | 12px vertical, 16px horizontal |
   | Modal padding, header/footer rule gap | 20px, 14px        | 16px, 12px                     |
   | Empty state vertical padding          | 48px              | 32px                           |
   | `Field` label–control gap             | 6px               | 4px                            |

2. **Control heights.** `Button` xs / sm / md are 28 / 32 / 36px (were
   28 / 36 / 40); `lg` stays 48px because only the home hero uses it.
   `Input` and selects are 36px, badges 20px, scope tabs 28px. Buttons keep
   their pill shape explicitly (`rounded-full`) instead of relying on the
   large radius token. Small targets never go below 28px and primary controls
   never below 32px; body text stays 14px and captions 12px.
3. **Heading levels.** `SectionHeader` gives the title size (20px) to the page
   `h1` only; section `h2`/`h3` use the heading size (16px), so a page reads
   as one title over its sections.
4. **Collections are one list with divider rows.** A collection of like items
   is a single `ListGroup` whose `ListGroupRow`s are separated by hairlines,
   never one card per item. A row puts identity, badges and actions on its
   first line; secondary facts share a second line; anything longer (errors,
   check results) appears only when it exists. The item in effect is marked by
   its badge plus a thin accent bar at the row's leading edge, not by a
   coloured card border. API endpoints (`ServiceCard`) and Global Prompts
   (`ExtensionCard`) now follow this, joining tools, desktop apps, Skills and
   MCP. The session list shows each conversation on two lines instead of
   three.
5. **Fewer boxes and less copy.** A box that only frames one control goes
   (the session search field). A description that restates its heading goes
   (Settings, updates, licence). A badge that repeats the page name on every
   row goes (the kind badge on Global Prompts).

## Consequences

- At 1180 × 760 the endpoints page fits about five endpoint rows instead of
  one and a half; Global Prompts fits about nine instead of two; Sessions
  fits about seven conversations instead of three; Settings shrinks from
  2063px to 1627px of scroll height.
- Pages that are being redesigned separately (home, local routing) pick up
  the primitive changes without edits.
- `ServiceCard` no longer has a card variant or a visible "used by" box; the
  tools using an endpoint are announced to screen readers only. The unused
  `ds.service.currentlyUsedBy` / `ds.service.notUsed` strings and the three
  removed descriptions stay in the locale files until the next locale
  cleanup.
- Radius, shadow and colour tokens are unchanged (spec §48, §49, §46).
