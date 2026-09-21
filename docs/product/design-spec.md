# AI Manager — Product and Technical Design Specification v1.0

## Product Requirements, Technical Architecture, UI Design and AI Coding Rules v1.0

> This document is the highest-level engineering specification of this project.
>
> All AI coding agents, developers and subsequent refactoring tasks must comply with this document first.
>
> When existing code conflicts with this document, it is not allowed to keep growing the wrong architecture to accommodate old code; a minimal migration plan must be proposed first.
>
> Large-scale refactoring without an explanation is not allowed.
>
> Rewriting CC Switch low-level functionality that already works stably in one go is not allowed.
>
> The goal of the first phase of this project is not to prove technical capability, but to deliver, at the lowest development cost, a beginner experience that is clearly better than traditional developer tools.

# 1. Project Definition

## 1.1 Tentative Product Positioning

The final product brand name has not been decided yet.

This document consistently uses:

**AI Manager**

as the development codename.

Chinese product positioning:

**AI Coding Butler** (the Chinese-language product name)

English category description:

**AI Coding Manager**

Core one-liner:

**Help ordinary users install, manage, configure and maintain AI coding tools such as Claude Code, Codex and OpenCode.**

The product is not a new AI Coding Agent.

The product does not replace Claude Code, Codex or OpenCode.

The product manages these tools.

It can be understood as:

**A "PC manager" utility for the AI coding tool space.**

The product experience direction takes inspiration from CleanMyMac's simplicity, polish, state visualization and one-click operations, but must not copy CleanMyMac's specific interface, icons, illustrations, layout details, brand elements or visual assets.

---

# 2. Core Development Principles

## 2.1 Phase One Principles

The first version should not chase a large number of original features.

Phase one prioritizes completing:

1. Reuse the mature low-level capabilities of CC Switch
2. Restructure the information architecture
3. Redo the UI
4. Lower the barrier to use
5. Add installation and management entry points
6. Add a unified status home page
7. Support Windows and macOS
8. Establish architectural isolation, leaving room for future feature expansion

Rough targets for the first version:

**70% low-level capability reuse**

**25% UI and product logic restructuring**

**5% new capabilities**

Building a large environment diagnostics system, a large cleanup system or a complex AI Doctor during the MVP phase for the sake of so-called differentiation is not allowed.

---

# 3. Upstream Project Strategy

## 3.1 CC Switch Positioning

CC Switch is the primary upstream code source for this project.

Reuse is allowed for:

- Claude Code configuration management
- Codex configuration management
- OpenCode configuration management
- Gemini CLI related capabilities
- Provider management
- Provider switching
- MCP
- Skills
- Prompts
- Usage
- Backup
- Proxy
- Failover
- Session
- Environment detection
- Installation detection
- Update detection
- Parsing of existing configuration
- SQLite data layer
- Rust Service
- Rust DAO
- Verified cross-platform logic

Do not reimplement capabilities that already work stably just so the code looks like your own.

## 3.2 The Product Must Not Become a Simple Reskin

The new frontend must not directly call the original CC Switch Commands all over the place.

A product API layer of our own must be added.

The architecture must form:

```text
New UI
   ↓
Application API
   ↓
Product Services
   ↓
Compatibility Layer
   ↓
CC Switch Existing Services
   ↓
OS / Config Files / SQLite
```

This guarantees that when the underlying implementation is replaced in the future, the entire UI does not need to be rewritten.

---

# 4. Upstream Compatibility Layer

This is one of the most important architectural designs of the entire project.

Establish:

```text
src-tauri/src/compat/
```

to isolate the CC Switch upstream code. The directory holds code this project
wrote to call upstream; it is not where upstream code lives.

Recommended:

```text
src-tauri/src/
    commands/
    application/
    domain/
    adapters/
    repositories/
    platform/
    compat/
    infrastructure/
```

where:

```text
compat/
    ccswitch/
```

is only responsible for:

- Calling the original CC Switch Services
- Converting CC Switch data into this product's Domain Models
- Shielding changes in CC Switch's internal structure

The frontend must never know about internal objects such as:

```text
CCSwitchProvider
CCSwitchAppType
CCSwitchMcpConfig
```

They must be converted into our own:

```text
Tool
Provider
Extension
HealthItem
Operation
```

---

# 5. Git and Upstream Update Strategy

The project may start as a fork of CC Switch.

Keep:

```text
origin
upstream
```

where:

```text
origin = this project's repository
upstream = CC Switch
```

Blindly merging the entire upstream/main directly is forbidden from now on.

Because the UI will gradually diverge completely.

When updating CC Switch in the future:

1. Create a temporary sync branch
2. Review the ChangeLog of the new CC Switch version
3. Identify the commits related to the Rust backend, configuration parsing and tool support
4. Prefer cherry-picking
5. Run the full test suite
6. Then merge into main

Branch example:

```text
sync/ccswitch-3.x.x
```

Restoring this product's UI to the CC Switch UI for the sake of syncing upstream is not allowed.

---

# 6. License Rules

The original MIT License must be preserved.

Keep in the root directory:

```text
LICENSE
```

Additionally create:

```text
THIRD_PARTY_NOTICES.md
```

listing:

```text
CC Switch
Copyright Jason Young
MIT License
```

If CC Switch files are heavily modified, existing legitimate copyright notices must not be removed.

