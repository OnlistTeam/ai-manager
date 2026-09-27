# ADR-0053: Home chooses endpoints; API Endpoints chooses and manages them

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0051 (decisions 2, 5 and 6), ADR-0050 decision 6 (where the
  live routing switch is shown), design-spec §26–§29, §33 and §35.

## Context

After ADR-0051 some jobs had two or three entry points. Updating a tool could
be done from a Home row, from "Update All" on Home, and from the Software page.
The live routing switch and its request stage sat on Home and again on the
Routing tab. Each duplicate is one more place a non-technical user has to
learn, and one more place where the two copies can disagree.

Switching the endpoint a tool uses is different. A first version of this
decision removed the endpoint cards' Use button so that only Home switched.
In use that hurt: the endpoints page is where a user adds, checks and compares
endpoints, and having to leave it to use the one just checked was one more
page change for the most common action. Both places now switch, through one
shared flow, so they cannot disagree.

## Decision

1. **Home answers "which tool uses what", and can change it.** One
   compact row per installed tool that can manage endpoints
   (`canManageProvider`): the tool, and at the trailing edge a picker showing
   the endpoint in use (with its pinned model when it has exactly one). The
   pickers share one width so they line up down the list. Opening one lists
   that tool's saved endpoints, the one in use first and checked, the rest in
   the user's order from the endpoints page, with a filter field once there
   are more than seven. It ends with "Manage API endpoints…", or, when the
   tool has no endpoints yet, says so and offers "Add an endpoint…"; both open
   the tool's API Endpoints tab. Picking an entry runs the recoverable switch
   the endpoints page runs (`useRecoverableProviderSwitch`): the preflight,
   the reopen toast, "nothing changed" when the target does not answer, and
   the failure line under the row. Picking the entry already in use writes it
   again, which re-applies a selection a hand edit overrode.
2. **States that cannot be switched are not invented anew.** They come from
   `homeToolConnection.ts`: while the list loads the picker is a placeholder;
   when the list cannot be read it is plain text; tools that pick the model
   themselves (additive, ADR-0039) show the count of added endpoints and the
   list only says the model is chosen inside the tool, with the way to manage
   them. The whole-row "open endpoints" target and its chevron are removed;
   the picker's last item replaces them.
3. **API Endpoints answers "what I have", and can change it too.** Add,
   edit, check and probe, reorder (the order is the failover order) and
   remove, and every card keeps its Use button (for tools that pick the model
   themselves, "Add to tool"), exactly as before this decision. The card a
   tool is using carries the "In use" badge and the accent bar. Both places
   switch through the same hook, so a switch started on either page shows
   the same preflight, reopen hint and recovery, and the other page reflects
   it from the shared query cache. The contextual recovery after a failed
   check ("try the next endpoint") stays on the card that failed, and the
   toast after adding an endpoint still offers to use it right away.
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

- Positive: Home fits every installed tool and its endpoint on one screen,
  and switching never needs a page change; the endpoints page still lets a
  user check an endpoint and use it in place.
- Positive: updates and live routing each have exactly one place.
- Negative: switching has two entry points. They share one hook and one
  query cache, so they behave the same and always agree on what is in use.
- Negative: bulk "Update All" (one review for every tool with an update)
  existed only on Home and is gone; updates are reviewed one tool at a time
  on the Software page. If a bulk update is wanted again it belongs on the
  Software page.
- `OptionPicker` (Popover plus cmdk, both existing dependencies) is the shared
  primitive for the picker; no dependency, command or schema change.
