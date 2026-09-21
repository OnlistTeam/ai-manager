# AI Manager Architecture

## 1. Purpose and scope

This document is the engineering map of AI Manager: which layers exist, where code belongs, which
boundaries are enforced by CI, and how the main behaviors flow through the system. It is written for
the state of the tree after the internal cleanup (Cargo library `ai_manager_lib`, test home
variable `AI_MANAGER_TEST_HOME`, unregistered inherited command modules removed). It deliberately
contains no change history; decisions and their rationale live in `docs/adr/`, and the current
upstream-sync ledger lives in `docs/development/UPSTREAM_SYNC.md`.

Read in this order before changing code:

1. `AI_RULES.md` — non-negotiable coding rules and the short list of hard boundaries.
2. This document — layers, directories, data flow, contracts.
3. `docs/product/design-spec.md` — the product and engineering specification; it wins on conflict.
4. The ADRs referenced by the area you are touching.

## 2. System overview

AI Manager is a local-first desktop application (Tauri 2, React 18, TanStack Query 5, SQLite) that
lets non-technical users install, update, configure, and repair AI coding CLIs. It is a fork of
CC Switch (MIT), distributed as a whole under AGPL-3.0-or-later. The inherited Rust backend
(configuration parsers, provider engine, MCP/Skills/
Prompts services, database, proxy) stays in place and is reached only through a compatibility layer;
the user interface, the product API, and the safety layers around every write are original.

### 2.1 Six layers

| Layer          | Lives in                                                               | Responsibility                                                              |
| -------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Presentation   | `src/` (React)                                                         | Display, interaction, forms, motion, state feedback. Nothing else.          |
| Application    | `src-tauri/src/application/`                                           | Use cases: orchestration, capability gates, transactions, task registration |
| Domain         | `src-tauri/src/domain/`                                                | Product models, error model, message keys. No Tauri, no OS API.             |
| Infrastructure | `src-tauri/src/infrastructure/`, `src-tauri/src/repositories/`         | Product data directory, logging limits, OperationManager, product tables    |
| Platform       | `src-tauri/src/platform/`                                              | `cfg(target_os)` concentration: commands, executor, terminals, redaction    |
| Compatibility  | `src-tauri/src/compat/ccswitch/` (facade) over the inherited modules | The only code allowed to see CC Switch types, services, and the database    |

Adapters (`src-tauri/src/adapters/`) sit between Application and Compatibility/Platform: they implement
the `ToolAdapter` trait per tool and own the runners that turn plans into processes.

### 2.2 The single valid data flow

```text
React component
  → feature / entity hook            (src/features, src/entities; TanStack Query)
    → native client                  (src/native: invoke + Zod validation + NativeError)
      → Tauri command                (src-tauri/src/commands/app_*.rs; thin, validates, forwards)
        → application service        (src-tauri/src/application)
          → domain model / rules     (src-tauri/src/domain)
            → adapter / repository   (src-tauri/src/adapters, src-tauri/src/repositories)
              → compatibility facade (src-tauri/src/compat/ccswitch)
                → OS process / SQLite / tool configuration files
```

Events travel the other way on one channel: `OperationManager` emits `operation://changed`, the tray
emits `provider://changed`, the deep-link handler emits `deeplink://pending`, and
`src/native/events.ts` validates every payload before any cache is touched.

Forbidden shortcuts: UI touching the file system, a component running SQL, the renderer assembling
a shell command, `invoke`/`listen`/`emit` outside `src/native/`, and product Rust importing an
inherited module directly instead of going through the facade.

## 3. Repository layout

### 3.1 Frontend (`src/`)

```text
src/
  main.tsx                 Bootstrap: i18n, query client, error boundary, startup warm-up, AppRoot
  app/                     Shell: AppRoot (readiness gate), AppShell, routes.ts, route intents,
                           route motion/backdrop, StartupScreen, warmSessionEnvironment.ts
  pages/                   One directory per destination: home, tools, services, extensions, data,
                           settings; plus routing, usage, sessions, workspace which render only
                           inside their parent page's tabs
  features/                Product behaviors: tool-management, provider-management,
                           extension-management, health, task-center, backup, updater,
                           import-existing, deep-link-import, scope-memory. Each exports an
                           index.ts
  entities/                One TanStack Query module per native resource (tool, operation,
                           provider, extension, settings, backup, update, health, import, session,
                           usage, routing, prompt, skill-*, desktop-app, desktop-preferences,
                           network-proxy, openclaw-workspace, deeplink). Query keys, options,
                           mutations
  native/                  The only Tauri boundary: client.ts (invokeNative), events.ts,
                           updater.ts, commands/*.ts (one file per command family), schemas/*.ts
                           (Zod wire schemas; the TypeScript twins of the Rust domain types)
  shared/
    ui/                    Product design-system components (Button, Card, Modal, ScopeTabs,
                           Sidebar, ToolCard, ServiceCard, ProgressTask, ...) and cn.ts
    ui/__gallery__/        Dev-only state matrix, reachable with ?gallery; excluded from builds
    styles/tokens.css      The single source of truth for colour, radius, shadow, motion, type
    lib/nativeError.ts     The only place that turns an error into renderable copy
  i18n/                    index.ts (lazy locale loading) and locales/{en,zh,zh-TW,ja}.json
  lib/                     query/ (queryClient, sessionCache), platform.ts, frontendLogger.ts,
                           windowActivity.ts and a few inherited helpers
  config/                  Inherited provider preset inputs consumed by the preset generator
  components/              Inherited boot-path leftovers: DatabaseUpgrade, FrontendErrorBoundary, ui/
  utils/, types/           Inherited helpers; do not grow them, put new code in a domain directory
```

Rules of placement: a page composes features and entities and holds only page-local UI state; a
feature owns mutations, confirmation dialogs, and derived presentation; an entity owns query keys
and the cache contract for one native resource; `shared/ui` components take props only and never
import `@tauri-apps` or `@/native`.

### 3.2 Backend (`src-tauri/src/`)