The product brand, icons, name and new UI may use our own brand entirely.

---

# 7. Technology Stack

## 7.1 No Technology Stack Upgrades in the MVP

First-version principle:

**Inherit the current CC Switch technology stack first; do not upgrade frameworks while changing the product.**

Frontend:

```text
React
TypeScript
Vite
Tailwind CSS
TanStack Query
react i18next
react hook form
Zod
shadcn/ui
```

Backend:

```text
Tauri 2
Rust
Tokio
Serde
SQLite
thiserror
```

Testing:

```text
Vitest
React Testing Library
MSW
Cargo Test
```

During phase one, proactively upgrading core dependencies such as React, Tauri and Tailwind for the sake of newer versions is forbidden.

Establish a separate Upgrade ADR only after a stable MVP is complete.

---

# 8. Overall Architecture

Adopt:

```text
Presentation
Application
Domain
Infrastructure
Platform
Upstream
```

A six-layer design.

## 8.1 Presentation

React UI.

Responsible only for:

- Display
- User interaction
- Forms
- Animation
- State feedback

Must not be responsible for:

- File operations
- Shell operations
- PATH detection
- Configuration file modification
- Raw SQLite queries
- Provider configuration conversion
- API Key storage

---

# 9. Application Layer

Responsible for Use Cases.

For example:

```text
ScanEnvironment
InstallTool
UpdateTool
UninstallTool
SwitchProvider
AddProvider
TestProvider
InstallMcp
RemoveMcp
InstallSkill
BackupConfig
RestoreConfig
ImportCCSwitch
```

The Application layer coordinates multiple Domain Services.

For example:

```text
InstallTool
```

may call:

```text
ToolRegistry
Installer
OperationManager
BackupService
HealthService
```

---

# 10. Domain Layer

Domain must not depend on React.

Domain must not depend on Tauri.

Domain must not depend directly on Windows or macOS APIs.

Domain defines the product's core models.

For example:

```ts
Tool;
ToolStatus;
Provider;
ProviderStatus;
Extension;
HealthStatus;
Operation;
VersionInfo;
```

The Rust side establishes the corresponding types.

---

# 11. Tool Adapter Architecture

All AI tools must implement a unified Adapter.

For example:

```text
ToolAdapter
```

Capabilities include:

```text
detect
get_version
get_latest_version
install
update
uninstall
repair
get_config
backup_config
restore_config
get_auth_status
```

Different tools have different capabilities.

Therefore an additional structure must be provided:

```text
ToolCapabilities
```

Example:

```text
can_install
can_update
can_uninstall
can_repair
can_manage_provider
can_manage_mcp
can_manage_skills
```

The UI is forbidden from writing:

```text
if tool === "claude"
```

and then adding special-case logic everywhere.

The UI must be decided based on:

```text
capabilities
```

---

# 12. Tools Supported in Phase One

P0:

### Claude Code

Must support:

```text
Detect
Install
Version
Update
Uninstall
Provider
MCP
Skills
Prompts
```

### Codex

Must support:

```text
Detect
Install
Version
Update
Uninstall
Provider
MCP
```

### OpenCode

Must support:

```text
Detect
Install
Version
Update
Uninstall
Provider
MCP
```

P0.5:

### Gemini CLI

Keep it if the original CC Switch implementation can be reused at low cost.

OpenClaw, Hermes and similar tools may continue to exist at the low level in the first version, but Beginner Mode does not need to display all of them.

---

# 13. Platform Abstraction

All differences between Windows and macOS must be fully isolated.

Establish:

```text
platform/
    mod.rs
    macos/
    windows/
```

For example:

```text
PlatformService
ShellService
PathService
ProcessService
CredentialService
PackageManagerService
```

Business code is forbidden from being littered with:

```rust
#[cfg(target_os = "windows")]
```

Platform checks must be concentrated as much as possible inside:

```text
platform/
```

---

# 14. Shell Execution Safety Rules

The frontend must not concatenate Shell commands directly.

Forbidden:

```text
"npm install " + userInput
```

Forbidden:

```text
powershell userInput
```

All commands must be converted into:

```text
CommandSpec {
    program,
    args,
    env,
    timeout,
    sensitive_args
}
```

Arguments must be passed as arrays.

Executing concatenated strings is forbidden.

For example:

Correct:

```text
program = "npm"
args = ["install", "-g", package]
```

Forbidden:

```text
command = "npm install -g " + package
```

Installers must have an Allowlist.

For example, only allow controlled commands from:

```text
npm
pnpm
bun
brew
winget
powershell
cmd
```

---

# 15. Tauri Permission Principles

All Tauri Capabilities follow the principle of least privilege.

Forbidden:

```text
allow all filesystem
allow all shell
```

Only allow access to the paths and commands this product actually needs.

If different windows have different permissions in the future, separate Capabilities should be established.

The frontend must not obtain more system permissions than its actual functionality requires.

---

# 16. Data Layer

The first version continues to use SQLite.

But this product must have its own database.

Directly using:

```text
~/.cc-switch/cc-switch.db
```

as this product's database is forbidden.

This product's database:

```text
app.db
```

is stored under the Tauri AppData path.

Hard-coding by hand is forbidden:

```text
~/Library/...
%APPDATA%...
```

