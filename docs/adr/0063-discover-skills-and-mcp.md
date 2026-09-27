# ADR-0063: Discover Skills and MCP servers from their pages

- Status: accepted
- Date: 2026-09-27
- Related: ADR-0022 (Skill mirror fallback), ADR-0047 (MCP install draft),
  ADR-0048 (one list for Skills and MCP), ADR-0056 (outbound proxy modes)

## Context

The Skills and MCP pages manage what is already there. Adding something new
meant knowing where to look: a server's README for the JSON to paste, or a
GitHub repository to add as a Skill source before its catalog listed it. The
people this product is for do not know which servers exist or which Skills
others find useful, and "paste configuration" is a step they abandon.

Two public sources answer "what is there": the official MCP Registry
(`registry.modelcontextprotocol.io`), which lists servers with how to run
them, and skills.sh, which lists Skills by how often they are installed. The
Registry is large and uneven, so a short list of servers the product has
checked by hand is the better first screen.

## Decision

1. **A Discover section at the bottom of each page.** Below the list and its
   notes: a heading, one line on where the cards come from (or the result
   count while searching), a search box, and a grid of compact cards. It asks
   its sources only once it scrolls near the viewport, so the list above
   never waits for it. Global Prompts get no section.

2. **Sources.**
   - MCP, first screen: a featured list kept in the product
     (`application/discover/catalog.rs`), each entry checked by hand: its
     package exists under that name or its endpoint answers. Its one-line
     descriptions are product copy in the locales
     (`discover.mcp.featured.<id>`).
   - MCP, search: featured entries that match, then
     `GET /v0/servers?search=<q>&limit=100&version=latest`. Entries marked
     other than `active` are skipped. An entry becomes a connection by its
     remote first (streamable HTTP, then SSE, HTTPS only), else by its package
     (npm through `npx -y`, PyPI through `uvx`, an OCI image through
     `docker run -i --rm`). Required or secret environment variables and
     headers become inputs (a header keeps its value template, such as
     `Bearer {}`), and so do required positional arguments; an entry that
     needs a named argument the dialog cannot fill is left out. Results are
     ranked (exact name, then name, title and description matches, an icon
     and a repository count for a little, republishing wrappers sink), cut to
     30, and deduplicated by what they run. Registry descriptions are shown as
     returned.
   - Skills, first screen: skills.sh's most installed, read from the
     `initialSkills` data its front page carries (requests say they come from
     a browser; skills.sh answers unknown clients with a page of its own), at
     most four per repository so the first screen is not one author's.
   - Skills, search (two characters or more): local matches of that list,
     then `GET /api/search?q=<q>&limit=40`; at most 60 cards.
   - Skill descriptions: each card's skills.sh page `<meta name="description">`,
     fetched for the cards on screen, six at a time.

3. **What is cached where.** In memory for the session: registry and skills.sh
   searches (10 minutes), the popular list (6 hours; a failed read is retried
   after 5 minutes), the registry servers shown (so one can be added by its id
   and its icon fetched), and where a Skill was found in its repository.
   On disk under `<product data>/cache/discover/`: the last popular list
   (`skills.json`), the Skill descriptions (`descriptions.json`, kept), and
   pictures (`icons/<sha256 of the URL>.txt`, a data URL). When skills.sh
   cannot be reached the list falls back to the disk copy, then to a snapshot
   shipped with the app. A failed source shows one quiet line in the section;
   featured or cached cards stay.

4. **What leaves the machine.** The search words go to the MCP Registry or
   skills.sh; Skill page and picture requests go to skills.sh and the picture's
   host. No key, cookie, account or identifier is sent. Every request uses the
   product's outbound client, so the proxy setting (follow the system, direct,
   or a custom address) applies, with a 20 second timeout.