```text
src-tauri/src/
  lib.rs                   Tauri setup (product data dir, panic hook, file logger, legacy-identifier
                           migration, Database::init, OperationManager, tray, updater manager) and
                           the invoke_handler registration list
  commands/                app_api.rs, app_deeplink_api.rs, app_desktop_api.rs,
                           app_network_api.rs, app_routing_api.rs,
                           app_session_api.rs, app_skill_api.rs, app_system_api.rs,
                           app_update_api.rs, app_usage_api.rs, app_workspace_api.rs: the product
                           IPC surface. What is left of the inherited command layer is the part the
                           product still reaches: misc.rs (install/probe helpers, called through the
                           facade), config.rs (open_app_config_folder) and the three OAuth state
                           wrappers the proxy forwarder resolves by type
  application/             One service per use case: tool_lifecycle, tool_launch, tool_*_preview,
                           deep_link_import,
                           tool_version_*, provider_directory, provider_preflight,
                           provider_batch_test, extension_directory, mcp_*, skill_*,
                           prompt_directory, product_settings, backup_*, import_existing,
                           health_check, app_update, network_proxy, desktop_*, session_directory,
                           usage_overview, routing_control, openclaw_workspace, reveal
  domain/                  tool, provider, provider_endpoint, provider_runtime, extension, mcp,
                           prompt, skill, operation, health, import, backup, settings, session,
                           usage, routing, desktop_app, app_update, deep_link, error, message_keys
  adapters/                tool_adapter.rs (trait + contexts), registry.rs (AdapterRegistry,
                           UpstreamToolAdapter), lifecycle_runner.rs, uninstall_runner.rs,
                           version_catalog_runner.rs, native_supply.rs
  platform/                mod.rs (Platform), command.rs (CommandSpec, AllowedProgram),
                           executor.rs (+ cancellation), plan.rs, redact.rs,
                           shell_environment.rs, terminal.rs (+ macos.rs, linux.rs),
                           detached.rs, desktop_app.rs
  infrastructure/          paths.rs (product data dir, app.db), operations.rs (OperationManager),
                           logging.rs (log limits)
  repositories/            ccswitch_import.rs (read-only intake of a CC Switch database),
                           tool_version_events.rs (product-owned version history table)
  compat/ccswitch/         The facade, one module per inherited engine: tools, install_probe,
                           lifecycle(_specs), versioning, native_supply, update_preview,
                           tool_launch, tool_paths, provider/ (create, saving, switching,
                           live_preservation, removal, advanced, long_tail, presets),
                           provider/deep_link (link config evidence),
                           provider_endpoints, provider_runtime/ (effective), extension/ (mcp,
                           skill, prompt), skill_catalog, health, import, settings, backup/
                           (transfer, post_restore), backup_schedule, session, usage, routing,
                           workspace, network_proxy, desktop_preferences, app_update,
                           tool_version_storage, paths
  (inherited)              app_config.rs, config.rs, *_config.rs, provider.rs, prompt*.rs,
                           settings.rs, store.rs, tray.rs, database/, services/, mcp/, proxy/,
                           session_manager/, pi_config/, error.rs, ... — CC Switch code, kept in
                           its upstream location so cherry-picks apply cleanly (ADR-0003)
```

### 3.3 Boundary rules (enforced by `pnpm check:boundaries`)

| Rule | Scope                                                           | Statement                                                                                                                                                                                                           |
| ---- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1   | `domain/`, `platform/`                                          | May reference only product crate roots (`domain`, `platform`, `adapters`, `application`, `repositories`, `infrastructure`, `upstream`); `domain` must not mention `tauri`                                           |
| R2   | `application/`, `adapters/`, `repositories/`, `infrastructure/` | Same allowlist; every inherited module is reached through `crate::compat::ccswitch`. One documented exemption: `infrastructure/paths.rs` reads `crate::config` to locate the CC Switch source database (ADR-0002) |
| R3   | `compat/ccswitch/`                                            | The only directory that may `use crate::services`, `crate::database`, `crate::store`, and the other inherited modules                                                                                               |
| R4   | `src/` except `src/native/`                                     | Must not import `@tauri-apps/*`. Allowlisted bootstrap exceptions: `src/main.tsx`, `src/components/DatabaseUpgrade.tsx`, `src/lib/frontendLogger.ts`, `src/lib/windowActivity.ts`                                   |
| R5   | `src/app/`, `src/pages/`                                        | Must not import `@/native`; they reach the backend only through entity and feature hooks                                                                                                                            |

Rules that are reviewed rather than linted:

- Only `repositories/` and the compatibility facade touch SQLite; components, hooks, and
  application services never see SQL or a `Connection`.
- No `if tool === "claude"` branching in UI or application code. Behavior is driven by
  `ToolCapabilities` (`canInstall`, `canUpdate`, `canUninstall`, `canRepair`, `canLaunch`,
  `canManageProvider`, `canManageMcp`, `canManageSkills`, `canManagePrompts`, `canManageVersion`)
  plus the live `Tool.status`; a tool-specific `match ToolId` belongs in the facade or `domain/tool.rs`.
- Features import each other only through `index.ts`. New dependencies, core dependency upgrades,
  and database schema changes need a written rationale or ADR before the change.

## 4. Frontend architecture

### 4.1 Bootstrap and shell

`src/main.tsx` awaits i18n, subscribes to the backend init-error event, checks
`get_init_error` (a too-new database renders the `DatabaseUpgrade` recovery screen instead of the
product), tells the native window its appearance once, starts `warmSessionEnvironment` without
awaiting it, and mounts `AppRoot` inside `QueryClientProvider` and `FrontendErrorBoundary`.

`AppRoot` is the readiness gate: it subscribes once to operation and provider events, starts the
signed-update status query, and renders `StartupScreen` until product settings have loaded. Only
then does it mount `AppShell`, so every page can assume the settings cache is populated. There is
no first-run guide; a first launch goes straight to the shell, with the existing-setup import
asked as a dialog beside it when a compatible setup is found. `AppShell` owns the sidebar, the
single scrollable `main` (scroll resets on every navigation), route motion, lazy-loaded page
chunks, and the one-shot route intents.