The Tauri Path API must be used.

---

# 17. CC Switch Import

If CC Switch is detected on first launch:

Display:

## Import existing setup

Chinese wording:

## Existing AI Configuration Found

Content:

```text
12 AI Services
8 MCP Servers
15 Skills
```

Buttons:

```text
Import
Skip
```

During import:

1. Read
2. Parse
3. Convert
4. Write into our own database
5. Do not modify the original CC Switch database

Both programs should be allowed to be installed at the same time.

---

# 18. External Configuration Write Rules

Configuration files of Claude Code, Codex, OpenCode and others do not belong to this product.

Any modification must perform:

```text
Read
Validate
Backup
Modify
Write Temp
Validate Again
Atomic Replace
Verify
```

On failure:

```text
Rollback
```

Directly overwriting the user's original file with:

```text
fs.write(config)
```

is forbidden.

Backup files carry a timestamp.

---

# 19. Secret Security

API Keys are highly sensitive data.

Any new code is forbidden from:

```text
console.log(apiKey)
log::info!(api_key)
```

Error messages must not contain the full API Key.

User-directed local endpoint UI (2026-09-20): saved API keys are shown in full
on endpoint cards and prefilled in plain-text edit fields, with explicit Copy
buttons beside keys and URLs. Runtime credential-status messages must not hide
these saved values. This display exception does not permit secrets in logs,
errors, analytics, or automatic clipboard writes. OAuth credentials are not
promoted into editable API keys.

Logs must automatically redact:

```text
Authorization
API Key
Token
Cookie
Secret
```

The product should ultimately access Secrets through a unified:

```text
SecretStore
```

Do not let business modules know where Secrets are actually stored.

---

# 20. Frontend Directory

The new UI is recommended to adopt Feature First.

```text
src/
    app/
        App.tsx
        router.tsx
        providers/

    pages/
        dashboard/
        tools/
        services/
        extensions/
        activity/
        settings/

    features/
        tool-management/
        provider-management/
        health/
        import/
        backup/
        updater/

    entities/
        tool/
        provider/
        extension/
        operation/

    shared/
        ui/
        icons/
        hooks/
        lib/
        constants/
        types/
        i18n/

    native/
        client/
        commands/
        events/
        schemas/
```

---

# 21. Single Entry Point for the Native API

Across the entire React project:

Only:

```text
src/native/
```

is allowed to call:

```text
invoke()
listen()
emit()
```

Forbidden for:

```text
pages/
features/
components/
```

to directly:

```ts
import { invoke } from "@tauri-apps/api/core";
```

All Tauri calls must go through:

```text
NativeClient
```

For example:

```ts
native.tools.list();
native.tools.install(id);
native.providers.list();
native.providers.switch(id);
native.health.scan();
```

This way, renaming a Rust Command later does not affect the UI.

---

# 22. TanStack Query Usage Rules

All data coming from Rust, SQLite, the file system or the network:

Use TanStack Query.

For example:

```text
tools
providers
skills
mcp
versions
usage
health
```

React local state may only hold:

```text
Modal open
Selected tab
Input
Hover
Temporary form state
```

Copying server state into large amounts of:

```text
useState
Context
```

is forbidden.

---

# 23. No Large Global State Library

The MVP does not introduce:

```text
Redux
MobX
Zustand
```

unless approved through an ADR.

Prefer:

```text
TanStack Query
React state
React Context
useReducer
```

This reduces the chance of AI auto-generating multiple state systems.

---

# 24. Product Information Architecture

Default left-hand navigation (ADR-0038, user-approved task-oriented layout):

```text
Home
AI Tools
API Endpoints
Skills
MCP
Global Prompts
Sessions
Settings
```

Chinese version (zh locale labels, glossed in English):

```text
Home
Software
API Endpoints
Skills
MCP
Global Prompts
Sessions
Settings
```

Do not show by default:

```text
Provider
Proxy
Failover
Environment
JSON
TOML
```

Routing and usage stay within API Endpoints. Local configuration, memory,
storage, and the OpenClaw workspace live in the corresponding software's
details. MCP and Global Prompts offer a per-software “Show in file manager”
button; session details offer it per session. Missing files reveal an existing
parent directory without creating or editing anything (ADR-0038).

---

# 25. Progressive Disclosure

The product has no Beginner / Advanced mode switch. Delivered features are always available; users do not need to understand a global mode first.

The first screen shows only the status and primary actions needed for the current task. Low-frequency technical information is collapsed in place on the page it belongs to, for example:

```text
Headers
Multi-route
Runtime context
Storage paths
Health check details
```

Technical terms appear together only when the user expands the corresponding details.

---

# 26. Home Page Design

The home page is the most important page of the product.

Overall visual goal:

**Refined, light, modern, trustworthy, safe.**

It should not look like:

```text
A GitHub tool
System Settings
An IDE
A traditional back-office admin system
```

It should look like a mature commercial macOS App.

---

# 27. Home Page Layout

Top welcome area:

```text
Good afternoon

Your AI coding setup is ready.
```

Chinese version (glossed):

```text
Good afternoon

Your AI coding environment is running normally
```

A large visual status card sits in the center.

Not a mechanical dashboard.

Use a soft animated Orb or circular status visual.

There are only three status levels:

```text
Ready
Needs Attention
Action Required
```