5. **"Added" is decided natively.** A server is already here when a connection
   the product manages, or one found in any app's own MCP file, runs the same
   thing: the same URL (case and trailing slash ignored), or the same launcher
   (path and `.cmd`/`.exe` ignored) starting the same package (version
   ignored). A Skill is already here when a managed Skill, or a found one whose
   source the skills CLI recorded in `~/.agents/.skill-lock.json`, comes from
   the same GitHub repository and its folder or name is the skills.sh id. A
   found Skill with no recorded source is never claimed. The card then says
   Added and names what it is already called. The list queries sit under the
   extensions key, so a finished Skill or MCP task refreshes them with the
   list above.

6. **Adding reuses the install paths.** The renderer sends only an id, the
   values typed, and the apps chosen; it never sends a command or a URL.
   - MCP: native builds the guided install draft of ADR-0047 from the entry
     (env values as environment variables, header values placed into their
     template, arguments appended, a leading `~/` expanded) and runs
     `McpInstallationService` for the first app. The same task then switches
     the new row on for every other app through the list's own set-enabled
     write. The values follow ADR-0047: no `Debug`, no `Serialize`, and errors
     name the rule, never the value. A featured server's translated one-liner
     is kept as the connection's description; a registry server keeps its own.
   - Skills: native downloads the repository the way a catalog refresh does
     (GitHub first, the verified jsDelivr snapshot after a transport failure,
     ADR-0022), finds the Skill whose SKILL.md is named the skills.sh id or,
     failing that, whose folder is, and installs that catalog item through
     `SkillInstallationService` for the first tool; the same task switches it
     on for the rest.
   - The dialog preselects every app on the page that can take the item. For
     MCP that is decided by transport, from data (`compat/ccswitch/discover.rs`):
     Claude Code, Gemini CLI and OpenCode take local, HTTP and SSE servers;
     Codex, Grok Build and Hermes take local and HTTP; Claude Desktop's file
     takes local commands only (remote servers go through its own Connectors).
     Apps that cannot run the server are named once under the switches.
   - A card adds directly when nothing is required; otherwise, or on a click
     anywhere else on the card, the dialog opens.

7. **Pictures.** The window's policy allows only same-origin and `data:`
   images, so a native command returns a picture as a data URL. It fetches only
   addresses the section handed out: a featured server's logo, a GitHub owner
   avatar in the exact `https://github.com/<owner>.png?size=96` form, or a
   registry icon seen this session. Anything else is refused before a request
   is made. At most 1 MiB, typed by its bytes (PNG, JPEG, GIF, WebP, ICO or
   SVG), never by the server's header. Without one, the card shows the name's
   initial on a tile.

8. **Links.** Homepage, skills.sh page and GitHub repository open through a
   native command that takes the item's kind and id and builds or looks up the
   address itself, as the About panel does.

## Consequences

- A server or Skill can be found and added in two clicks, in every app that
  can use it, without editing a file or pasting JSON.
- Seven new commands (`app_discover_*`) and a product-owned cache directory.
  No new dependency and no schema change.
- The featured list is maintained by hand. An entry whose package is renamed
  or whose endpoint moves has to be edited here; there is no automatic check.
- skills.sh's front-page data is not a published API. If its shape changes the
  section falls back to the disk copy and then the shipped snapshot, and says
  once that skills.sh could not be reached.
- Adding a Skill downloads its repository twice: once to find the folder (the
  answer is kept for the session), once in the install task itself, which owns
  its own download and verification.
- One inherited visibility change: `services/skill.rs` exposes
  `parse_agents_lock` and the owner and repository of `LockRepoInfo` to the
  crate, so found Skills can be matched by source. Recorded in
  `docs/development/UPSTREAM_SYNC.md`.

## Alternatives rejected

- **Load remote pictures directly.** Needs a wider image policy for the whole
  window, and turns every card into a request the product does not control.
- **Send the connection spec to the renderer and install from there.** The
  command and headers would cross IPC and the renderer would be trusted with
  them; an id resolved natively keeps the install path as narrow as the
  guided form's.
- **Add Skills through a configured source.** Would first add a whole
  repository as a source, then pick from its catalog: two steps for one Skill,
  and a source list that grows with every experiment.
