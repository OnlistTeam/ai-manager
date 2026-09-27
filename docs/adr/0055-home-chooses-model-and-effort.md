# ADR-0055: Home chooses the model and the thinking effort

- Status: accepted
- Date: 2026-09-27
- Amended: 2026-09-27, decision 8 (Claude Code's effort follows Claude
  Code's own resolution); 2026-09-27, decisions 1, 2 and 9 (the row names
  the model, not the endpoint; the effort is a slider without notes)
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

1. **A row is the tool, its model, and its effort.** The first picker names
   the model in use, led by the mark of the endpoint it runs on (the
   endpoint's logo or initials, the tool's own mark for its sign-in), with
   the endpoint's name on its tooltip; "Default" when this product's model key
   is absent. The second names the effort. Every picker in a column has one
   width, so both columns line up down the list. A tool without an effort
   setting keeps an empty slot of the same width, so its model picker stays in
   the column. The endpoint has no picker of its own on these rows: choosing
   an endpoint is choosing where a model comes from, so it happens in the
   model list, and the API Endpoints page keeps its own "Use" button
   (ADR-0053).
2. **The model list is grouped by endpoint.** A filter field sits on top; a
   rail beside the list shows all models, only the starred ones, or one
   endpoint's. Each endpoint is a section headed by its name, the one in use
   first, then the rest in the user's order. An address set outside this app
   heads its section with where it comes from ("terminal variable"), the full
   source and what a choice here does on the tooltip (ADR-0053 decision 8);
   the pill turns amber only when that address outranks a switch, the one
   case where a choice here would not take effect. Each section starts with
   "Default", which removes this product's model key so the tool decides; its
   note, "what the tool ships with", is said once at the top. A model can be
   starred; stars are a view preference kept per tool and endpoint in this
   device's local storage, like the live panel's email masking. A model name
   typed into the filter that is in no list can be used as typed, under the
   endpoint the rail shows or else the one in use. Picking under the endpoint
   in use sets the model; picking under another endpoint, "Default" included,
   runs the shared switch flow to that endpoint first and sets the model once
   the switch has succeeded, so a target that does not answer still changes
   nothing. "Manage API endpoints…" stays the last entry. Tools whose model is
   not chosen here keep the plain endpoint list (decision 7).
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

   | Tool        | Model (belongs to the endpoint)          | Effort (belongs to the tool)                                                                       |
   | ----------- | ---------------------------------------- | -------------------------------------------------------------------------------------------------- |
   | Claude Code | `env.ANTHROPIC_MODEL` in `settings.json` | `effortLevel`, `modelSettings.<model>.effortLevel` and `env.CLAUDE_CODE_EFFORT_LEVEL` (decision 8) |
   | Codex       | `model` in `config.toml`                 | `model_reasoning_effort` in `config.toml`                                                          |
   | Gemini CLI  | `GEMINI_MODEL` in `~/.gemini/.env`       | none; the effort slot stays empty                                                                  |

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
8. **Claude Code's effort is shown and written the way Claude Code uses it.**
   The first version wrote only the top-level `effortLevel`. Claude Code no
   longer resolves effort from that key alone, so the pill could name a level
   a new session would not run at, and a choice made here could change
   nothing. The facts, from Claude Code's model configuration and settings
   reference:

   - A session takes the first of: an explicit choice
     (`CLAUDE_CODE_EFFORT_LEVEL`, from the settings file's `env` block or the
     terminal, `--effort`, or `/effort` in the session); the level saved for
     that model in `modelSettings.<model>.effortLevel`, else the top-level
     `effortLevel`; the model's default (`high`, except `medium` on Opus 5.5).
   - In the user settings file the top-level `effortLevel` is the older form
     `/effort` wrote. It still applies to Opus 5, Fable 5.1, Sonnet 5 and
     earlier, but not to Opus 5.5 or models released after it. In project,
     local and managed settings it applies to every model.
   - `/effort` saves per model under `modelSettings`, keyed by the canonical
     name (`claude-opus-5-5`), and matches aliases, `[1m]` and dated or cloud
     spellings to the same entry. Both settings keys accept `low` to `xhigh`;
     `max` persists only through `CLAUDE_CODE_EFFORT_LEVEL`, which also
     outranks `/effort`.

   So the product now does what `/effort` does, for every model at once:

   - **A level from `low` to `xhigh`** is written to the top-level
     `effortLevel` (for the older models), to the `effortLevel` of every entry
     already in `modelSettings`, and to an entry for each current
     effort-capable model in the domain table (`claude-fable-5-1`,
     `claude-opus-5-5`, `claude-opus-5`, `claude-sonnet-5`). Other fields of an
     entry, such as a `maxEffortLevel` cap, stay. `env.CLAUDE_CODE_EFFORT_LEVEL`
     is removed, so the choice takes effect and a later `/effort` still works;
     it changes only its own model's entry.
   - **Max** is written as `env.CLAUDE_CODE_EFFORT_LEVEL=max`, the one place it
     persists. It then holds for every session and `/effort` cannot change it
     until another level is chosen here, which removes the variable.
   - **Tool default** removes the top-level key, the variable and the
     `effortLevel` of every `modelSettings` entry, dropping an entry left
     empty; each model then runs at its own default.

   The pill resolves the level in the same order: the variable in the
   settings file (an empty value there cancels the terminal's), else the
   terminal's variable, shown with where it comes from and not choosable,
   like an address set outside this app; else the level for the model in use,
   named by `env.ANTHROPIC_MODEL`, the terminal's `ANTHROPIC_MODEL` or the
   `model` key, with aliases mapped through the domain table. A model outside
   the table is taken to read the top-level key, as every model before
   Opus 5.5 does. When no model is named (or it is `default`, `best` or
   `opusplan`), the pill shows one level only if every model in the table and
   in `modelSettings` runs at it, and otherwise says the models differ, with
   each model's level on its tooltip. The
   terminal is read through the same cached login-shell probe as the
   effective connection, and only for the variables the tool reads for these
   two settings. A switch keeps `modelSettings` and the variable from the
   file in force, like the top-level key; a route puts back only the keys it
   wrote (ADR-0054), which these are not.

9. **The effort is a slider.** The pill shows four bars lit up to the level
   and the level's name. It opens a slider from "Default" to the tool's
   strongest level, with the two ends named under it. Each stop the slider
   settles on (300 ms after the last keyboard step, or on release) is written
   at once and the slider stays open, so neighbouring levels can be tried in
   turn; closing it mid-step still writes the stop it was left on. The
   slider carries no notes: the rules of decision 8 decide what is written,
   and the pill already names what a new session runs at. The one exception
   is a level a terminal variable holds, where the slider cannot move and
   one line says which variable, in which file, holds it.

## Consequences

- Positive: one row states and changes all three choices, and a switch never
  silently discards a model or an effort the user picked.
- Positive: no new dependency and no schema change. Three commands are added:
  `app_tool_model_choice` (read), `app_tool_model_set` and
  `app_tool_effort_set`.
- Negative: the pill reads only the user settings file and the terminal.
  An `effortLevel` or `modelSettings` in a project, local or managed settings
  file, a `--effort` flag, a `maxEffortLevel` cap, an organisation default,
  and the fallback of a level a model does not support (`xhigh` runs as
  `high` on Opus 4.6) can each make a session differ from what the pill
  says. The model a session with no named model runs depends on the account,
  which is why the pill then compares every model instead of guessing one.
- Negative: the table of effort-capable models is a short current set, like
  the official model lists. A model released later gets an entry only once
  `/effort` saves one or the table is updated; until then the tool-wide key
  and its own default decide for it.
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
