# ADR-0014: Integrate the OpenClaw Workspace With Fixed Identifiers and On-Demand Reads

- Status: Accepted
- Date: 2026-08-26

> 2026-08-30: ADR-0031 supersedes this document's Advanced Mode visibility gate; the fixed-identifier, on-demand-read, and safe-write boundaries are unchanged.

## Context

The inherited code already knows OpenClaw's nine core Markdown files and the `memory/YYYY-MM-DD.md`
journal convention, but the old frontend called Tauri directly, passed file paths, and put full file
bodies and directory operations in one wide interface. It has not entered AI Manager's product command
allowlist and does not satisfy the "renderer never touches paths" rule or the external-configuration
safe-write contract.

The OpenClaw workspace is still a high-value local capability. It belongs in Advanced Mode, but
reusing the old UI must not re-open arbitrary paths, arbitrary files, or a raw configuration panel.

## Decision

1. Add an Advanced Mode-only `workspace` route and re-validate Advanced Mode in the Application layer;
   hidden navigation is not a permission boundary, and turning Advanced Mode off or restoring the old
   route falls back to Settings.
2. The renderer may submit only the nine fixed file IDs, a validated calendar date, and two fixed
   directory enums. Native re-resolves the OpenClaw root directory on every call; absolute paths,
   recovery-copy paths, and file-picker results never cross the IPC.
3. The first screen reads only bounded metadata. Core file bodies and single-day journal bodies are
   loaded only after the user explicitly selects them; search terms are capped at 200 characters,
   single files at 1 MiB, and enumeration, result count, and cumulative scanned bytes all have limits.
   When a limit is hit, `limited` is returned instead of pretending the result is complete.
4. Reads and writes accept only UTF-8 regular files. Symbolic links, directories in place of files,
   and other irregular entries all fail closed; a missing fixed file may be safely created, but
   arbitrary paths and arbitrary file names never enter the protocol.
5. Saves follow `Validate → Snapshot → Recovery Copy → Private Atomic Write → Verify → Rollback`;
   deletion likewise makes a private recovery copy first and verifies the file is gone, rolling back
   on failure. Recovery copies live in a product-specific hidden directory inside the workspace and
   reuse the retention-count setting.
6. React Query uses a session-scoped infinite cache; unmounting and remounting the route does not
   rescan; after a save the open body cache is updated directly and only the overview and list are
   invalidated. A rescan can only be triggered by first entry, a related write operation, or an
   explicit user refresh.
7. The new product API enters through `src/native`'s strict Zod protocol into eight narrow commands,
   then `OpenClawWorkspaceControl → WorkspaceStore` reuses the upstream conventions. The old
   `src/lib/api/workspace.ts` and old components are not production entry points and do not count
   toward capability coverage.

## Consequences

- AI Manager can safely browse, search, create, edit, and recoverably delete OpenClaw workspace
  content while keeping paths out of the renderer.
- The OpenClaw workspace counts as a production capability of Advanced Mode; session deletion/batch
  operations, cloud sync, and deep links remain separate gaps.
- This slice adds no dependencies or database structures and does not open Raw Config,
  environment-variable editing, arbitrary directory browsing, or batch file interfaces.
