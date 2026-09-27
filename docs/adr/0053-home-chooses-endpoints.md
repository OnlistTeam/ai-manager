# ADR-0053: Home chooses endpoints, API Endpoints manages them

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0051 (decisions 2, 5 and 6), ADR-0050 decision 6 (where the
  live routing switch is shown), design-spec §26–§29, §33 and §35.

## Context

After ADR-0051 the same job had two or three entry points. Switching the
endpoint a tool uses could be done from a Home row's "Switch to…" menu and
from a "Use" button on every endpoint card. Updating a tool could be done from
a Home row, from "Update All" on Home, and from the Software page. The live
routing switch and its request stage sat on Home and again on the Routing tab.
Each duplicate is one more place a non-technical user has to learn, and one
more place where the two copies can disagree.

The owner's direction: one place per job.

## Decision

1. **Home answers "which tool uses what", and is where it is chosen.** One
   compact row per installed tool that can manage endpoints
   (`canManageProvider`): the tool, and at the trailing edge a picker showing
   the endpoint in use (with its pinned model when it has exactly one). The
   pickers share one width so they line up down the list. Opening one lists
   that tool's saved endpoints, the one in use first and checked, the rest in
   the user's order from the endpoints page, with a filter field once there
   are more than seven. It ends with "Manage API endpoints…", or, when the
   tool has no endpoints yet, says so and offers "Add an endpoint…"; both open
   the tool's API Endpoints tab. Picking an entry runs the same recoverable
   switch as before (`useRecoverableProviderSwitch`): the preflight, the
   reopen toast, "nothing changed" when the target does not answer, and the
   failure line under the row. Picking the entry already in use writes it
   again, which is how a selection overridden by a hand edit is re-applied now
   that the endpoints page has no Use button.
2. **States that cannot be switched are not invented anew.** They come from
   `homeToolConnection.ts`: while the list loads the picker is a placeholder;
   when the list cannot be read it is plain text; tools that pick the model
   themselves (additive, ADR-0039) show the count of added endpoints and the
   list only says the model is chosen inside the tool, with the way to manage
   them. The whole-row "open endpoints" target and its chevron are removed;
   the picker's last item replaces them.
3. **API Endpoints answers "what I have".** Add, edit, check and probe,
   reorder (the order is the failover order) and remove. Cards no longer have
   a Use or "add to tool" button; the card a tool is using carries a small
   read-only "In use" badge and the accent bar. A quiet line under the page
   title says the choice is made on Home and links there. The contextual
   recovery after a failed check ("try the next endpoint") stays on the card
   that failed, because it belongs to that check, and the toast after adding
   an endpoint still offers to use it right away.
4. **Storage stays per tool.** An endpoint is saved under the tool that uses
   it, not in one shared pool. Tools are connected directly: each writes its
   own configuration file in its own protocol (Anthropic, OpenAI Responses,
   Gemini, OpenCode's provider map), with its own base URL shape, key
   variable and model names. The same relay usually needs a different base
   URL and model per tool, so a shared entry would have to carry per-tool
   variants anyway, and the picker would have to hide the entries a tool
   cannot speak. Keeping the list per tool keeps every entry valid for the
   picker it appears in, and needs no schema change.
5. **Updates live on the Software page.** Home shows the number of available
   updates as a link to the Software page and has no update action: the row
   Update button, "Update All", its review dialog and its failure notice are
   removed from Home. The Software page keeps its per-tool update. Findings
   already stated elsewhere on Home (an update, a tool without an endpoint)
   stay out of the findings list.
6. **Live routing leaves Home.** The routing switch, privacy line and live
   request stage are shown on the Local Routing tab of API Endpoints only;
   the slot ADR-0051 reserved on Home is removed.
7. **The status line is one quiet line**: status icon, one sentence, the
   update count, the last check, Recheck and the single next step as small
   buttons, without a card frame.

## Consequences

- Positive: each job has exactly one place; Home fits every installed tool and
  its endpoint on one screen, and switching never needs a page change.
- Positive: the endpoints page is a plain inventory; its cards are shorter
  and carry no action that changes what a tool uses.
- Negative: bulk "Update All" (one review for every tool with an update)
  existed only on Home and is gone; updates are reviewed one tool at a time
  on the Software page. If a bulk update is wanted again it belongs on the
  Software page.
- Negative: for additive tools the card's "add to tool" action is gone.
  Adding an endpoint already writes it into the tool's configuration, so it is
  only missed when the entry was removed from that file by hand; restoring it
  then means adding the endpoint again.
- Negative: a user who adds an endpoint on the endpoints page and dismisses
  the toast has to go to Home to use it.
- `OptionPicker` (Popover plus cmdk, both existing dependencies) is the shared
  primitive for the picker; no dependency, command or schema change.
