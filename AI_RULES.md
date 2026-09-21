# AI_RULES.md — Mandatory Rules for AI Coding

> Any AI (or human) writing code in this repository must first read this file and docs/ARCHITECTURE.md in full.
> When they conflict, docs/product/design-spec.md wins.

## Core Prohibitions (non-negotiable)

1. **Do not bypass architecture.** Data may only flow UI → Hook → NativeClient → Command → Application → Domain → Adapter/Repository.
2. **Do not call Tauri directly outside `src/native/`.** Only `src/native/` may use `invoke()/listen()/emit()`.
3. **Do not access SQLite outside repositories.** Components, hooks, and the Application layer never touch raw SQL.
4. **Do not execute shell commands from React.** Shell runs only on the Rust side and must go through `CommandSpec { program, args[], … }` plus an allowlist; string-concatenated commands are forbidden.
5. **Do not add dependencies without approval.** Before adding an npm/cargo dependency, write down why it is needed, why the existing dependencies are insufficient, and the size, security, and maintenance impact. Never upgrade core dependencies such as React, Tauri, or Tailwind on your own.
6. **Do not expose secrets in logs.** Never log API keys, tokens, or cookies; error messages must not contain a complete key; display keys as `sk-ant-••••••••A12F`; logs are redacted automatically.
7. **Do not rewrite upstream code unless necessary.** Stable upstream (CC Switch) capabilities are called through the `compat/ccswitch/` compatibility layer; do not rewrite them, move them, or change them just to make them "look like ours".
8. **Prefer adapters over conditional logic.** No scattered `if tool === "claude"`; drive UI and logic from `ToolCapabilities`.
9. **Add tests for changed behavior.** Changed behavior needs tests; never delete existing tests.
10. **Keep changes scoped.** One Phase / one Feature per task. When you find old code that violates the rules, fix only the part your task touches and record the rest as Technical Debt; do not refactor the whole module in passing.

## Code Size and Structure

- React component < 250 lines, hook < 200 lines, Rust service < 500 lines; split modules as they keep growing.
- No catch-all `utils.ts` / `helpers.ts` / `common.rs` / `misc.rs`; every helper belongs to a clearly named Domain directory.
- The third time a similar piece of UI appears, extract a component (ToolCard / StatusBadge / EmptyState / ProgressTask / SettingRow …).
- Features import each other only through their public `index.ts`.

## Types and Errors

- No `any` in new TS code; narrow unknown data with `unknown` + Zod / type guards. `@ts-ignore` is not a default solution.
- No `unwrap()/expect()` sprawl on Rust business paths; expected errors return `Result<T, AppError>`; only true invariants may `expect`, with the reason noted.
- Errors use one shape: `AppError { code, message_key, technical_message, remediation, context_id }`; the frontend branches on stable error codes, never on error strings.
- Never show `std::io::Error` / `ENOENT` / `Exit code 127` to the user directly; the user sees plain language plus View Details.
- Native API return shapes must be stable (`Result<T>`); important models are typed on both the Rust and TS side, and data crossing the boundary is validated.

## State and Data

- Rust / SQLite / file / network data always goes through TanStack Query; React state holds only transient UI state such as modal open flags, selected tabs, and inputs.
- No Redux / MobX / Zustand (unless an ADR says otherwise).
- Modifying external tool configuration (`~/.claude` etc.) must follow the eight-step flow: Read → Validate → Backup → Modify → Write Temp → Validate Again → Atomic Replace → Verify, with Rollback on failure. Never overwrite with a bare `fs.write(config)`.
- Tests never touch the user's real `~/.claude` / `~/.codex`; use TempDir + `tests/fixtures/` (valid / invalid / legacy / empty / corrupted).

## Explicitly Forbidden

Copying large blocks of code to save effort · rewriting the whole project without authorization · deleting existing tests · turning off TS strict checks · swallowing errors in try/catch · 1000-line components · duplicate sources of state · changing the database schema without approval · deleting old migrations without approval · exposing raw installer shell output to non-technical users.

## Closing Output for Every Task

```text
Changed Files / Architecture Impact / Tests Added / Tests Passed / Known Limitations / Next Recommended Step
```

## UI Definition of Done

A page is done only when every state exists — Loading / Empty / Success / Warning / Error / Disabled / Hover / Focus / Dark Mode / Chinese / English — interactive controls are keyboard-operable, and state is not conveyed by colour alone.
