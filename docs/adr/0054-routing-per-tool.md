# ADR-0054: Routing is chosen per tool, never forced

- Status: accepted
- Date: 2026-09-27
- Amended: 2026-09-27, decisions 8 and 9 (endpoint changes and every way the app ends)
- Supersedes: ADR-0050 decisions 1 and 2 (one switch over every tool, and
  live routing as a derived on/off state) and the parts of decision 6 that
  describe that switch. The trace (decisions 3 to 5) and the live stage stay.
- Amends: ADR-0007 decisions 2 and 3 (the command list and what enabling
  takeover checks), design-spec §26 and §33.

## Context

ADR-0050 put one switch over the four routing targets. Turning it on took
over every tool that had a current service, including a Claude Code that
signs in with its own subscription. Such a login has no address or key in
Claude Code's settings file; the inherited forwarder has nothing to send its
requests to, so from the moment the switch went on every request failed.

It stayed broken after the switch went off and even after AI Manager quit.
The takeover writes the local address into `~/.claude/settings.json` `env`.
Measured on Claude Code 2.1.280 with a scratch config directory: a running
session reloads that file and picks up every changed value (a new address, a
new token, new model names) on its next request, but a key that disappears
from the file keeps its last value in the running session. Restoring the
subscription login removes `ANTHROPIC_BASE_URL`, so the running session kept
calling the local port, which nothing answered once routing stopped. Only a
restart of Claude Code cleared it.

Three things were wrong: a switch that routed tools the user never chose, a
takeover of a connection that cannot be forwarded, and an exit that says
nothing about sessions that still hold the local address. The inherited
restore also writes the whole backed-up file back, so any change made to the
tool's settings while it was routed (a permission granted in Claude Code, an
MCP server added through AI Manager) was silently undone.

## Decision

1. **Per tool, opt in.** There is no master switch and no
   `app_routing_set_live_mode`. Each routing target has its own switch;
   nothing is routed until the user turns a tool on, and every launch starts
   with every tool direct (switches left on by an earlier run are cleared,
   and a route it left behind is put back).
2. **Only what can be forwarded.** Each target carries `unavailable`:
   `noService` (no endpoint chosen), `ownLogin` (the tool signs in with its
   own account, so there is no address and key to forward to) or
   `incomplete` (the endpoint lacks an address or key). It is computed from
   the current endpoint with the same adapter the forwarder uses: an address
   and a credential, or an account the forwarder signs in for itself. Turning
   such a tool on fails with `error.routing.cannotForward` and writes
   nothing. The renderer shows the reason in place of the switch.
3. **The gateway outlives a tool's switch.** The local gateway starts with
   the first routed tool and keeps running while AI Manager runs, in the
   tray included (closing the window only hides it). Turning the last tool
   off leaves it running, so a session that still holds its address keeps
   working until it restarts. It stops when AI Manager quits, or when the
   user stops everything from the leftover-restore action.
4. **Only the route's own settings go back.** When a tool is routed (and
   after a hot switch or a failover change rewrites its config) the product
   notes which settings the route changed and to what, in the settings key
   `aimgr.routing.owned.<app>`. The note holds paths and the values the
   route wrote (the local address, a placeholder token, model aliases); the
   real values stay in the inherited backup. When the route ends, the
   backup handed to the inherited restore is the current file with only the
   noted settings put back, and only where the file still holds the route's
   value; a setting still naming the local address or placeholder is put
   back regardless. TOML parts are edited in place, keeping comments. The
   Codex login in `auth.json` is restored as backed up, as before. Without a
   note (a route started by an older version) the whole backup is restored.
5. **Honest restart notes.** Each target carries `pickup`: `live` when the
   tool rereads its settings file while running (Claude Code), `atStart`
   when it reads it once (Codex, Gemini CLI, Grok Build). After a switch the
   row says what an open session does:
   - on, `live`: open sessions start going through AI Manager right away;
   - on, `atStart`: open sessions keep their direct connection until they
     restart;
   - off, `live`: open sessions switch back right away but keep any setting
     routing added until they restart;
   - off, `atStart`: open sessions keep going through AI Manager until they
     restart, and it keeps forwarding for them while it runs.