Chinese version (glossed):

```text
Good
Needs Attention
Needs Handling
```

Do not manufacture pseudo-precise health scores such as 83 or 92 points during the MVP phase.

---

# 28. Home Page Core Card

For example:

```text
AI Environment

Ready

3 tools installed
2 services connected
8 extensions active
```

When there are problems:

```text
2 items need attention
```

Button:

```text
Review
```

---

# 29. Quick Actions

The home page provides:

```text
Install AI Tool
Update All
Connect AI Service
Check Setup
```

Chinese version (glossed):

```text
Install AI Tool
Update All
Connect AI Service
Check Environment
```

---

# 30. AI Tools Page

Each AI Tool uses a large standalone Card.

For example:

```text
Claude Code

Installed
2.3.x

Official AI coding agent by Anthropic

Open
Update
...
```

Not installed:

```text
Claude Code

Not Installed

Install
```

Beginners must never be shown installation Shell Commands.

The installation process displays:

```text
Preparing
Downloading
Installing
Checking
Ready
```

Not:

```text
Running npm...
Exit Code 0
```

Technical logs go under:

```text
View Details
```

---

# 31. Installation Interaction

The user clicks:

```text
Install
```

A concise confirmation appears:

```text
Install Claude Code?

AI Manager will install everything required.
```

Buttons:

```text
Install
Cancel
```

After starting, a Task appears.

On completion:

```text
Claude Code is ready
```

Offer:

```text
Open
Done
```

---

# 32. Uninstall Interaction

Uninstalling is a dangerous operation.

It must clearly distinguish:

```text
Uninstall App
Remove Settings
Remove Cache
```

Checked by default only:

```text
Uninstall App
```

User configuration must not be deleted by default.

Display:

```text
Keep your settings
```

enabled by default.

Each of the three options must list, item by item, the real command that will be executed on the current machine or the file / folder paths that will be processed;
an option without an independent path is disabled with a clear explanation, and a vague "delete the configuration directory" must not stand in for the facts.
The preview is for display only; at actual execution time Native must re-detect the installation ownership and paths, and the Renderer must not pass
commands or paths back as uninstall parameters. Automatic deletion may only act on sufficiently specific tool targets within the safety boundary of the user's home directory;
broad top-level root targets such as `~/Documents` and `~/.config` must be rejected even if they come from custom configuration.
Parent symlinks must not be followed out of bounds; when the final target itself is a symlink, only the link is removed and the link target is not traversed.

---

# 33. AI Services Page

Beginner Mode does not use "Provider" as the primary product term.

Page name:

```text
AI Services
```

Chinese version (glossed):

```text
AI Services
```

For example:

```text
Claude
OpenAI
OpenRouter
Custom
```

Button:

```text
Connect
```

Not:

```text
Add Provider
```

---

# 34. Provider Parameter Hierarchy

Adding an API endpoint shows directly by default:

```text
Name
Base URL
API Key
Model
```

Vetted provider presets remain as an optional quick-fill entry point, not a prerequisite step for adding an endpoint. Low-frequency parameters are collapsed in place within the current form:

```text
Headers
Multi-route
Environment Variables
Model Mapping
Timeout
Proxy
```

---

# 35. Current Service Switching

The card clearly shows:

```text
Currently Used by

Claude Code
OpenCode
```

Switch button:

```text
Use
```

After success:

```text
Now active
```

Do not use developer wording such as:

```text
Apply Provider Configuration
```

---

# 36. Extensions Page

Current task-oriented pages (ADR-0038): Skills, MCP and Global Prompts each use
a compact header with title, Help, and supported actions, followed by the software
selector and inventory. Explanations and file-location caveats are accessible in
the collapsed Help disclosure, not repeated as permanent paragraphs above the
list. Add actions belong in the header, not below a long inventory. Save failures,
unsupported scopes and operation errors remain visible.

Top-level page:

```text
Extensions
```

Inside:

```text
Skills
MCP
Prompts
```

Beginner Mode adds explanations.

For example:

### MCP

```text
Connect AI tools to files, browsers and other services.
```

Chinese wording (glossed):

```text
Let AI tools connect to files, browsers and other services.
```

Avoid showing JSON on first entry.

---

# 37. Quick Check

The first version only does lightweight aggregation.

It is not a newly developed complex diagnostics engine.

Generate from existing information:

```text
Claude Code installed
Codex installed
OpenCode update available
2 MCP servers unavailable
1 AI service cannot connect
```

The home page shows:

```text
3 items need attention
```

Click:

```text
Review
```

---

# 38. MVP Health Data Sources

Use only data that is easy to obtain:

```text
Tool installed
Tool version
Update available
Provider selected
Provider connectivity
MCP status
Config parse status
```

The first version does not do:

```text
Full PATH conflict scanning
Multi-version Node governance
System junk scanning
Deep registry cleanup
Automatic WSL repair
Automatic development environment rebuild
```

These belong to P1 or P2.

---

# 39. Operation Manager

Long-running tasks such as install, update and uninstall must not block the UI.

Establish:

```text
OperationManager
```

Each operation has:

```text
operation_id
type
target
status
progress
message
started_at
finished_at
```

Status:

```text
queued
running
success
failed
cancelled
```

The Rust backend pushes progress via Tauri Events.

---

# 40. Task Center