`warmSessionEnvironment` issues one silent read of the local inventories (tools, settings, desktop
apps, backups, extensions, proxy, skill sources, sessions, then health and the scoped provider and
extension lists for installed tools); TanStack deduplicates pages that mount while it runs.

### 4.2 Routing

`src/app/routes.ts` is the routing table: `APP_ROUTES = home | tools | services | extensions | mcp |
prompts | data | settings`, and `NAV_ITEMS` is the same order with `settings` pinned to the footer. The sidebar never
folds. `useAppRoute` remembers the last route in `localStorage` (a navigation side effect, not a
preference); unknown or retired values fall back to Home through `isAppRoute`, with no alias table.

Secondary surfaces are tabs inside their parent page, rendered with the shared `ScopeTabs`
component (roving tab stop, arrow/Home/End navigation, `aria-controls`): API Endpoints has
`services | routing | usage` (`pages/services/useServicesTab.ts`). Skills (`extensions`), MCP,
and Global Prompts are independent destinations reusing `ExtensionsPage` with a fixed kind.
Sessions keeps the stable `data` route ID. Per-tool configuration, memory, storage, and the
OpenClaw workspace live in Software details (`ToolDetailsModal`, ADR-0038).

Tab selection is page-local state and never enters `ProductSettings`. Cross-page hand-offs use
`useRouteIntents` (`openServices(tool?, tab?)`, `openExtensions(tab?, kind?)`, `openRoute`);
intents are consumed on mount and cleared by any ordinary navigation. See ADR-0034, ADR-0036,
ADR-0037, ADR-0038. MCP/prompt reveal resolves the live vendor path natively from
scope/kind; missing files reveal the nearest existing directory without writing files.

### 4.3 State

Everything that comes from Rust, SQLite, the file system, or the network is TanStack Query state.
`src/lib/query/queryClient.ts` applies `sessionCacheOptions` by default: infinite `staleTime` and
`gcTime`, no refetch on mount, focus, or reconnect. A local inventory is a snapshot of this desktop
session; it changes only when the user refreshes or a successful mutation invalidates it.

- `entities/*` own query keys (`toolKeys`, `operationKeys`, `settingsKeys`, ...) and the mutation
  contracts. `entities/operation/useOperationEvents.ts` is the single subscriber of
  `operation://changed`; it merges snapshots into the operations cache and invalidates tool,
  extension, skill, and health queries only on terminal states.
- `useSaveProductSettings` (`entities/settings`) is the only place that merges a `Partial` into a
  full settings object; it writes optimistically and rolls back on failure. It never writes without
  a cached baseline.
- Tool inventory is two queries: `app_tools_list` (local, fast) and `app_tools_check_versions`
  (network, session-scoped, no retry). `useToolInventory` overlays the second onto the first only
  when the local versions still match.
- React state holds UI temporaries only: open dialogs, selected tab, form input, hover. Redux,
  MobX, and Zustand are not used and need an ADR to be introduced.

### 4.4 Native layer

`src/native/client.ts` exports `invokeNative(command, schema, payload)`: every response is parsed
with the Zod schema for that command and every rejection is converted to `NativeError`. A response
that fails validation becomes `INTERNAL` / `error.native.responseSchemaMismatch`. `src/native/index.ts`
assembles the `native` object (`native.tools`, `native.providers`, `native.extensions`, ...) and
re-exports the schemas and types that entities use. The renderer sends stable identifiers only
(`ToolId`, provider ids, opaque references, preview fingerprints); paths, commands, package names,
and raw configuration never cross the IPC boundary. Provider drafts and saved
provider API keys are the explicit endpoint-editor exception, validated by typed
schemas; runtime credential evidence itself still contains no secret.

### 4.5 Internationalization

Four locales (`en`, `zh`, `zh-TW`, `ja`) are loaded lazily by `src/i18n/index.ts`; `zh` is the
default, `en` the fallback. Product copy lives in the namespaces listed in
`tests/i18n/messageKeys.test.ts` (`PRODUCT_NAMESPACES`: `error`, `operation`, `nav`, `tool`,
`tools`, `home`, `taskCenter`, `services`, `extensions`, `routing`, `usage`,
`sessions`, `preferences`, `data`). Inherited namespaces are not held to the same guarantees.

Guards:

- Every key in `src-tauri/src/domain/message_keys.rs` (`USER_FACING_MESSAGE_KEYS`) plus the two
  native-side keys must resolve in all four locales; the Rust registry test in the same file scans
  the product directories and fails on any `"error."`/`"operation."` literal that is not
  registered, so the guard is bidirectional.
- Product namespaces must have identical key sets in all four locales, no blank strings, and no
  technical vocabulary (`stderr`, `exit code`, `ENOENT`, ...).
- `tests/config/productProvenanceLocales.test.ts` rejects any mention of the upstream product name
  inside product namespaces. Adding a namespace means adding it to both allowlists.

### 4.6 Design tokens and appearance

`src/shared/styles/tokens.css` is the only source of colour, radius, shadow, motion, and type
values; `tests/shared/styles/tokens.test.ts` and `tailwindTheme.test.ts` fail on drift between the
hex tokens, the Tailwind HSL bridge, and the shadcn aliases. The application has one appearance: a
coloured canvas whose hue follows the active route (`data-route`), with surfaces lifted by
translucent layers. There is no user-facing theme switch. Components come from `src/shared/ui`;
`shared/ui/cn.ts` registers the product type scale with `tailwind-merge` and is the only `cn`
import for product code. Motion respects `prefers-reduced-motion` through tokens.

### 4.7 Error model in the renderer