6. **Quitting asks.** A user-initiated quit (tray Quit, closing the window
   with minimize-to-tray off) while any tool is routed is held: the window
   comes forward and asks, naming the routed tools and what their open
   sessions will do (`live` sessions switch back on their own; `atStart`
   sessions stop working until restarted). `app_quit_confirmed` quits. A
   second quit request within a minute of asking goes ahead, so a window
   that cannot answer never blocks quitting. The update restart is not held
   (decision 9).
   On quit every routed tool is put back as in decision 4 and the gateway
   stops.
7. **The view.** The Local Routing tab is one compact list: a row per
   routing target with the tool's logo and name, the endpoint it uses, and
   the switch or the reason it cannot be routed. A routed row shows its
   failover switch and queue under it. The live stage, the one-line
   narration, the totals and the recent requests stay below the list for as
   long as the gateway runs.
8. **An endpoint change never leaves a tool on a route that cannot carry
   it.** Every path that makes an endpoint current (Home, the endpoint list,
   Try Next, the tray) goes through one backend switch. When the tool is
   routed and the gateway could not carry the new endpoint (the reasons of
   decision 2), that tool's route ends first, exactly as turning it off does
   (decision 4), and only then is the new endpoint applied, directly. The
   switch outcome carries `routingEnded` (the tool's `pickup`), and the
   success toast says in one line that routing for the tool is off because
   the endpoint cannot go through AI Manager, followed by the off note of
   decision 5. A switch to another endpoint the gateway can carry keeps the
   route and only moves its upstream. Saving the current endpoint of a
   routed tool so that it can no longer be carried (its key removed, say)
   ends the route the same way, after the save; `app_provider_save` returns
   `{ providers, routingEnded? }` and the edit form says it in a toast. If
   the switch fails after the route ended, the route stays off: the tool is
   back on its previous endpoint directly, which is a safe place to stop.
   The tray switch ends the route too but has nowhere to say so; the
   Local Routing row shows the tool direct.
   The "routed, but the endpoint cannot be routed" warning on a row stays,
   now only for states no switch or save produces: a restored backup or an
   outside change to the endpoint record.
9. **Every way AI Manager ends puts the routed tools back.**
   - The update restart (`app_update_install_and_restart`) already runs the
     same clean-up as quitting before the new version starts (before the
     install on Windows, where the installer ends the process; after it
     elsewhere). It is not held by the quit question: the user already
     chose to restart. While any tool is routed, the ready-to-restart note
     in Settings and a tooltip on the window's restart button add one line,
     "Tools routed through AI Manager go back to direct connections when it
     restarts."
   - On macOS a logout, restart or shutdown, the app menu's Quit (Cmd+Q)
     and the Dock's Quit all end the app through NSApplication
     `terminate:`. Neither tao 0.34 nor Tauri 2.10 implements
     `applicationShouldTerminate:`; the event arrives only as
     `RunEvent::Exit` (from `applicationWillTerminate:`), with no reason and
     no way to hold it, and never as `ExitRequested`. So the quit question
     never blocked a logout, but none of these paths ran the clean-up
     either. The end of the event loop now runs it on the main thread,
     bounded to five seconds; the next launch puts back anything left.
     These quits do not ask, and cannot: telling the system's quit from the
     user's would mean patching the app delegate at run time (reading the
     quit Apple event's reason), which is not worth the risk.
   - A process killed by a signal (`kill`, a crash) still skips the
     clean-up; handling SIGTERM would need tokio's `signal` feature, a new
     dependency path. The next launch puts the tools back (decision 1).
   The 60-second rule of decision 6 stays for the quits that do ask.

## Consequences

- Positive: routing a tool is a choice the user makes for that tool, and a
  tool that cannot be forwarded can no longer be broken by it.
- Positive: edits made while a tool is routed survive the route's end.
- Positive: turning a tool off never cuts off a session that still points at
  the gateway while AI Manager runs.
- Negative: the product repeats the inherited takeover-off bookkeeping
  (restore, delete the backup, clear the switch and health) so that it can
  skip the gateway stop; an upstream change to that path must be mirrored.
- Positive: choosing an endpoint the gateway cannot carry turns routing off
  for that tool and says so, instead of leaving it pointed at the gateway.
- Positive: logging out, shutting down and macOS's own Quit put the routed
  tools back instead of leaving that to the next launch.
- Negative: a quit from the tray or the window is held until answered (or a
  second quit within a minute); macOS's own Quit and Dock Quit are not
  asked about, because they share the system's quit path.
- Negative: a quit from the system blocks the ending process on the main
  thread for up to five seconds while the tools are put back.
- Negative: the tray switch cannot tell the user that routing ended.
