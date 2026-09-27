# ADR-0051: Compact home with each tool's connection

- Status: accepted
- Date: 2026-09-26
- Revises: design-spec §26–§29 (home page). Relates to ADR-0035 and ADR-0039.
- Amended by: ADR-0053 (the row's picker replaces the switch menu, row
  target and Update button; Update All and the live routing slot leave Home).

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
3. **Cheap evidence only.** The row reads the saved inventory and the saved
   entry's edit profile. It does not request the runtime context, whose login
   shell probe costs 0.3–2 s per tool (ADR-0035). So the row states what the
   saved inventory says: the selected entry's name; "Official sign-in" when
   the official entry is selected or nothing is selected and the tool has its
   own login (`discovery.access`); "Not connected" when a tool that needs an
   endpoint has none; the count of added endpoints for tools that choose the
   model themselves (additive entries have no single selection, ADR-0039).
   A variable exported in a shell profile can still override this; the API
   Endpoints page carries that precise answer, and the row opens it.
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
- Positive: Home stays fast; opening it starts no login shell.
- Negative: the row can differ from what the tool really uses when a shell
  variable or a hand edit overrides the saved selection. The endpoints page
  shows the difference; Home does not.
- Negative: the turntable sprite sheet and the turntable support in the shared
  spatial scene are no longer shown anywhere in the product. They are left in
  place for now and can be removed together with the shared UI in a separate
  change.