A unified task center exists in the top-right corner or at the bottom of the UI.

For example:

```text
Updating OpenCode
72%
```

Multiple tasks:

```text
2 operations running
```

Recent history is retained after tasks finish.

---

# 41. Mutual Exclusion of Write Operations on the Same Tool

Executing concurrently:

```text
Update Claude
Uninstall Claude
Repair Claude
```

on the same Tool is forbidden.

OperationManager must implement:

```text
per tool mutation lock
```

Different Tools may run concurrently.

---

# 42. Error Model

The frontend is forbidden from directly displaying:

```text
std::io::Error
ENOENT
Exit code 127
thread panicked
```

Unified:

```text
AppError {
    code
    message_key
    technical_message
    remediation
    context_id
}
```

Shown to ordinary users:

```text
Claude Code installation failed.

Try again or view details.
```

Technical details:

```text
View Details
```

---

# 43. Error Codes

Establish stable Error Codes.

For example:

```text
TOOL_NOT_FOUND
INSTALL_FAILED
UPDATE_FAILED
CONFIG_PARSE_FAILED
CONFIG_WRITE_FAILED
PROVIDER_UNREACHABLE
MCP_UNAVAILABLE
PERMISSION_DENIED
NETWORK_ERROR
```

The frontend decides presentation based on the Code.

Branching logic on Error Strings is forbidden.

---

# 44. Logging

A unified logging module.

Levels:

```text
ERROR
WARN
INFO
DEBUG
TRACE
```

Release defaults to INFO.

Debug may enable DEBUG.

Log files rotate automatically.

They must not grow without bound.

Recommended limits:

```text
10 MB per file
5 files
```

Secrets must be redacted.

---

# 45. UI Visual Language

Visual references:

```text
CleanMyMac
Raycast
Arc
Linear
Modern macOS system apps
```

Reference only:

```text
Sense of space
Information hierarchy
Animation rhythm
Degree of simplicity
```

Visual assets must not be copied.

---

# 46. Color Design

Establish Semantic Colors.

Large amounts of the following inside components are forbidden:

```text
#123456
```

Tokens must be used:

```text
--bg-primary
--bg-secondary
--surface
--surface-hover
--text-primary
--text-secondary
--border
--accent
--success
--warning
--danger
```

Support:

```text
Light
Dark
System
```

The first version must get Dark Mode right.

---

# 47. Spacing

A unified 4px base grid.

Primary spacing values:

```text
4
8
12
16
20
24
32
40
48
64
```

Arbitrary values are forbidden:

```text
margin: 13px
padding: 27px
```

---

# 48. Border Radius

Establish:

```text
sm
md
lg
xl
2xl
```

Primary large cards should have a clearly visible radius.

Do not make every component glassy.

Glass effects are used only for:

```text
Hero
Modal
Floating Task Center
```

---

# 49. Shadows

Shadows must be soft.

Do not use the dark, heavy shadows of traditional Windows software.

At most these levels:

```text
shadow-sm
shadow-md
shadow-lg
```

---

# 50. Fonts

Bundling unlicensed fonts is forbidden.

Prefer system fonts.

macOS:

```text
SF Pro system font
```

Windows:

```text
Segoe UI
```

Chinese:

```text
PingFang SC
Microsoft YaHei
```

Fallback uses the system Sans Serif.

---

# 51. Page Width

Recommended default window:

```text
1180 × 760
```

Minimum:

```text
900 × 620
```

The window is resizable.

Maximum width of the main content area:

```text
1400px
```

Do not stretch content indefinitely on ultra-wide screens.

---

# 52. Sidebar

Desktop Sidebar:

```text
220px
```

Collapsible to:

```text
72px
```

Contains:

```text
Logo
Home
AI Tools
AI Services
Extensions
Local Data
Settings
```

Other delivered entries are directly available; the sidebar order places core tasks such as Home, AI Tools and API Endpoints first, followed by Routes, Usage, Sessions and Workspaces.

---

# 53. macOS and Windows Appearance

The main design language stays consistent.

But the conventions of both systems must be respected.

macOS:

```text
Whitespace for the Traffic Light area
Command-key shortcuts
System font
```

Windows:

```text
Normal Minimize
Maximize
Close
Ctrl shortcuts
Segoe UI
```

Do not break basic system interaction conventions for the sake of forced uniformity.

---

# 54. Animation

Goal:

Light, not flashy.

Ordinary Hover:

```text
120ms to 180ms
```

Modal:

```text
180ms to 240ms
```

Page transitions:

```text
around 200ms
```

Large amounts of bouncy spring animations are forbidden.

Installation progress may have a very subtle animated visual.

---

# 55. Performance Rules

Any Shell, Network or File Scan:

must not block the frontend main thread.

All time-consuming Rust operations:

```text
async
```

Large file scans use background Tasks.

A React page must not cause the entire application to re-render because of a single state change.

Large lists use reasonable Memoization.

Do not perform meaningless complex optimization prematurely.

---

# 56. Internationalization

P0:

```text
English
Simplified Chinese
```

Default to Simplified Chinese when the operating system is Chinese.

Other languages default to English.

Later:

```text
Traditional Chinese
Japanese
Korean
```

Existing CC Switch translations may continue to be used when they can be kept at low cost.

---

# 57. Copywriting Principles

