# ADR-0042: Editing a connection variable a shell profile sets

- Status: accepted
- Date: 2026-09-21
- Supersedes nothing. Extends ADR-0035 (effective connection) and ADR-0041 (model probe).

## Context

The API Endpoints page shows a card for the connection in force when it is not
one of the saved services — typically an address exported from a shell profile.
Until now that card could only open the file in the system editor, and its badge
said "not saved here". Two complaints, both fair:

1. That card was the only connection in the product that could not be tested.
2. "Not saved here" describes where a record lives. What a reader needs to know
   is that this address wins over whatever they pick in the list below it.

There was also a capability gap underneath. On a real machine the variable was
in `~/.config/zsh/secrets.zsh`, pulled in by a `source` line in `.zshrc`. The
upstream scanner (`services/env_checker.rs`) reads a fixed list of seven files
and does not follow `source`, so the product could only report "something in
your terminal sets this" — correct, and useless. `~/.zshrc` did contain a
commented-out `export` of the same variable, and pointing at that line would
have been worse than pointing at nothing.

## Decision

**Follow `source`.** A new module, `compat/ccswitch/provider_runtime/shell_files`,
walks the start-up tree: a wider base list than upstream's (adding `.zshenv`,
`.zlogin` and fish's `config.fish`), then `source` and `.` directives up to
three levels, expanding only `~`, `$HOME` and `$XDG_CONFIG_HOME`. It is not a
shell: no conditionals, no command substitution, no arbitrary parameters. Cycles,
file size, and total files read are all bounded.

The login shell stays the authority on the _value_ (ADR-0035). A file is only
credited with a variable when the value it writes equals the value the shell
actually exported, and when several lines match, the last one read wins — the
way the shell resolves it. Upstream's scanner is kept as a fallback for the few
files this walk does not cover.

**Offer an edit only where a line can be pinned down, and change only that
line.** `shell_edit` re-locates the line, checks it still holds the value the
dialog displayed, and refuses otherwise. It preserves the keyword, the variable
name, the quoting style, the indentation, any trailing comment, the line ending,
and the presence or absence of a final newline. It copies the file into the
backup directory first, writes through a temporary file and a rename, restores
the original permissions, and reads the result back before reporting success.

Four cases are refused rather than guessed at:

- a value built by running a command or interpolating another variable, where
  replacing the text with a literal would change what the line does;
- a line that changed since the dialog read it;
- a file outside the user's home directory (a system profile is located and
  displayed, never modified);
- a value containing a line break or control character, which would split one
  assignment into two statements.

**Test the connection in force.** Two commands mirror the saved-service probe
but take no service id: the backend resolves the address and credential through
the same precedence rules the page displays. This required the effective-
connection resolver to carry credential _values_ internally; `EffectiveConnection`
drops them on the way out, and `Resolution`'s `Debug` renders them redacted.

**Say what the card means.** The badge is now "set outside this app", and the
card carries one sentence: this address is what the tool will use, and it
overrides anything chosen below.

## The reversal this involves, and why it is acceptable

`environment.rs` carried the rule that a variable's value is used "only in
memory inside the compatibility layer; never logged, never in technical_message,
never across IPC". The edit dialog breaks the last clause: it prefills the
current value, so the value crosses IPC.

The alternative is asking someone to retype an address they cannot see, which
is a guess rather than an edit. ADR-0039 already settled that a saved key is
visible and copyable on its own card, so a value the user put in their own
profile being visible in a dialog they opened is consistent rather than new.

The clause that does **not** move is the log. `ShellVariableLocation`,
`ShellVariableUpdate`, `ShellVariableWritten` and `ShellVariableSite` all have
hand-written `Debug` implementations that render values as `<redacted>` and
paths as `<path>`; the service logs the variable name, the line number, and
whether the line was editable, and nothing else. Tests assert the redaction
rather than trusting it.

## Consequences

- The scope is bounded to the variables the tool itself reads
  (`connection_variables`), checked again in the backend on write. This cannot
  become a general "read or write any variable in my shell" capability.
- A variable set somewhere the walk does not model — a conditional, a plugin
  manager, a `source` of a computed path — has no edit button. That is the
  honest state, and it is preferable to rewriting the wrong line.
- Changing a profile does not change a terminal that is already running. The
  dialog says so, because there is nothing the application can do about it.
- Upstream's `services/env_manager.rs` is **not** reused. It filters out every
  line matching the variable name regardless of which line was located, rebuilds
  the file with `lines().join("\n")` — dropping a trailing newline and silently
  converting CRLF to LF — writes non-atomically, and "backs up" a JSON of names
  and values rather than the file, so a bad edit cannot be undone.

## Alternatives rejected

- **Rewriting every matching line.** What upstream does. A profile with a line
  in a branch that never runs, or a deliberately commented history of previous
  values, would be silently rewritten.
- **Making the user save the connection as a service first.** It would make the
  most confusing setup the only one that has to be duplicated before it can be
  looked at, which is the cognitive load the complaint was about.
- **Shelling out to the user's shell to resolve `source`.** Accurate, and it
  executes their profile to answer a question about editing it. The value
  already comes from a login shell probe; this walk only needs a line number.
