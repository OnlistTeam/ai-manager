# ADR-0054: Home chooses the model and the thinking effort

- Status: accepted
- Date: 2026-09-27
- Amends: ADR-0053 decision 1 (what a Home row chooses), ADR-0041
  decision 1 (when a model catalogue is read), design-spec §29.

## Context

After ADR-0053 a Home row chose only the endpoint. The two other things a
user changes most often, which model the tool runs and how hard it thinks,
had no place in this product: the model sat in the endpoint's edit form, two
dialogs deep, and the thinking effort existed only in each tool's own files
and commands (`/effort` in Claude Code, `model_reasoning_effort` in Codex's
`config.toml`). A non-technical user who wants "the stronger model, thinking
harder" had to learn a file format per tool.

These three choices are made together, and often: a cheaper model for a small
task, a stronger one with more effort for a hard one. They belong on one
screen, one row per tool.

## Decision

1. **A row is the tool, its endpoint and model, and its effort.** The first
   picker names the endpoint in use and its model; the second names the
   effort. Every picker in a column has one width, so both columns line up
   down the list. A tool without an effort setting keeps an empty slot of the
   same width, so its model picker stays in the column.
2. **The model picker is grouped by endpoint.** Each of the tool's saved
   endpoints is a row, the one in use first, then the rest in the user's
   order, with its models indented under it. An address set outside this app
   keeps its place at the top, checked and not choosable, with its note
   (ADR-0053 decision 8). Under the endpoint in use the first entry is
   "Tool default", which removes this product's model key so the tool decides
   again. A model typed into the filter field that is in no list can be used
   as typed. Picking a model under the endpoint in use sets it; picking one
   under another endpoint runs the shared switch flow to that endpoint first
   and sets the model once the switch has succeeded, so a target that does not
   answer still changes nothing. Picking an endpoint row switches without
   touching its model. "Manage API endpoints…" stays the last entry.
3. **Where the models come from.** An official entry, and the tool's own
   sign-in when nothing is selected, list a short built-in set per tool; the
   set lives in one domain table (`domain/model_choice.rs`) next to the effort
   levels and reaches the renderer with the current choice, so no screen holds
   a model name. A saved custom endpoint lists what its own model catalogue
   returns (ADR-0041), and an address set outside this app lists what the
   effective catalogue returns. Each endpoint's saved model, which is the model
   last used with it, comes first under it. Catalogues are read only while the
   picker is open, cached with TanStack Query for five minutes, and never
   retried; a catalogue that cannot be read leaves the saved model and the
   free-text entry.
4. **Catalogues are read when the picker opens.** ADR-0041 allowed the
   credential-bearing catalogue request only from an explicit click on one
   named service. Opening a tool's model picker is an explicit click that
   names the tool, and the request it makes is the same free `GET …/models`,
   sent with each saved endpoint's own key to that endpoint's own address: the
   address the tool itself sends the key to. No model is ever called, nothing
   is generated, and the log boundary of ADR-0041 decision 3 is unchanged.
5. **Only the tool's own keys are written.**

   | Tool        | Model (belongs to the endpoint)          | Effort (belongs to the tool)                   |
   | ----------- | ---------------------------------------- | ---------------------------------------------- |
   | Claude Code | `env.ANTHROPIC_MODEL` in `settings.json` | `effortLevel` in `settings.json`               |
   | Codex       | `model` in `config.toml`                 | `model_reasoning_effort` in `config.toml`      |
   | Gemini CLI  | `GEMINI_MODEL` in `~/.gemini/.env`       | none; the effort slot stays empty              |

   The model slot is the one the endpoint edit form and the model test's
   "use this model" already write, so an endpoint has one model wherever it is
   read. It is written to the saved endpoint and, when that endpoint is in
   use, to the one key in the live file. The effort is written to the live
   file and to the in-use endpoint's saved copy, so an edit of that endpoint
   on the API Endpoints page (which rewrites the file from the saved copy)
   keeps it. Every write reads, validates, backs the file up under the product
   data directory, changes the one key, writes a temporary file, validates it,
   replaces the original atomically, reads it back to verify, and restores the
   backup if the read-back disagrees. JSON keeps its key order, TOML its
   comments and layout (`toml_edit`), `.env` every other line as it was.
6. **Precedence against a later switch.** A model belongs to the endpoint it
   was picked under, because model names are endpoint-specific: a relay's
   `glm-5` means nothing to the official service. A switch therefore writes the
   target's own saved model, and switching back restores the model picked for
   the first endpoint; a pick is never carried to another endpoint, and the
   switch's settings preservation already treats these keys as connection keys.
   The effort belongs to the tool and survives every switch: after a switch the
   effort in force before it is put back, and an effort the user reset to the
   tool default stays unset even when the target's saved copy still carries an
   older one. A copy of the file that upstream backfills into an endpoint on
   switching away can hold an older effort; the file in force, not that copy,
   decides.
7. **Capabilities decide what is shown.** `ToolCapabilities` gains
   `canChooseModel` and `canChooseEffort`, both derived from the domain table,
   so the row never checks a tool's name. Tools outside the table (OpenCode,
   which picks the model inside the tool per ADR-0039, and Grok Build,
   OpenClaw, Hermes and Pi) keep the endpoint-only picker.

## Consequences

- Positive: one row states and changes all three choices, and a switch never
  silently discards a model or an effort the user picked.
- Positive: no new dependency and no schema change. Three commands are added:
  `app_tool_model_choice` (read), `app_tool_model_set` and
  `app_tool_effort_set`.
- Negative: Claude Code now saves an effort per model under `modelSettings`
  when `/effort` is used, and that saved level outranks the top-level
  `effortLevel` this row writes. In the user settings file Claude Code also
  ignores the top-level key for Opus 5.5 and later models that have no saved
  level of their own, and `CLAUDE_CODE_EFFORT_LEVEL` outranks both. The row
  shows and writes the documented top-level key; the effort menu lists the
  models that carry their own saved level, so the row never implies it
  governs them. Writing per-model levels was rejected because the model a
  "Tool default" session runs depends on the account and cannot be read from
  the file.
- Negative: an in-tool `/model` choice is saved by Claude Code as `model`,
  which an endpoint's `ANTHROPIC_MODEL` outranks. Choosing "Tool default" on
  Home removes the endpoint's model and hands the choice back to the tool.
- Negative: Codex's effort levels depend on the model. The list offers
  `low`, `medium`, `high` and `xhigh`, which every model in Codex's own
  catalogue accepts; a level already in the file that is not listed (such as
  `minimal` or `max`) is shown as the current value and kept until changed.
- Negative: while live routing has taken over a tool, its live file holds the
  routing placeholders; a model or effort written then is in the saved
  endpoint, but turning routing off restores the file from before the
  takeover.
- The built-in official lists are a short starting set, not a catalogue. They
  are reviewed with each tool's release notes; any other model name can be
  typed.
