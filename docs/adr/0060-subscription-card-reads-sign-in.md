# ADR-0060: The subscription card reads whether the tool is signed in

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0057 decision 3, design-spec §34.
- Amended by: ADR-0061 (the card signs in; decision 1 no longer holds).

## Context

ADR-0057 made the subscription card the entry that leaves a tool on its own
account, and said the card never signs anyone in. It also never looked. The
card carried a check whenever the official entry was in the list, and upstream
seeds that entry on first start, so every user saw the check. Its dialog then
said "it is already in the endpoint list". A user who had never signed in to
Codex read both as "you are signed in" and found out otherwise only inside the
tool.

Both tools answer the question themselves, locally and in well under a second:

- `codex login status` prints `Logged in using ChatGPT`, `Logged in using an
  API key - …` or `Not logged in`, and exits non-zero for the last. It reads
  `auth.json` or the keyring, whichever Codex was told to use.
- `claude auth status --json` prints `loggedIn` and `authMethod`, and for a
  claude.ai sign-in also `email`, `orgName` and `subscriptionType`. Its answer
  follows the settings in force, so while Claude Code points at a relay it
  reports the relay's token as `oauth_token`. The account `/login` saved is
  still named in `~/.claude.json` under `oauthAccount`, which holds the email
  and organisation and no credential.

magpie reads the same two sources, then goes further: it runs its own OAuth,
stores refresh tokens and replays them through its gateway. That is the part
that carries account risk, and it is not what this card needs.

## Decision

1. **The card reads, it still never signs in or holds a credential.**
   `app_provider_tool_login_status` returns `{ state, account, plan }` with
   `state` one of `signedIn`, `apiKey`, `signedOut`, `unknown`. Nothing that
   authenticates crosses the boundary or is logged: the command output is
   parsed in the facade and only these three fields leave it.

2. **Each tool is asked in its own words.** The facade re-probes the install
   (as a launch does) and runs the tool's status command anchored to that
   binary, with the binary's directory first on `PATH` so a Node launcher finds
   its Node.
   - Claude Code: `claude.ai` is signed in, with the email and plan it names.
     `none` is signed out. Any other method means the settings in force hide
     the login, so the answer comes from `oauthAccount` in the Claude config
     file: present is signed in with that email, absent is signed out.
   - Codex: ChatGPT is signed in, an API key is `apiKey`. When Codex reports
     no login while another endpoint is in use, upstream may be holding the
     ChatGPT login it moved out of `auth.json` (ADR-0040) and will write back
     on switching to the official entry; if the official record holds ChatGPT
     tokens the answer is signed in. Only their presence is checked.
   - Gemini CLI and Grok Build have no status command, so they are `unknown`,
     as is a tool that is not installed, runs under WSL, or does not answer
     within ten seconds. `unknown` shows nothing rather than guessing.

3. **Where the state shows.** On the add page the card's trailing mark is the
   state ("Signed in", "Not signed in") instead of the saved check. The dialog
   names the account and plan when signed in and how to sign in otherwise, and
   drops "it is already in the endpoint list". In the endpoint list the
   official card's key line says the same. On Home, an official entry that is
   signed out says "Not signed in" where the model would stand. The state is
   read again when the window regains focus, so signing in at the terminal and
   coming back is enough.

## Consequences

- Several accounts per tool, and a gateway spending a subscription on another
  tool's behalf, stay out of scope. Both need the product to hold refresh
  tokens and are decided separately.
- Codex's ChatGPT tokens still pass through the database when the user
  switches away and back (technical debt #1); this ADR only reads that they
  are there.
- A future Claude Code that renames `authMethod` values or moves
  `oauthAccount` degrades to `unknown` or to the file answer, never to a
  false "signed in" from a relay token.
