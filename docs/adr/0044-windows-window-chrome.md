# ADR-0044: One window surface on Windows too

- Status: accepted
- Date: 2026-09-22
- Extends nothing. Removes `platform::window_chrome`.

## Context

The product window is one continuous coloured field whose tint follows the
active route. On macOS the webview reaches the very top: `titleBarStyle:
"Overlay"` draws the traffic lights inside the client area, and `AppShell`
reserves a draggable strip for them. There is no seam.

Windows kept its native title bar, so the window was a gradient with a flat
slab of unrelated chrome bolted to the top of it. An earlier attempt narrowed
the gap rather than closing it: `platform::window_chrome` handed DWM a fixed
caption colour (`#2a2740`, the canvas base at the time) through
`DWMWA_CAPTION_COLOR`. That was strictly better than the system's near-black
default, but it could only ever be one colour, and by the time the canvas
became a per-route gradient the hard-coded purple-black had drifted into being
the very thing it was meant to fix — a dark bar sharing no hue with anything
under it. Reported as "the Windows build still has a black strip at the top".

A single caption colour cannot track a gradient. Either the top of the window
belongs to the system or it belongs to the product; there is no third option
that looks deliberate.

## Decision

**Windows draws its own caption.** `tauri.windows.conf.json` sets
`decorations: false`, and `features/window-chrome/WindowControls` renders a
32 px bar: a drag region filling the width, and minimize / maximize-restore /
close at the right in the order and at the size Windows uses, so muscle memory
still lands on them. The maximize glyph follows the real window state, which is
re-read on every resize so Aero Snap — which never goes through our button —
stays in sync.

`AppStatusBar` reserves 150 px on its right when running on Windows, so the
privacy line, the update action and the task centre never slide under the close
button.

**`platform::window_chrome` is deleted**, along with both of its call sites and
the `set_theme` follow-up that re-asserted the caption colour. A window with no
caption has nothing for `DWMWA_CAPTION_COLOR` to paint, and keeping a hard-coded
colour for the 1 px Windows 11 border would reintroduce the same mismatch in
miniature. That also removes one `unsafe` FFI block.

**Linux keeps its native decorations.** Drag regions are disabled there
(`DRAG_REGION_ENABLED = !isLinux()`) to work around a Wayland
`gtk_window_begin_move_drag` defect, so an undecorated Linux window would have
no way to be moved. A seam is a worse look; a window you cannot drag is a worse
product.

**The renderer drives the buttons through the native layer.** `minimize`,
`toggleMaximize`, `close` and `isMaximized` live in
`src/native/commands/windowControls.ts`, because only `src/native` may import
`@tauri-apps`; the component sits under `features/`, because the app shell may
not reach the native layer directly. The four matching `core:window:*`
permissions were added to the capability, and the release contract test asserts
both that they are present and that this component is the only thing consuming
them.

## Consequences

- Windows resizing, edge snapping and the Win+Arrow shortcuts keep working:
  wry leaves `WS_THICKFRAME` on undecorated windows, so the system still owns
  the frame's hit-testing.
- The window loses the system caption's right-click menu (Move / Size /
  Maximize). Double-click to maximize and drag-to-snap both survive through the
  drag region.
- Three new strings per locale (`nav.window.*`), which is the cost of drawing
  controls the system used to label.
- The Windows build no longer depends on `Win32_Graphics_Dwm`; the feature stays
  in `Cargo.toml` only if another caller needs it.

## Alternatives rejected

- **Keep DWM colouring and pick a better colour.** This is what already
  shipped. The canvas is a gradient that changes per route; no single caption
  colour matches it, and the one that was chosen aged into the reported bug.
- **`decorations: false` on all three platforms.** Linux cannot afford it, as
  above, and macOS already has a better answer built into the platform.
- **A third-party decoration crate.** Another dependency to carry for three
  buttons and a drag region.
