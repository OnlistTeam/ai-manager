# ADR-0051: Compact home with each tool's connection

- Status: accepted
- Date: 2026-09-26
- Revises: design-spec §26–§29 (home page). Relates to ADR-0035 and ADR-0039.
- Amended by: ADR-0053 (the row's picker replaces the switch menu, row
  target and Update button; Update All and the live routing slot leave Home;
  the row follows the effective connection).

## Context

Home opened with a large spatial hero (a turning 3D model, a status sentence
and one button) followed by three large quick-action cards: Install AI Tool,
Update All, Connect API Endpoint. Together they filled the first screen while
answering almost nothing. The user's first question on opening the app is
"what is each of my AI tools connected to right now?", and the only way to
answer it was to open API Endpoints and pick each tool in turn.

The owner's direction: show a lot on one screen, but keep it simple — compact
rows instead of big cards and whitespace.

## Decision

1. **Status line instead of the hero.** One row: status icon and one sentence
   (all good / N items need attention / nothing installed), the last-checked
   time, Recheck, and the primary action (the recommended next step, or
   "Start <tool>" when ready). It keeps every state the hero had: checking,
   the connection check running or failing to start, updates available, the
   initial "could not check" error with its retry, and the refresh-failed
   notice above it. The hero, its skeleton, the spatial model on Home and the
   home-only styles are removed; the route loading placeholder for Home is a
   plain heading like every other page.
2. **My tools list.** One row per installed tool whose capabilities allow
   endpoint management (`canManageProvider`), never a tool-name check. Each
   row shows the tool, what it is connected to, the pinned model when the
   selected endpoint has exactly one, a small "Switch to…" menu of the other
   saved endpoints, an Update button when an update is available, and opens
   API Endpoints for that tool. The switch reuses the endpoints page's flow
   (`useRecoverableProviderSwitch`, moved into `features/provider-management`),
   so it runs the same preflight and shows the same reopen hint. A target
   that does not respond changes nothing, and the row says so and links to
   the endpoints page, where the failed check and its recovery are shown.
3. **Saved inventory first, the effective connection in the background.**
   The row answers at once from the saved inventory and the saved entry's
   edit profile: the selected entry's name, the official entry included, so
   the row and its picker name it alike (ADR-0053); "Official sign-in" when
   nothing is selected and the tool has its own login (`discovery.access`);
   "Not connected" when a tool that needs an endpoint has none; the count of
   added endpoints for tools that choose the model themselves (additive
   entries have no single selection, ADR-0039).
   It then reads the tool's effective connection (ADR-0035) through the same
   session-cached query the API Endpoints page uses, which the session warm-up
   already fills, so the login-shell probe (0.3–2 s per tool) still runs once
   for both pages. When that answer differs from the saved selection, the row
   follows it: a saved endpoint the tool really uses is named instead of the
   selected one, and an address none of them has is named by its host and
   where it comes from, with a warning tint (ADR-0053 decision 8). Until a
   fresh answer is in (first load, or the re-read after a switch) the saved
   answer stands, without a spinner, and a failed read never promotes stale
   evidence.
   Revised 2026-09-27: this decision first read the saved inventory only, to
   keep Home free of the probe. Then a user whose Claude Code took
   `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` from variables exported in
   a shell profile saw Home claim "Official sign-in" while the tool used a
   relay. A row that is fast but wrong about the one thing it states is worse
   than a row that corrects itself a second later.
4. **Findings stay, compactly,** and only when there is something with a next
   step. A finding a row already shows — an available update, a tool with no
   endpoint — is left to that row instead of being said twice.
5. **Quick-action cards go.** Install AI Tool and Update All become small
   buttons in the list header; Update All is shown only while some installed
   tool has an update, with its disabled reason spelled out beside it.
   Connect is covered by the rows.
6. **A reserved slot** below the status line is where the live routing switch
   and panel will mount; nothing is built for it here.

## Consequences

- Positive: the first screen answers "what is connected to what" for every
  tool, and switching an endpoint no longer needs a page change.
- Positive: Home stays fast: it renders from the saved inventory, and the
  login-shell probe it waits on is the one the session warm-up already runs
  for the endpoints page.
- Negative: for up to a couple of seconds after launch, or after a switch,
  the row can still show the saved answer while the effective connection is
  being read.
- Negative: the turntable sprite sheet and the turntable support in the shared
  spatial scene are no longer shown anywhere in the product. They are left in
  place for now and can be removed together with the shared UI in a separate
  change.
