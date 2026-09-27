# ADR-0061: Signing in to a subscription from AI Manager

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0060 decision 1, ADR-0057 decision 3.

## Context

ADR-0060 made the subscription card say whether the tool is signed in, and
left signing in to the tool: open a terminal, run `codex login` or `claude`
then `/login`, come back. For the users this product is for, that is the
step that fails. They asked for the sign-in to start from the card.

Each vendor's own sign-in is an OAuth round the tool runs itself, and it can
be run the same way from here:

- **Claude Code** (`/login`): authorization code with PKCE on
  `claude.com/cai/oauth/authorize`, Claude Code's client, a callback on a
  loopback port, and the code exchanged at `platform.claude.com`. The tokens
  go to the macOS Keychain item `Claude Code-credentials` (elsewhere
  `~/.claude/.credentials.json`), and the account's identity to
  `oauthAccount` in `~/.claude.json`.
- **Codex** (`codex login`): the same shape on `auth.openai.com` with Codex's
  client, whose callback is fixed at `localhost:1455`. The tokens are
  `~/.codex/auth.json`. Upstream already keeps ChatGPT accounts for Codex
  (`CodexOAuthManager`, several per user, each refreshed in one place) and
  lets an official Codex record be bound to one of them; switching to that
  record writes its account into `auth.json`. It signs in by device code,
  which needs the user to type a code and, on many accounts, to turn device
  codes on first.
- **Gemini CLI**: Google's authorization code with PKCE and Gemini CLI's
  installed-app client. The tokens are `~/.gemini/oauth_creds.json`, the
  account `~/.gemini/google_accounts.json`. Switching to the Google official
  record already sets `security.auth.selectedType` to `oauth-personal`.
- **Grok Build**: `grok login --device-auth` prints a link and a code and
  finishes when the browser does. Its credential format is its own.

magpie runs all four from its window and remembers several Claude and
ChatGPT accounts, switching between them by moving credentials in and out of
the tool's own store. It also spends saved accounts through its gateway on
other tools' behalf; that part is not taken here.

## Decision

1. **The subscription card signs in.** Its dialog has one action, Sign in.
   AI Manager opens the vendor's page in the browser and waits; the dialog
   shows the link to open again or copy and, for Grok, the code. Canceling or
   closing the dialog ends the wait and frees the port. A sign-in expires
   after ten minutes. Nothing about the round (verifier, state, code, tokens)
   reaches the renderer or the log; the dialog polls
   `app_provider_sign_in_status` for `{ phase, url, code, account, failure }`,
   and the URL is built and opened by the backend, never taken from the page.
   The same dialog opens from the list: an official endpoint whose tool is
   signed out says so and carries a Sign in button, rather than sending the
   user to Add endpoint. Editing an official endpoint asks for no key; a key
   typed there would quietly replace the sign-in, and the vendor's API key
   is its own endpoint on the add page. An official record that already
   holds a key keeps the field.

2. **Each account is an endpoint.** Where the product can keep an account,
   signing in adds an official record bound to it, named by the account, and
   switching to that record is switching account:
   - Codex: the code is exchanged with Codex's client, and the tokens are
     handed to upstream's `CodexOAuthManager` (one added `pub(crate)` entry
     point). The record is bound through upstream's own `authBinding`, so
     switching, switching away and refreshing are upstream's.
   - Claude Code: accounts are kept in `claude-accounts.json` in the product
     data directory, readable by the user alone, the way upstream keeps
     Codex's. A record is bound with `authBinding` naming `claude_oauth`,
     which upstream ignores. Switching to it writes the account into the
     Keychain item (or credentials file) and `oauthAccount`. Before that, the
     account Claude Code is signed in to is saved over its copy (its refresh
     token rotates), and one the product did not know yet is saved with an
     endpoint of its own, so a switch never loses a sign-in.
   - Signing in again to an account that has a record updates it and adds
     nothing.
   The unbound official record stays what it was: whatever the tool is
   signed in to.

3. **Gemini CLI and Grok Build sign the tool in.** They keep one account,
   the tool's own: Gemini's tokens are written as Gemini CLI writes them, and
   Grok's login runs Grok's own CLI. The official record is put back if it
   was removed.

4. **After a sign-in** the dialog closes and the same toast as any added
   endpoint offers to use it now. Signing in never switches by itself.

5. **Keychain access** is one more allowlisted program, `/usr/bin/security`,
   fixed path, macOS only, with the credential argument marked sensitive.

6. **The entry for the tool's own sign-in is listed only while it stands
   for something.** The official record that follows whatever the tool is
   signed in to comes from CC Switch's seed. With accounts as endpoints it
   adds nothing while the tool says it is signed out: using it would point
   the tool at no account. So the endpoint list leaves it out then, unless it
   is the endpoint in use, holds a key of its own, or the tool's answer is
   unknown (ADR-0060 shows nothing it cannot read). The record stays in the
   database, since upstream seeds it again anyway; a drag keeps its slot after
   the rows shown. Signing in from the add page brings it back or adds the
   account's own endpoint.

## Consequences

- AI Manager now holds refresh tokens for Claude accounts it signed in, in a
  file only the user can read, as upstream already did for ChatGPT. Removing
  a Claude account's record forgets the account.
- Claude Code's OAuth client is used by a program that is not Claude Code;
  the tokens only ever go to Claude Code, which uses them as its own `/login`
  would. A vendor that closes its client to other redirect ports or changes
  its token shape breaks the sign-in, not the tool: the old way (sign in
  inside the tool) keeps working and the status line of ADR-0060 still reads
  it.
- The tray menu still lists the entry for the tool's own sign-in: reading
  the sign-in runs the tool's status command, too slow for every menu
  rebuild.
- The gateway using saved accounts on other tools' behalf, several Gemini or
  Grok accounts, and quota readouts stay out of scope.
