# ADR-0002: Own app.db — Reuse the Upstream Schema, Change the Database Path

- Status: Accepted
- Date: 2026-08-18

## Context

The product spec §16 requires this product to own its database, `app.db` (in the Tauri AppData path), and forbids treating `~/.cc-switch/cc-switch.db` as our own database; §17 requires coexistence with CC Switch and that imports never modify the upstream database. The inventory (CODE_INVENTORY.md), however, shows that the `services/provider/`, `services/skill.rs`, and `database/dao/*` code we want to reuse is all bound to the upstream `Database` singleton, which is hard-coded to `~/.cc-switch/cc-switch.db`.

## Decision

1. **Main database = the full upstream schema (all 17 migration versions reused as-is) + our own path**: `Database` initialization now points to `app_data_dir()/app.db` (Tauri Path API); schema, migrations, and DAOs are untouched.
2. **Importing from CC Switch = a second, read-only connection**: when `~/.cc-switch/cc-switch.db` is detected, open it read-only, read → convert → write into our own app.db, and never write to the upstream database.
3. Database path resolution is centralized in one place (the infrastructure layer); business code must not hand-write home paths.

## Alternatives

- **Design a brand-new independent schema**: fragments the DAO/Service reuse surface and explodes MVP effort. Rejected.
- **Keep using ~/.cc-switch/cc-switch.db directly**: violates §16; the two products would corrupt each other's data. Rejected.

## Consequences

- Positive: a one-line path change yields full data-layer reuse; upstream schema migrations can be followed via cherry-pick; import is simple (same-schema copy plus filtering).
- Negative: schema evolution is coupled to upstream; our own new tables need a separate migration number range or prefix to avoid conflicts with future upstream migrations (decided at implementation time and recorded in the migration file header comment).
