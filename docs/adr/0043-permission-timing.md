# ADR-0043: Asking for a macOS permission only when there is something to do with it

- Status: accepted
- Date: 2026-09-22
- Supersedes nothing.

## Context

A fresh install showed this on first launch, before the user had touched
anything:

> "AI Manager.app" wants to control "System Events.app". Allowing control will
> provide access to documents and data in "System Events.app", and to perform
> actions within that app.

The subtitle underneath it came from our own `Info.plist`: "AI Manager uses
Terminal only when you choose Open." The app was therefore asking for an
automation grant while telling the user it would only need one later. That is
the worst possible first impression for a product whose pitch is that your
setup stays on your computer, and the sentence was not even describing the
request being made.

The request did not come from the terminal bridge. `auto-launch` 0.5 drives
launch-at-login on macOS through Finder's login items, and every operation —
`enable`, `disable`, **and `is_enabled`** — runs
`tell application "System Events" to …` (`macos.rs:196`). Rendering the
settings page reads the current state of the toggle, so simply *looking at* the
switch triggered the grant for a feature nobody had turned on.

A second, separate problem sat behind it. macOS records a refused automation
grant permanently: after "Don't Allow" there is no second prompt, and every
later attempt fails with `errAEEventNotPermitted` (-1743). Our terminal bridge
reported that as the generic "The tool could not open in Terminal", with the
remediation "Open a terminal in your project folder and start the tool there" —
true, but it leaves a button on screen that can never work again and no hint
why.

## Decision

**Launch at login uses a launch agent, not a login item.** `auto-launch`
already supports this (`set_use_launch_agent(true)`): enabling writes one plist
under `~/Library/LaunchAgents`, disabling removes it, and reading the state is
a file existence check. No AppleScript, no grant, no prompt, and the settings
page renders instantly instead of waiting on a subprocess.

Two details this requires:

- The agent's `Label` is the bundle identifier (`tools.aimanager.desktop`)
  rather than the display name, so the file is recognisable in
  `~/Library/LaunchAgents` and valid for `launchctl`.
- The path stays `current_exe()` — the real binary inside the bundle. `launchd`
  execs the path it is given and cannot exec a `.app` directory. The bundle's
  `Info.plist` still applies, so the window and Dock tile behave exactly as
  they do when the bundle is opened from Finder. The old code deliberately
  rewrote the path *to* the `.app`, which was correct for a login item and is
  wrong here; a test pins the direction so it cannot drift back.

**A refused terminal grant gets its own error and a button.** `osascript`'s
stderr is matched for the three spellings macOS uses (`-1743`,
`errAEEventNotPermitted`, "Not authorized to send Apple events"), and that case
returns `error.tool.terminalAutomationDenied` with the remediation
`error.remediation.allowTerminalAutomation`. `ToolActionError` renders an
"Open System Settings" button whenever it sees that remediation key, and the
destination
(`x-apple.systempreferences:com.apple.preference.security?Privacy_Automation`)
is fixed in the native side — the renderer names the intent, never an address,
the same rule as the About panel's legal notice links.

**Opening the home page reaches nothing off this machine.** The same
complaint named the start-up check, and it turned out to be fair for a reason
that was not obvious: `useToolInventory` pairs a local read with
`app_tools_check_versions`, which queries every tool's registry. So opening the
app sent a handful of outbound requests before the user had done anything —
from a product whose one-line promise is that your setup stays on this
computer. The local list is unchanged; the version lookup is now opted out of
on the home page and runs when "Check again" is pressed. The Software page
keeps checking on arrival, because looking for updates is why someone opens
it.

**No permanent "permissions" switch in Settings.** The grant is only needed for
one action, the failure is self-announcing, and the repair button appears
exactly where the failure does. A standing row would add a control the user has
to reason about on every visit to pay for a problem most users will never have.

## Consequences

- The first launch of a fresh install asks for nothing. The only grant this
  product ever requests is the terminal one, and only after the user clicks
  Open.
- A login item created by an earlier build's AppleScript path is **not**
  removed, because removing it would require the very permission this change
  exists to avoid. Someone who enabled the switch before this change has both a
  stale login item and a launch agent; the single-instance guard makes the
  duplicate launch harmless, and turning the switch off in this build removes
  the agent but leaves the login item. Given 0.2.0 was the first stable
  release, the affected population is close to nobody, and the alternative is
  reintroducing the prompt for everyone.
- `preferences.desktop.launch.descriptionMac` no longer says "login item",
  because it is not one. It now describes the effect rather than the mechanism,
  which is what that line should have said in the first place.

## Alternatives rejected

- **`SMAppService`** (macOS 13+) is Apple's current API and needs no grant
  either, but reaching it means Objective-C bindings and a second code path for
  macOS 12. A launch agent is what `SMAppService.agent` registers anyway.
- **Deferring the read until the user opens Settings.** The prompt would move
  rather than disappear, and the toggle still could not render its own state
  without asking.
- **Dropping launch-at-login on macOS.** It is a reasonable feature; the
  mechanism was the problem, not the feature.