Errors arrive as `{ code, messageKey, technicalMessage, remediation, contextId }`. Code decides
behavior, `messageKey` and `remediation` go through i18n, and `technicalMessage` is shown only
behind an explicit details affordance (today: the Task Center's error details dialog). Matching on
error strings is forbidden. `src/shared/lib/nativeError.ts` (`toErrorCopy`) is the only conversion
point; anything that is not a `NativeError` becomes `INTERNAL` / `error.native.unrecognized`.
Recoverable failures keep the user's input and reuse the same button as the retry entry, and a
durable in-page alert replaces repeated toasts.

### 4.8 Testing conventions

- Vitest with jsdom, `tests/setupGlobals.ts` and `tests/setupTests.ts`. Tests live under `tests/`
  mirroring `src/` (`tests/app`, `tests/pages`, `tests/features`, `tests/entities`, `tests/native`,
  `tests/shared`, `tests/i18n`, `tests/config`), plus a few colocated `src/**/*.test.ts` for
  inherited preset data.
- `tests/msw/tauriMocks.ts` mocks `@tauri-apps/api/core` so that `invoke(command)` becomes a POST to
  `http://tauri.local/<command>`, answered by the MSW handlers in `tests/msw/handlers.ts` (a test
  overrides them with `server.use`); `emitTauriEvent` drives event listeners.
- Fixtures are inline objects in the test file or shared MSW handlers; there is no `tests/fixtures/`
  directory. Tests assert on translation keys unless they install a locale slice.
- Contract tests in `tests/config/` and `scripts/*.node.mjs` read source files and assert
  structure (route chunks, bundle boundaries, shipped scene models, locale coverage).

## 5. Backend architecture

### 5.1 Startup

`lib.rs` setup resolves the product data directory from the Tauri AppData path and injects it into
`infrastructure::paths` and the panic hook; installs the file logger (`logs/ai-manager.log`,
10 MiB, four archives; `crash.log`, 5 MiB, two archives, entries redacted and capped); creates the
`AppUpdateManager`; runs the one-time allowlisted migration from the inherited product identifier;
initializes the database; registers `OperationManager`, the skill service state, and the OAuth
states used by the proxy; builds the tray. The window contract is `1180x760`, minimum `900x620`,
identifier `tools.aimanager.desktop`; the CSP allows only same-origin resources, data images, and Tauri
IPC; `src-tauri/capabilities/default.json` grants the main window seven permissions.

### 5.2 Commands

Product commands are `#[tauri::command]` functions in `commands/app_*.rs`. They parse and validate
arguments, load state, call one application service, and return `Result<T, AppError>`; blocking
work is wrapped in `spawn_blocking`. The registration list in `lib.rs` contains every `app_*`
command plus three shell commands (`get_init_error`, `open_app_config_folder`, `set_window_theme`).
`scripts/product-native-shell.node.mjs` derives the expected set from the `invokeNative` calls in
`src/native/` and fails when the registered set differs, so a command is wired on both sides at once.

### 5.3 Application services

Each service is a use case with an explicit capability gate: `ToolLifecycleService` (preview
validation, per-tool lock, operation registration, post-action detect and verify, version-event
append), `ProviderDirectory` (list, create from a reviewed preset, custom create, save, switch,
remove, test), `ExtensionDirectory`, `ProductSettingsService`, `BackupDirectory`,
`HealthCheckService`, `AppUpdateManager`, `SessionDirectory`, `RoutingControl`, and so on. Services
depend on ports (traits) and facade types, never on inherited types.

### 5.4 Domain

`domain/` defines the product models that cross the IPC boundary: `Tool` (id, status,
capabilities, discovery, versions), `Provider` and its drafts and profiles, `EffectiveConnection`,
`Extension` (deliberately without any configuration payload), `Operation` (id, kind, status,
progress, phase key, bounded redacted log, optional output), `HealthSnapshot`, `ProductSettings`,
`BackupList`, `SessionSummary`/`SessionThread`, `UsageOverview`, `RoutingOverview`, desktop app
models, and `AppUpdateStatus`. `ToolId` is the ten-entry registry (`claude-code`, `codex`,
`opencode`, `gemini-cli`, `grok-build`, `openclaw`, `hermes`, `pi`, `kimi-code`, `deepseek-dsh`);
the mapping to the inherited `AppType` lives in `compat/ccswitch/tools.rs` (ADR-0004,
ADR-0005). Every Rust model has a Zod twin in `src/native/schemas/`.

### 5.5 Adapters

`ToolAdapter` (`adapters/tool_adapter.rs`) is the per-tool interface: `capabilities`, `detect`,
`detect_local`, `install_source`, `install`, `update_preview`, `validate_update`, `update`,
`install_version`, `version_catalog`, `repair`, `uninstall`, `launch`. `AdapterRegistry` builds one
`UpstreamToolAdapter` per `ToolId`; the adapter asks the facade for probes and plans and hands them
to the runners: `lifecycle_runner` (executes `LifecyclePlan`s through `CommandExecutor`, applies
the community-mirror retry policy), `uninstall_runner` (pre-checks every path before deleting
anything), `version_catalog_runner` (registry queries), and `native_supply` (ADR-0033). Adapter
tests inject a fake executor and never spawn processes.

### 5.6 Platform

`platform/` is where `cfg(target_os)` is allowed. `Platform::current()` replaces ad-hoc cfg checks
in business code. `CommandSpec { program, program_path, args, env, timeout, sensitive_args }` is
the only way to describe a process: `program` is an `AllowedProgram` (npm, pnpm, bun, brew, volta,
uv, pipx, winget, powershell, cmd, bash, osascript, open, codesign, wsl, env, the four Linux
terminal programs, the two desktop launchers, and the ten tool binaries for self-update and
launch), `program_path` anchors the absolute executable so a narrow GUI `PATH` cannot produce
`exit 127`, and arguments are always an array. `executor.rs` runs specs with polling, timeouts,
cancellation tokens (process-tree termination confirmed before a `Cancelled` state), and redacted,
bounded output. `shell_environment.rs` runs the user's login shell once through the allowlisted
`bash` trampoline to learn the real terminal environment. `terminal.rs` hands interactive launches
to Terminal.app (constant AppleScript, paths as argv), Windows/WSL allowlisted programs, or the
four Linux terminals via `/usr/bin/env -C`. `redact.rs` masks credential-shaped tokens.

### 5.7 Infrastructure

`infrastructure/paths.rs` resolves the product data directory (Tauri AppData; a test fallback
under `AI_MANAGER_TEST_HOME`) and `app.db`; the CC Switch database path is exposed only for the
read-only import. `infrastructure/operations.rs` is `OperationManager`: an in-memory state machine
(`queued | running | success | failed | cancelled`), per-tool and per-desktop-app mutation locks,
cancellation registration for the safe phases only, bounded retained history (200 terminal
operations, 120 log entries of at most 4 KiB), and the `operation://changed` emitter behind an
`OperationEvents` trait so tests need no Tauri runtime. Read-only kinds (`testProviders`, `scan`)
do not take the tool lock.

### 5.8 Repositories and SQLite

The product owns `app.db` under its data directory and reuses the inherited schema and migration
chain (ADR-0002); the current schema version is 20, and versions 18 to 20 are product-owned tables
(verified tool version events, Claude Desktop MCP projection). Inherited tables are reached through
the inherited DAOs behind the facade; `repositories/` holds only shapes the facade does not own:
`ccswitch_import.rs` opens a CC Switch database read-only, copies it into memory, and merges
allowlisted `providers`, `provider_endpoints`, `mcp_servers`, and `skills` rows inside a staged
transaction; `tool_version_events.rs` implements the `ToolVersionEventRepository` port. Any schema
change requires an ADR first, and no migration is ever deleted or rewritten; `Database::init` may
only perform idempotent key cleanup in the `settings` KV table without bumping the version.

### 5.9 Compatibility layer contract

`compat/ccswitch/` is a facade, not a copy: each module wraps one inherited service (for example
`ProviderStore` over `ProviderService`, `ExtensionStore` over the MCP/Skill/Prompt services,
`SettingsStore` over the `settings` KV table, `BackupStore` over `Database` backups,
`SkillCatalogStore` over `SkillService`, `SessionStore` over `session_manager`) and converts
inherited models and errors into domain types. Inherited error strings are redacted and truncated
into `technical_message`; they never become user copy.

- What may be called: only through a facade module. New product code never adds `use
crate::services::...` outside `compat/ccswitch/`.
- Changes to inherited files: the default is zero. The accepted exception is visibility widening
  (`mod x;` to `pub(crate) mod x;`, or `fn` to `pub(crate) fn`) so the facade can reuse an
  existing function. A behavioral change to an inherited file must fix a real bug that would be
  sent upstream and is recorded in the sync ledger.
- Files the product owns outright: the product layers listed in section 3.2, `lib.rs` setup and
  registration, `tauri.conf.json` and `capabilities/`, `Cargo.toml` identity, the provider preset
  catalog (`provider/provider_presets.generated.json`, generated from the inherited
  `src/config/*ProviderPresets.ts` inputs by `pnpm generate:provider-presets` and reviewed under
  ADR-0016), all of `src/` except the inherited helper directories, and `scripts/`.
- Files that track upstream: the inherited backend modules, the preset inputs, and the inherited
  locale namespaces; they are refreshed by cherry-pick or per-file checkout during a sync.

### 5.10 Error model

`domain/error.rs` defines `AppError { code, message_key, technical_message, remediation,
context_id }` serialized as camelCase JSON, and the stable `ErrorCode` set: `TOOL_NOT_FOUND`,
`INSTALL_FAILED`, `UPDATE_FAILED`, `UPDATE_PREVIEW_STALE`, `UNINSTALL_FAILED`, `LAUNCH_FAILED`,
`CONFIG_PARSE_FAILED`, `CONFIG_WRITE_FAILED`, `PROVIDER_UNREACHABLE`, `PROVIDER_NOT_FOUND`,
`SESSION_NOT_FOUND`, `EXTENSION_NOT_FOUND`, `BACKUP_NOT_FOUND`, `MCP_UNAVAILABLE`,
`PERMISSION_DENIED`, `NETWORK_ERROR`, `OPERATION_CONFLICT`, `UPSTREAM_ERROR`, `INTERNAL`. Renaming
a code or a message key is a breaking change. Business paths return `Result<T, AppError>`; `unwrap`
and `expect` are reserved for true invariants with a comment. The inherited `crate::error::AppError`
is a different type that serializes to a string; it never reaches the renderer.

### 5.11 Secrets and redaction

API keys, tokens, cookies, and credential-bearing proxy URLs are never logged.
The user-directed local endpoint UI displays saved `Provider.apiKey` values in full,
prefills them in edit forms, and offers explicit clipboard buttons. Runtime
credential-status metadata must not suppress these saved values, and it never
contains raw auth-store or environment credentials. Subprocess
output, inherited error strings, panic messages, and URLs pass through `platform::redact` (known
prefixes such as `sk-`, `ghp_`, `xai-`, `AIza`; `name=value` pairs whose name hints at a secret;
`Authorization: Bearer ...`). Release logs default to INFO. There is no OS keychain-backed secret
store yet (section 9).

### 5.12 Managed-file write path

Files that belong to a tool (`~/.claude/settings.json`, `~/.codex/config.toml`, prompt files, and
so on) are modified only through the eight-step sequence from the design specification: read,
validate, back up (timestamped), modify, write to a temporary file, validate again, atomic
replace, verify by reading back; any failure rolls back to the backup. The inherited
`config::atomic_write` / `atomic_write_private` provide the temp-plus-rename primitive (private
files are `0600` on Unix); `compat/ccswitch/extension/prompt/live.rs` is the reference
implementation. Product-managed files keep exactly one write entry: when a capability such as
`canManagePrompts` says the product manages a file, other surfaces only locate or open it and hand
management over instead of writing (ADR-0037).

### 5.13 Command allowlist

Shell execution never starts from a string. Plans are built in the facade (`lifecycle.rs`,
`lifecycle_specs.rs`) as `CommandSpec` values, checked against `AllowedProgram`, anchored to the
probed executable, and run by the platform executor with a timeout. Network-facing installers get
the download proxy (loopback only, no credentials) through the child environment; only npm-family
installers may retry once against the community registry after a recoverable failure. Uninstall
targets must be lexically inside the home directory, in a tool-owned root, and are canonicalized to
refuse symlink escapes; the whole batch is pre-checked before the first deletion.

## 6. Key behaviors

### 6.1 Tool lifecycle and operations

`app_tools_list` returns the local inventory from `detect_local`; `app_tools_check_versions` adds
the latest versions from the network. Install, update, change version, repair, and uninstall begin
with a read-only preview that returns a fingerprint; the command validates it, the adapter re-probes
and rebuilds the plan under the per-tool lock, and fails closed when the plan changed. Progress
arrives as `Operation` snapshots with phase keys from `domain::operation::phase`. Cancellation is
offered only in `preparing`/`downloading`; the runner confirms the process tree is gone before the
terminal state. Successful product-initiated installs, updates, and version changes append a
verified `tool_version_events` row (ADR-0019); ownership-preserving version changes and the Hermes
uv/pipx ownership proof follow ADR-0010 and ADR-0020. Repair is opened only for the one proven
recovery shape (Codex npm-sibling breakage on POSIX).

### 6.2 Provider switching, live preservation, and effective connection (ADR-0035)

Switching a service reuses the inherited `ProviderService::switch` (database current row plus live
file write). `compat/ccswitch/provider/switching.rs` wraps it: it captures the live file first,
lets upstream write, then pastes back every key that is not a connection key (Claude `env`
entries outside `ANTHROPIC_*` and related prefixes, Codex keys outside `model_provider*`/`model`
and the MCP projection, Gemini keys outside the API key and base URL) so hooks, permissions, and
project variables survive a switch; for Claude it writes `""` for missing connection variables so a
relay variable left in the shell cannot silently override the choice. Preservation is skipped while
proxy takeover is active, and a failed upstream switch is undone to the previous provider.
`ProviderRuntimeContext` carries a read-only `EffectiveConnection` (endpoint, credential state, and
the source of each: live config, rc file, environment, tool default) evaluated per tool in
`provider_runtime/effective.rs` against the login-shell environment; it never contains a secret.

### 6.3 Takeover and local routing (ADR-0007)

`RoutingControl` exposes the inherited local gateway for Claude Code, Codex, Gemini CLI, and Grok
Build only: overview, explicit takeover, priority queue, hot switch, automatic failover, and stop
all. Every mutation holds one product-level async lock; enabling failover requires an active
takeover and a healthy first target. The domain projection contains provider ids, names,
priorities, health summaries, and counts; credentials, request bodies, live backups, notes, and
paths never cross IPC. The product startup does not activate the inherited cloud sync or usage
polling services; deep links are handled by the product's own parser (section 6.9), never by the
inherited one.

### 6.4 Native supply and restricted networks (ADR-0033)

Updating a natively installed Claude Code first probes the official channel (one bounded HEAD);
if unreachable, or if `claude update` fails, `adapters/native_supply.rs` downloads the platform
package Anthropic publishes on the npm registry (official first, community mirror only after a
recoverable failure under the Automatic policy), verifies `dist.integrity` (sha512 only), extracts
`package/claude` into the official versions directory (`.partial` then rename), verifies the Apple
Team identifier with `/usr/bin/codesign` on macOS and the reported `--version`, then atomically
repoints the launcher; failure deletes only the new file. Standalone Codex installs are native
ownership: `codex update`, then the repository install script forced onto GitHub Releases. Windows
and musl targets fail closed, and the product never mirrors vendor binaries.

### 6.5 Sessions and local data (ADR-0008, ADR-0036)

The Sessions destination shows the session browser; per-tool local context and storage now
live in Software details (ADR-0038). `SessionStore` reuses the inherited session scanners for
the eight tools that have them and returns titles, summaries, project basenames, times, and opaque
references; bodies are read only after explicit selection and under byte limits; resume rebuilds
argv on the backend and hands it to the terminal launcher. Local context items report path and
size; the prompt item defers to the extensions page when `canManagePrompts` is true (ADR-0037).

### 6.6 Extensions (ADR-0014, ADR-0022, ADR-0037)

`ExtensionStore` projects MCP servers, Skills, and Prompts into one `Extension` shape per
`(tool, kind)` scope without configuration payloads. Guided MCP installation accepts a strict
inbound draft (name, stdio command and arguments, or an HTTPS/loopback URL; never JSON, env,
headers, or tokens); Skill installation, ZIP installation, updates, backups, and trusted sources
wrap the mature `SkillService`; Prompt bodies are read only on demand and the active file is
edited through the eight-step path. Skill and MCP tasks take the same `OperationManager` lock.

### 6.7 Backups, transfer, and import (ADR-0002, ADR-0030)

`BackupStore` wraps database backups (create, list, restore, rename, delete, schedule); restore
takes a safety snapshot first and re-projects only the current-service choice, leaving extension
files untouched. Configuration archives are exported as private files and imported through a
staged temporary database with an authorizer that rejects cross-file statements; the archive is
plaintext SQL today, so the UI warns that it may contain service keys (the encrypted format of
ADR-0030 is proposed, not implemented). Importing an existing CC Switch setup is read-only on the
source and never rewrites live tool configuration; the user selects a service afterwards.

### 6.8 Updater trust (ADR-0013, ADR-0018)

`AppUpdateManager` is the only user of the Tauri updater plugin; the renderer observes it through
four narrow commands. `channelReady` is true only for protected release builds that inject the
channel at build time with HTTPS endpoints and the product minisign public key; otherwise the
phase is `unconfigured`, no request is made, and the too-new-database screen cannot download a
build. Stable builds read two manifests in fixed order; prerelease builds read an isolated staging
manifest. Retries are bounded to three attempts for transient failures only; signature, shape, and
authorization failures stop immediately, and a version that disappears between retries is a
failure, not "up to date".

### 6.9 One-click import by deep link (ADR-0029)

The product registers exactly one scheme, `aimanager`, and never co-registers the inherited one.
`domain/deep_link.rs` is the single parser both paths use: a strict allowlist of scheme, host,
path and query, with duplicate keys, unknown keys, fragments, userinfo, NUL, over-long fields and
malformed Base64 refused, and an 8 KiB cap on the raw URL. `x-` prefixed parameters are ignored
rather than rejected, so the format can grow.

The only difference between the two paths is the credential policy. A link the operating system
delivers (cold-start `get_current`, or a second instance the single-instance plugin forwards) is
`LinkOrigin::Argv` and may not carry a key in `apiKey` or inside `config`; a link the user pastes
into Settings is `LinkOrigin::Paste` and may, and also accepts the upstream scheme and a bare
`v1/import?...` query. Credential detection reuses `platform::redact`, cross-checked in
`compat/ccswitch/provider/deep_link.rs` against `api_key_slots` and the upstream credential
reader; a tool with no upstream service format fails closed.

`application/deep_link_import.rs` holds an in-memory queue of at most eight entries with a
ten-minute expiry, hands the renderer only an opaque pending id, and projects a safe preview
(source, target, and the exact change) in which a credential appears as the field that holds it
and never as a value. Confirming consumes the entry and then calls the ordinary application
entries — `provider_directory::create_custom`, `prompt_directory::save`,
`SkillInstallationService`, `McpInstallationService` — so every capability gate, validation step
and task lock applies unchanged. Rejection, expiry and a second press produce no writes. The raw
URL is never logged on any path.

## 7. Upstream sync discipline (ADR-0001, ADR-0003)

- `upstream` is the CC Switch remote; `origin` is the product repository. `upstream/main` is never
  merged.
- A sync happens on a temporary `sync/ccswitch-<upstream-sha>` branch. Read the upstream changelog
  and commit list, then bring over only backend, parser, and tool-support changes with
  `git cherry-pick -x` or, for data-plane directories, per-file `git checkout upstream/main -- <path>`.
  Renderer, README, locale, and upstream test commits are not ported.
- Product-owned files (section 5.9) are never overwritten by a per-file checkout; when a directory
  is refreshed wholesale, the product variants inside it are restored and the reason recorded.
- Preset inputs under `src/config/` are refreshed the same way, followed by
  `pnpm generate:provider-presets` and a review of the generated catalog diff.
- Every sync runs the full gate set from section 8 and records the upstream range, the per-commit
  decision (absorbed, skipped, deferred pending ADR), and the evidence in
  `docs/development/UPSTREAM_SYNC.md`. A change that needs a schema migration is
  deferred to its own ADR rather than absorbed inside a sync.
- The root AGPL `LICENSE`, the upstream `LICENSE-MIT`, upstream copyright headers, and
  `THIRD_PARTY_NOTICES.md` are preserved.

## 8. Quality gates and conventions

Run locally before every commit; CI runs the same set.

```bash
pnpm typecheck
pnpm lint                  # eslint flat config; typescript-eslint, react-hooks, jsx-a11y
pnpm format:check          # prettier over src and tests
pnpm check:boundaries      # section 3.3
pnpm test:unit             # vitest
pnpm test:release          # node --test scripts/*.node.mjs (product entry, native shell, release contract)
pnpm check:release         # release configuration audit; --strict for release builds
pnpm build:renderer
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cd src-tauri && NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo test
```

`NO_PROXY` matters on machines with a system proxy: the inherited proxy tests talk to a local mock
upstream, and reqwest would otherwise route those requests through the system proxy.

Assertions that must be updated deliberately when the surface changes:

- `src-tauri/src/domain/message_keys.rs`: `USER_FACING_MESSAGE_KEYS` is sorted, duplicate-free,
  and its length is asserted (currently 316). Adding a key means adding it in all four locales.
- `scripts/product-native-shell.node.mjs`: the registered command set must equal the set of
  commands the renderer invokes; the window, CSP, and capability contracts are asserted there too.
- `scripts/product-entry.node.mjs`: the six lazily loaded page chunks and the absence of merged
  page routes.
- `tests/config/spatialPageVisuals.test.ts`: the five shipped scene models.
- `tests/i18n/messageKeys.test.ts` and `tests/config/productProvenanceLocales.test.ts`: the
  product namespace allowlists.

Conventions:

- Size limits: React component under 250 lines, hook under 200, Rust service under 500. Split
  rather than grow; files already over the limit are listed in section 9.
- No catch-all `utils.ts`, `helpers.ts`, `common.rs`, `misc.rs` in product code; no `any` in new
  TypeScript, unknown input is narrowed with Zod or type guards.
- Changed behavior comes with tests; existing tests are not deleted. Rust tests that touch process
  environment or global caches join the existing `serial_test` group, and never touch the real
  home directory: use `tempfile::TempDir` and `AI_MANAGER_TEST_HOME`.
- A page is complete only with loading, empty, success, warning, error, and disabled states,
  keyboard operability, and all four locales.
- ESLint is not adopted; typecheck, Prettier, and the contract tests are the lint layer.

## 9. Known technical debt

Items still true in the current tree. Fix them in a scoped change; do not extend them.

1. Provider API keys are stored in plaintext in the inherited `providers.settings_config` column,
   and therefore in database backups and exported archives. There is no `SecretStore`; the
   product guarantees redaction in logs; the user-directed endpoint UI explicitly
   receives saved keys through typed IPC for local display, editing and copying.
2. `ProviderService::switch` warnings (including a failed Codex `auth.json` cleanup when switching
   back to the official service) are only logged by `log_switch_warnings`; the UI reports success.
3. MCP toggles commit the database flag before writing the tool's live configuration
   (`services/mcp.rs`); a failed live write leaves the switch on while the tool does not see the
   server. The failure copy says so; a transactional fix needs an upstream change.
4. Renaming a Pi service fails: `services/provider/pi.rs` applies `align_native_display_name` in
   `add` but not in `update`, while `list` re-syncs the native name, so the save identity check
   rejects the new name.
5. A Windows tool directory that resolves into WSL is handled by launch and detection but not by
   the capability table or the install probe, so a lifecycle plan can target the host instead.
6. On Windows, a global CLI whose `%APPDATA%\npm\<tool>.cmd` has no sibling `npm.cmd` is reported
   `Unmanaged` (fail-closed) because proving npm ownership would need subprocess evidence.
7. Bun-owned tools resolve their version catalog through whatever `npm` is on `PATH`, because Bun
   has no registry query that works outside a project (`versioning.rs`).
8. CC Switch import copies `skills` rows but not the Skill directories, so imported Skills fail
   product-side toggles until reinstalled.
9. Usage sync uses a line-offset cursor (`services/session_usage.rs`) that counts an unfinished
   last line; the upstream byte-cursor fix requires schema version 21 and a separate ADR.
10. Session parsers materialize every message of a transcript before `SessionStore::thread`
    applies its byte and count caps; very large JSONL files are parsed fully into memory.
11. `AppUpdateManager`: when the primary manifest fails and a lagging fallback returns no update,
    the phase becomes `upToDate`; `empty_check_outcome` only guards the case where a version was
    already seen.
12. Files over the size guideline: `infrastructure/operations.rs` (1491 lines),
    `compat/ccswitch/provider_runtime/effective.rs` (1057), `platform/desktop_app.rs` (862),
    `commands/app_api.rs` (781), `application/tool_lifecycle.rs` (743),
    `compat/ccswitch/provider.rs` (687), `compat/ccswitch/skill_catalog.rs` (616), and
    `src/pages/tools/ToolsPage.tsx` (524; the shell plus `InstalledToolsPanel` split is pending).
13. Windows and Linux musl native supply fail closed (`native_supply::target_for` returns `None`):
    the official Windows layout and musl `--version` behavior are unverified, and Windows
    cross-compilation has not been completed locally.
14. Restoring an older native Claude Code version is not wired to native supply:
    `supports_version_restore` still treats `NativeInstaller` as a restricted source, so rollback
    needs the vendor download host.
15. The `ToolId` to `AppType` mapping exists twice (`compat/ccswitch/tools.rs::tool_id_to_app_type`
    and `compat/ccswitch/provider.rs::app_type_for`), kept aligned by a pairing test.
16. `SettingsStore::save` issues nine independent KV writes; a mid-sequence failure leaves a partial
    preference set, and restore succeeds even when writing preferences back fails.
17. `Database::restore_from_backup` does not check the `Ok(None)` outcome of the safety backup, and
    `require_known` has a small TOCTOU window against the retention policy (defense in depth only).
18. Resolved by ADR-0039: unreadable/malformed live configuration reports unknown selection
    instead of matching the official provider. OpenCode additionally distinguishes explicit
    default models from recent history and reads credential presence from its auth store.
    Arbitrary project/session/CLI overrides and alternate OpenCode config merges are not resolved;
    alternate config environment variables fail closed rather than claiming the standard file applies.
19. The login-shell probe (`-lic`) leaves background processes started by rc files orphaned on the
    success path, and the inherited rc keyword table does not know `CODEX_API_KEY`,
    `CODEX_ACCESS_TOKEN`, or `GOOGLE_API_KEY`, so those variables get no file attribution.
20. Switching Claude Code to the official service writes `""` for the `ANTHROPIC_*` connection
    variables, which also suppresses credentials that live only in the shell; this is logged, not
    explained in the UI.
21. The sessions filter lists all ten tools while the backend has session sources for eight; the
    proper fix is a `canBrowseSessions` capability, not a hard-coded list.
22. The `services.runtime.*` copy namespace belongs to components that now live in `pages/data/`;
    moving it is a pure rename that has not been done.
23. `technicalMessage` is viewable only in the Task Center details dialog; failures outside
    operations (for example extension toggles) have no place to show details and write no log line.
24. The legacy `advancedMode` field is still on the wire and in the KV store, normalized to `true`
    by `ProductSettingsService`; the `ensure_advanced` gates in `routing_control.rs` and
    `session_directory.rs` are dead checks to remove together with the field.

## 10. Glossary

- **AppError**: the structured product error (`code`, `messageKey`, `technicalMessage`,
  `remediation`, `contextId`); the only error shape that crosses IPC.
- **CommandSpec / AllowedProgram**: the array-based process description and the closed set of
  programs the executor may start.
- **Compatibility layer / facade**: `src-tauri/src/compat/ccswitch/`, the only code allowed to
  use inherited CC Switch modules.
- **Effective connection**: the endpoint and credential a tool will actually use, with the source
  of each, computed read-only (ADR-0035).
- **Live configuration / live preservation**: the file a tool reads at runtime, modified only
  through the managed write path; preservation pastes non-connection keys back after a switch.
- **Native supply**: fetching the vendor's own signed binary from the npm registry into the
  official layout when the vendor channel is unreachable (ADR-0033).
- **Operation**: a long-running task tracked by `OperationManager` with progress, phase, bounded
  redacted log, and a terminal state.
- **Preview fingerprint**: the SHA-256 the renderer returns to confirm exactly the plan it was
  shown; a changed plan fails closed.
- **Product data directory**: the Tauri AppData directory holding `app.db`, backups, logs, and
  product-managed skill storage.
- **ProductSettings**: the small preference set stored under `aimgr.*` keys in the inherited
  `settings` table (import prompt, scopes, download strategy, failover, terminal).
- **Takeover**: the inherited local gateway intercepting a tool's requests for routing and
  failover (ADR-0007).
- **ToolAdapter / ToolCapabilities**: the per-tool implementation of lifecycle actions and the
  boolean table that drives which actions the UI and application offer.
- **ToolId**: the ten-entry product tool registry; `AppType` is the inherited identifier it maps
  to inside the facade.
