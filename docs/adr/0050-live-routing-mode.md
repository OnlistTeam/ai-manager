# ADR-0050: Live routing mode and a memory-only request trace

- Status: accepted
- Date: 2026-09-26
- Amends: ADR-0007 decision 4 (what crosses the product IPC) and its
  consequence that request counters live in a collapsed statistics disclosure.

## Context

ADR-0007 exposes the inherited local gateway one tool at a time: a takeover
switch per tool, an optional failover queue, and aggregate counters. Users who
want every tool to go through AI Manager have to find and flip four switches,
and afterwards they cannot see what happened to a request: which tool sent it,
which service answered, whether the gateway moved it to a backup service, how
long it took. The counters say that something failed over, never what.

The proxy forwarder already makes each of those decisions in one place: it
passes over a paused service, starts a try, gets an answer, or gives up with
an error. Recording at those points costs a few lines in the inherited file.

## Decision

1. **One switch.** `app_routing_set_live_mode(enabled)` turns live routing on
   or off. On takes over every routing target that has a current service,
   through the same takeover path, backups and product lock as the per-tool
   switch (ADR-0007 decision 2 and 3). A tool that cannot be taken over is
   reported in `failures` with its own product error
   (`error.routing.liveTakeoverFailed`); the other tools stay taken over. If no
   tool has a current service the command fails with
   `error.routing.noLiveTargets`; if every attempt fails, the route it may have
   started is stopped again. Off is the existing stop-all-and-restore path.
   The renderer confirms both directions, naming the tools on the way in and
   reusing the stop-and-restore copy on the way out.
2. **No stored flag.** Live routing is on when the local route is running and
   at least one tool is taken over. The per-tool switches stay as the detailed
   controls and change the same state.
3. **The trace.** The proxy keeps the last 60 requests it handled for the four
   routing targets, in memory only, plus running totals (requests, rerouted,
   failed) since AI Manager started. Nothing is written to disk or logs; a
   restart starts empty. Per request it records exactly:
   - a sequence number, a revision, and the start time;
   - the tool (`ToolId`) and the model as the tool asked for it (from the
     body's `model`, or the `models/<name>` path segment), at most 128
     characters;
   - the services tried, in order, each as provider id, provider name,
     outcome (`pending`, `ok`, `failed`, `skipped`), HTTP status when the
     service answered with an error, a short error category, and
     milliseconds. A try is pushed as `pending` the moment it starts and is
     replaced by its outcome when it ends, so the renderer can show a request
     on its way to a service before that service answers;
   - the final status (`pending`, `ok`, `failed`), its error category, the
     total milliseconds (a streamed answer is timed to its end), and whether
     more than one service was tried.

   Error categories are `rateLimited`, `authFailed`, `serverError`,
   `timeout`, `network`, `rejected`, `unavailable`, `cancelled` and `other`.
   Request and response content, headers, keys, URLs, upstream error bodies
   and token counts are not recorded. Token counts are known only after the
   response is parsed for usage, outside the forwarder, and are left out
   rather than correlated later.

4. **What crosses the IPC** (amending ADR-0007 decision 4). In addition to
   the ADR-0007 projection, the product IPC now carries the trace fields in
   decision 3 and nothing else: a read command `app_routing_trace` returning
   the ring newest first, and a `routing://trace` event carrying one changed
   entry with the totals. Both are validated with zod in `src/native/`; the
   renderer keeps whichever copy of an entry has the higher revision.
5. **Where the code lives.** The forwarder (`proxy/forwarder.rs`) gains three
   hooks and a field on its active-connection guard so the request closes when
   the response is released. The inherited types are reduced to trace entries
   in `compat/ccswitch/routing/trace.rs`; the ring and the event sink are
   `infrastructure/routing_trace.rs`; the wire types are
   `domain/routing_trace.rs`. The live-mode orchestration is
   `compat/ccswitch/routing/live_mode.rs`.
6. **What the renderer shows.** A switch with one line of explanation, and
   while live routing is on a panel with a live stage: the routed tools on
   the left, AI Manager in the middle, and on the right the services of the
   tool that sent the latest request, in its routing order (the failover
   queue, else the current service), each marked after a try. Every trace
   change plays as a dot along the wires: tool → AI Manager → the service
   tried; a failed try returns to AI Manager and goes on to the next service;
   the answer (or the error the tool gets) travels back to the tool. One
   plain sentence under the stage narrates the latest step, next to the three
   totals, and the recent requests stay listed one line each below. Motion
   is skipped under `prefers-reduced-motion` and while the window is hidden;
   the stage then only changes state. Email addresses in displayed service
   names are masked (`a***@example.com`) by default; a header toggle, kept as
   a view preference in `localStorage`, shows them in full. The Routing page
   drops the collapsed statistics disclosure, and while tools are routed its
   stop button, because the panel and the switch cover them.

## Consequences

- Positive: one decision turns routing on for every ready tool, and the user
  can see where each request went without reading logs.
- Positive: the IPC addition is a bounded, content-free projection; tests
  assert that no key, URL, error body or note reaches the serialized trace.
- Negative: three hooks in an inherited file must be preserved when the
  forwarder is synced from upstream; the sync ledger records them.
- Negative: the totals count requests since AI Manager started, not since the
  route last started, and the aggregate proxy counters are no longer shown.
- Negative: a request whose entry leaves the 60-entry ring while still
  streaming is not counted in the rerouted or failed totals.