Beginner Mode uses the simplest English.

Recommended:

```text
Install
Update
Connect
Remove
Check
Ready
Fix
Open
```

Avoid as much as possible:

```text
Provision
Endpoint
Orchestration
Failover
Runtime
Environment
```

unless the user actively expands the corresponding technical details.

---

# 58. First Launch

Opening the application goes straight to the main interface. There is no
multi-step guide: a welcome screen, a scan animation, and a "how many tools are
installed" summary would only repeat what Home already shows on every launch.

The one question a first launch may ask is the existing-setup import of §17. It
is a dialog over the main interface, it appears only when a compatible setup is
actually found, and skipping it is a complete answer. The same import stays
available in Settings afterwards.

First use does not force the user to register an account for this product.

---

# 59. Local First

MVP:

```text
Local First
```

No cloud account required.

No forced login.

API Keys are not uploaded to a server.

User configuration is not uploaded automatically.

If Cloud Sync is added in the future:

a separate security design is required.

---

# 60. Analytics

The MVP disables sensitive behavior collection by default.

If basic Analytics is added:

Collecting the following is forbidden:

```text
API Key
Prompt
Chat Content
Config Content
File Path
Source Code
```

May be collected anonymously:

```text
App Version
OS
Feature Used
Crash Code
Install Success
Install Failure Error Code
```

with an opt-out provided.

---

# 61. Auto Update

The application itself uses the Tauri Updater.

Update packages must be signed.

macOS Release:

```text
Code Signing
Notarization
```

Windows Release:

```text
Code Signing
```

Phase one distributes via direct download from the official website.

The App Store or Microsoft Store are not MVP-blocking targets for now.

---

# 62. Release Target

First version:

```text
macOS Apple Silicon
macOS Intel
Windows x64
```

Windows ARM can be added later.

Do not block the first version on a Universal macOS Build.

Separate installers may be provided first:

```text
arm64
x64
```

---

# 63. macOS Installer

Preferred:

```text
DMG
```

going through:

```text
Signing
Notarization
Stapling
```

Unsigned official Releases must not be published.

---

# 64. Windows Installer

Support:

```text
NSIS Setup EXE
```

or:

```text
MSI
```

Official Releases must be signed.

Guarantee first:

```text
Double-click install
Automatic shortcut creation
Normal uninstall
Auto update
```

---

# 65. Test Architecture

Each layer is tested independently.

## Frontend

Use:

```text
Vitest
Testing Library
MSW
```

Cover at least:

```text
Tools
Providers
Health
Operation
Error UI
```

## Rust

Cover at least:

```text
Config parser
Config writer
Adapters
Migration
Provider conversion
Backup
Rollback
Version parser
```

---

# 66. File Operation Tests

Testing against the user's real:

```text
~/.claude
~/.codex
```

is forbidden.

Must use:

```text
TempDir
Fixture
```

Configuration file Fixtures:

```text
tests/fixtures/
```

including:

```text
valid
invalid
legacy
empty
corrupted
```

---

# 67. CI Gate

Any code merge must pass:

```text
TypeScript typecheck
ESLint
Format Check
Vitest
Cargo fmt
Cargo clippy
Cargo test
Build
```

Forbidden as the default way to solve problems:

```text
skip test
disable lint
@ts-ignore
```

---

# 68. AI-Generated Code Constraints

This is a mandatory rule set.

## Rule 1

Oversized files must not be created.

Recommended:

React Component:

```text
< 250 lines
```

Hook:

```text
< 200 lines
```

Rust Service:

```text
< 500 lines
```

If a file keeps growing:

split it into modules.

---

# 69. No Catch-All utils

Forbidden:

```text
utils.ts
helpers.ts
common.rs
misc.rs
```

that keep accumulating functionality.

Utility functions must belong to a clear Domain.

For example:

```text
version/
path/
provider/
config/
```

---

# 70. No Component Copy-Paste

If a similar UI appears for the third time:

a Component must be extracted.

For example:

```text
ToolCard
StatusBadge
ActionButton
EmptyState
ProgressTask
SettingRow
```

---

# 71. No Any

New TypeScript code is forbidden from using:

```ts
any;
```

If the type is unknown:

```ts
unknown;
```

then validate via Zod or a Type Guard.

---

# 72. Rust Error Handling

Business execution paths are forbidden from large amounts of:

```rust
unwrap()
expect()
```

Expected errors must return:

```text
Result<T, AppError>
```

Only internal invariants that truly cannot occur may use expect, with the reason documented.

---

# 73. API Response

All Native APIs return a stable structure.

Forbidden: today a

```text
string
```

and tomorrow an

```text
object
```

Recommended:

```text
Result<T>
```

The frontend parses it through the unified Native Client.

---

# 74. Frontend and Backend Types

All important Models:

must have explicit type definitions in both Rust and TypeScript.

Must not rely on:

```text
dynamic JSON object
```

being passed around everywhere.

Boundary data must be validated.

---

# 75. New Dependency Rules

AI must not automatically, for convenience:

```text
npm install xxx
cargo add xxx
```

Any new dependency must first be explained:

```text
Why it is needed
Why existing dependencies cannot solve it
Bundle size impact
Security impact
Maintenance status
```

and only then added.

---

# 76. ADR

Major technical decisions use:

```text
docs/adr/
```

