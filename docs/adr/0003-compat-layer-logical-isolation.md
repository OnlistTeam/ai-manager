# ADR-0003: Compatibility Layer Uses Logical Isolation; Upstream Code Stays in Place

- Status: Accepted
- Date: 2026-08-18

## Context

The product spec §4 requires a dedicated directory that isolates upstream code so that the frontend and the new layers never touch CCSwitch internal types. But upstream code physically lives at the top level of `src-tauri/src/` (commands/services/database/proxy/…); physically moving it into `compat/ccswitch/` would cause widespread path conflicts in the cherry-pick sync required by §5, effectively losing the ability to sync with upstream.

## Decision

1. **Upstream code stays where it is** (paths match the upstream repository, minimizing the cherry-pick conflict surface).
2. **Create `src-tauri/src/compat/ccswitch/` as a facade layer**: it contains only "calls into upstream services + conversion between upstream models and the Domain Model". This satisfies the isolation goal of §4.
3. **Boundary rules (CI-checkable)**:
   - `domain/`, `application/`, `adapters/`, and `repositories/` must not `use crate::services::…`, `use crate::database::…`, or other upstream modules; they may only go through `crate::compat::ccswitch`;
   - new frontend code (outside `src/native/`) must not import the upstream `src/lib/api/*`;
   - a script lint (grep rules) is added to CI, landing in Phase 1.
4. "Isolation" of upstream modules means that new code can only enter through the facade, not that the code lives in a particular physical location.

## Alternatives

- **Physically move into compat/ccswitch/**: a one-off git mv of hundreds of files, followed by conflicts on every cherry-pick. Rejected.
- **No facade; new layers call upstream services directly**: upstream internal refactors would break all new code, violating §4. Rejected.

## Consequences

- Positive: the cherry-pick path is preserved; the isolation goal is met; the facade layer is thin and testable.
- Negative: upstream modules and the new six-layer directories coexist at the top level of `src-tauri/src/`, so discipline depends on CI lint and code review; documentation must state clearly "which directories belong to upstream" (see ARCHITECTURE.md §3).