For example:

```text
0001-use-tauri.md
0002-secret-storage.md
0003-upstream-strategy.md
```

Each ADR:

```text
Context
Decision
Alternatives
Consequences
```

---

# 77. Docs

The repository must contain:

```text
docs/
    architecture/
    adr/
    product/
    development/
```

Core:

```text
ARCHITECTURE.md
CONTRIBUTING.md
AI_RULES.md
```

The content of this document may be split up and placed into these files.

---

# 78. AI_RULES.md

The repository root must contain:

```text
AI_RULES.md
```

Any AI must read it before starting to write code.

Core content:

```text
Do not bypass architecture.
Do not call Tauri directly outside src/native.
Do not access SQLite outside repositories.
Do not execute shell commands from React.
Do not add dependencies without approval.
Do not expose secrets in logs.
Do not rewrite upstream code unless necessary.
Prefer adapters over conditional logic.
Add tests for changed behavior.
Keep changes scoped.
```

---

# 79. Feature Boundary

One Feature must not freely access another Feature's internal components.

For example:

```text
features/tools/
```

must not directly:

```text
import "../providers/internal/xxx"
```

Only through the public API.

Each Feature may establish:

```text
index.ts
```

as its public interface.

---

# 80. Data Flow

Standard data flow:

```text
UI
↓
Feature Hook
↓
Native Client
↓
Tauri Command
↓
Application Service
↓
Domain Service
↓
Adapter / Repository
↓
OS / SQLite / Tool Config
```

Forbidden:

```text
UI
↓
File system directly
```

Forbidden:

```text
Component
↓
SQLite directly
```

---

# 81. MVP Page Scope

P0 develops only:

```text
Home
AI Tools
AI Services
Extensions
Settings
```

Do not develop a dozen top-level pages during the MVP phase.

---

# 82. MVP Feature Scope

Must:

```text
Detect Tool
Install Tool
Update Tool
Uninstall Tool

Provider List
Add Provider
Edit Provider
Switch Provider
Test Provider

MCP List
Enable
Disable
Install
Remove

Skills List
Install
Remove

Prompts Basic Management

Quick Check

Backup
Restore

Import CC Switch

Language
Theme

App Update
```

---

# 83. Temporarily Hidden, but Code May Be Kept

If it comes from CC Switch:

```text
Proxy
Failover
Sessions
Advanced Usage
Deep Link
Cloud Sync
```

the Backend may be kept.

Undelivered capabilities are not shown; delivered capabilities are directly usable, with information density controlled by collapsing on their own pages.

---

# 84. P1 Features

Developed after the first release based on user feedback:

```text
PATH Conflict Detection
Duplicate Installation Detection
Broken Config Repair
Cache Cleanup
Residual File Cleanup
Node Environment Check
Package Manager Health
Environment Variable Check
One Click Repair
Migration Between Computers
```

---

# 85. P2 Features

Later:

```text
AI Doctor
Automatic Root Cause Analysis
Full Environment Repair
Cloud Backup
Team Profiles
Shared Provider Configuration
Tool Marketplace
Extension Recommendations
WSL Management
Developer Environment Migration
```

---

# 86. Development Phases

## Phase 0

Goal:

Get the forked project running normally.

Complete:

```text
Clone / Fork CC Switch
Preserve License
Configure upstream
Build macOS
Build Windows
Run existing tests
Record baseline
```

Modifying business logic is forbidden.

Definition of Done:

```text
Original app builds
Tests pass
macOS starts
Windows starts
```

---

# 87. Phase 1

Establish architectural boundaries.

Complete:

```text
Domain Models
NativeClient
Compatibility Layer
ToolAdapter
Platform abstraction
OperationManager skeleton
```

The UI may remain very simple for now.

Goal:

**Establish the entry points of the new architecture first.**

Do not draw pretty pages first.

---

# 88. Phase 2

Complete the Design System.

Establish:

```text
Button
Card
Sidebar
Modal
Toast
Badge
Progress
EmptyState
ToolCard
ServiceCard
SectionHeader
```

Establish:

```text
Typography
Spacing
Radius
Color
Shadow
Motion
```

Complete:

```text
Light Theme
Dark Theme
```

---

# 89. Phase 3

Complete:

```text
Home
AI Tools
```

At this point the product must already be able to:

```text
Detect
Install
Update
Uninstall
```

Claude Code, Codex and OpenCode.

---

# 90. Phase 4

Complete:

```text
AI Services
Provider Switching
Provider Test
```

Prefer reusing existing CC Switch logic.

Do not rewrite the Provider engine.

---

# 91. Phase 5

Complete:

```text
Extensions
MCP
Skills
Prompts
```

Repackage the UI.

---

# 92. Phase 6

Complete:

```text
Quick Check
CC Switch Import
Backup
Restore
Progressive Disclosure
```

---

# 93. Phase 7

Productization:

```text
Installer
Signing
Notarization
Auto Update
Crash Handling
Logging
Final QA
```

---

# 94. AI Per-Phase Execution Requirements

AI is not allowed to complete the entire project in one go after receiving this document.

Complete only one Phase or one Feature at a time.

At the start of each task:

1. Read this document
2. Read AI_RULES.md
3. Read ARCHITECTURE.md
4. Read the current Feature
5. Review existing code
6. State the scope of changes
7. Start development

At the end of a task, output:

```text
Changed Files
Architecture Impact
Tests Added
Tests Passed
Known Limitations
Next Recommended Step
```

---

# 95. Forbidden AI Behavior

Strictly forbidden:

```text
Copying large blocks of code to save effort
Rewriting the entire project without authorization
Deleting existing tests
Turning off TypeScript strict checks
Heavy use of any
Swallowing errors with try catch
Showing Rust errors directly to users
Writing API Keys into logs
Executing Shell directly from the frontend
Components accessing SQLite directly
if/else on different Tools everywhere
Writing a 1000-line component
Creating multiple duplicate State Sources
Adding large numbers of npm packages without approval
Upgrading core dependencies without approval
Modifying the database Schema without approval
Deleting old Migrations without approval
```

---

# 96. AI Modification Principles

If existing code is found to violate the rules:

Do not refactor the entire module in passing.

Only:

```text
Fix the parts involved in the current task
Record the Technical Debt
Propose a separate refactoring task
```

This prevents AI from renovating the entire project every time it writes a small feature.

---

# 97. UI Completion Criteria

No page may be considered complete solely on the basis of:

```text
The feature runs
```

It must also satisfy all of the following states:

```text
Loading
Empty
Success
Warning
Error
Disabled
Hover
Focus
Dark Mode
Chinese
English
```

---

# 98. Accessibility

Interactive controls must support the Keyboard.

Buttons must have accessible Labels.

The following must not be expressed through color alone:

```text
Success
Warning
Error
```

An Icon or Text must accompany them.

---

# 99. Beginner Mode Product Principles

Users should be able to:

```text
Not know npm
Not know PATH
Not know JSON
Not know the MCP configuration format
Not know what a Provider is
```

and still complete the primary operations.

If a primary operation requires an ordinary user to open a Terminal:

the product design has failed.

---

# 100. Progressive Disclosure Product Principles

Users should be neither restricted by a mode gate nor buried under a full screen of technical information.

```text
Custom API
Base URL
Proxy
Failover
Usage
Sessions
Workspace
```

are directly usable once delivered; low-frequency or high-risk information such as Headers, Model Mapping, Environment Variables, Raw Config and Logs
must be implemented within safety boundaries and collapsed by default at the point of use. This forms a product structure of:

```text
Core actions first
Details when needed
```

---

# 101. The Real Competitive Edge of the Product's First Phase

It is not:

```text
More features than CC Switch
```

but rather:

```text
Features are easier to discover
Easier to understand
Easier to install
Easier to complete a correct configuration
Users know what to do after an error
A prettier interface
More like commercial software
Ordinary people dare to use it
```

Therefore, before developing any new feature, first ask:

```text
Is this feature really more important than improving the existing experience?
```

If the answer is no:

do not develop it for now.

---

# 102. MVP Success Criteria

After a new user opens the software for the first time:

without reading documentation.

The goal is to complete within a few minutes:

```text
Discover Claude Code
or install Claude Code

Connect an AI service

See the Ready status

Start using it
```

If completing these things requires:

```text
Opening a Terminal
Looking up a tutorial
Editing JSON
Checking environment variables
```

then the MVP fails.

---

# 103. Final Architecture Goal

The final shape:

```text
┌──────────────────────────────┐
│        Beautiful UI          │
│ Beginner / Advanced          │
└──────────────┬───────────────┘
               │
┌──────────────▼───────────────┐
│      Product Application     │
│ Tools Services Extensions    │
│ Health Operations Backup     │
└──────────────┬───────────────┘
               │
┌──────────────▼───────────────┐
│        Domain Layer          │
│ Tool Provider Extension      │
└───────┬───────────┬──────────┘
        │           │
┌───────▼──────┐ ┌──▼────────────┐
│ Tool Adapter │ │ Repositories  │
└───────┬──────┘ └──┬────────────┘
        │            │
┌───────▼────────────▼───────────┐
│ Compatibility / Infrastructure │
│ CC Switch / SQLite / Config    │
└──────────────┬─────────────────┘
               │
┌──────────────▼───────────────┐
│       Platform Layer         │
│ macOS / Windows / Shell      │
└──────────────────────────────┘
```

---

# 104. Final Instructions for the Executing AI

You are now responsible for developing this project.

Do not immediately start writing code at scale.

First perform the following work:

### Step 1

Inspect the current repository structure, package.json, Cargo.toml, existing tests, Tauri configuration and the CC Switch Backend.

### Step 2

Generate:

```text
docs/ARCHITECTURE.md
docs/AI_RULES.md
docs/adr/0001-upstream-strategy.md
```

The content must comply with this document.

### Step 3

List, in the current code:

```text
Modules that can be reused directly
Modules that need a Wrapper
UI that needs to be redone
Features temporarily hidden
Infrastructure that needs to be added
```

### Step 4

Propose a minimal Phase 1 modification plan.

The scope of changes must be controlled.

### Step 5

Only after completing the above analysis should Phase 1 coding begin.

Developing all pages at once is forbidden.

Refactoring the CC Switch Backend in one go is forbidden.

The goal is:

**Inherit stably first, then establish boundaries, then redo the experience.**

The final product must:

**Feel like a mature commercial PC manager utility, not a developer tool that moves command-line functionality into a window.**
