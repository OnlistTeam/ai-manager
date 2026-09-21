# Spatial model set v5 — real 3D

The home page's terminal is a **real 3D model** now, not a spinning image anymore.

| Asset                     | Purpose                                              |
| ------------------------- | ----------------------------------------------------- |
| `terminal.blend.py`       | Full source of the modeling + lighting + render pipeline (Blender ≥ 5.0) |
| `terminal-turntable.webp` | The 25-frame turntable sprite sheet for the home page hero |
| `terminal.png`            | Single shadowless front frame, source for the app icon |
| `mark.png`                | Sidebar top-left mark, a thumbnail of the front frame |
| `app-icon.png`            | White-background squircle icon source, fed into `tauri icon` |

## Why not a texture, and not Three.js

v4 was an AI-rendered PNG spun with CSS `rotateY`. It could pass for 3D — the
source art had volume, forced perspective, and a shadow displaced off the
rotation plane — but it broke down past a certain angle: **a flat texture has
no side face to reveal**.

There are two ways to get real 3D. Three.js is real-time rendering, at the
cost of a persistent WebGL context and continuous GPU usage (heavier than the
CSS compositor even with `frameloop="demand"`), plus roughly 200KB gzipped of
dependencies. That cost isn't worth paying for a decorative home-page model.

The turntable sequence gets the same result without those costs: every frame
is **actually rendered** from its corresponding yaw angle, so the side walls,
top face, and base top genuinely appear and disappear as the angle changes;
at runtime it's just panning an image, so GPU cost is identical to a static
image with no added heat. It's also smaller: v4's single PNG was 978 KB,
while this set of 25 frames is only 375 KB.

The sequence itself is discrete: 25 frames cover ±26°, 2.2° per frame.
Rounding to the nearest frame would make the object hard-jump by 2.2°
whenever the pointer moves continuously, and that jump, stacked on top of the
smooth translation happening at the same time, reads as "jittering." So
`turntableFrameAt` doesn't round: it holds the two neighboring frames and
hands the fractional part to two sprite layers doing a cross-fade, turning
the discrete sequence back into a continuous angle. Adjacent frames differ by
only 2.2°, a silhouette shift of two or three pixels, invisible even at a
50/50 blend.

The cost is compositing two 2000+px-square textures together: measured under
continuous pointer flicking, the median stays at 16.6 ms, but p95 rises from
17.9 ms (single layer) to 20.3 ms, with about 5% of frames dropping to 40-50
fps. Trading that for continuity is worth it. The blend layer is not
promoted to its own compositing layer — it's mostly near-transparent most of
the time, and pinning another large texture would only eat into compositing
resources; measured, that halved the number of frames that timed out. Note
this requires explicitly writing `will-change: auto`: with both classes on
the same element, the base class's `will-change: transform` would otherwise
still match — just omitting it from this rule doesn't stop that.

Pitch (±12°) is still handled by deforming the sprite with CSS `rotateX`;
the planar-approximation error is invisible at this small an angle.

Panning uses `transform` rather than `background-position`: the latter has
to re-rasterize the whole 2000+px-square image on every change, so a pointer
move becomes continuous repainting with visibly noticeable jank; switching to
`transform`, 117 frames of continuous pointer flicking measured a median of
16.6 ms, p95 of 17.9 ms, and zero dropped frames.

The sequence is rendered as the **camera orbiting the object** by yaw, but
the interaction wants the **object turning to face the cursor** — the two
directions are opposite. With the cursor on the right, the object should
turn its front to the right, meaning what we see is its left side, i.e. the
frame where the camera is on the left. The negation inside
`turntableFrameAt` is what makes this work; without it the model would
visibly turn away from the mouse.

## Reproducing

```bash
B=/Applications/Blender.app/Contents/MacOS/Blender
V5=src/assets/spatial/models/v5

# Turntable, 25 frames (must be noshadow — see why below)
$B -b -P $V5/terminal.blend.py -- /tmp/frames -26 26 25 512 noshadow
python3 $V5/pack-turntable.py /tmp/frames $V5/terminal-turntable.webp 5 5 512

# Single shadowless front frame → terminal.png / mark.png
$B -b -P $V5/terminal.blend.py -- /tmp/front 0 0 1 1100 noshadow

# App icon
python3 $V5/make-app-icon.py
pnpm tauri icon $V5/app-icon.png --ios-color "#FFFFFF"
```

The biggest difference from v3/v4: **this asset set is fully reproducible**.
Earlier generations were produced by `gpt-image-2`, which gives a different
result every time, so the original renders (with their backdrop) had to be
kept in the repo. Now, changing the aspect ratio, the color scheme, adding
frames, or changing the angle is just a matter of editing the script and
rerunning it.

## A few pitfalls hit along the way (all documented in the script comments too)

- `transform_apply(scale=True)` also defaults `location`/`rotation` to
  **True**. If you only meant to bake scale but wrote it this way, position
  gets baked into the mesh too and the object's origin is reset to zero;
  setting `rotation_euler` afterward then rotates around the world origin
  instead, flinging parts off-screen.
- Blender's Base Color takes **linear** values. Feeding it sRGB numbers
  directly makes everything look brighter and washed out: ivory turns pure
  white, mint green fades to pale green.
- The view transform must be `Standard`. The default AgX desaturates this
  color scheme into gray-green.
- The screen can't just be "placed inside the body" — a solid body would
  hide it entirely. The front panel needs a real recess cut with a boolean
  operation, with the screen sitting at the bottom of the recess and a
  visible rim of inner wall around it, to get the frame's inner shadow.
- The average color difference within adjacent frames is about 2.6/255.
  That is **not** sampling noise — raising samples from 96 to 512 and
  locking the random seed left that number completely unchanged. It's the
  real shading change from the object turning 2.2°.
- **Turntable frames must be rendered with noshadow.** Cycles' shadow
  catcher, under `film_transparent`, outputs **pure black** plus alpha,
  which becomes a solid black blob over any background color instead of
  tinting with the route color the way a CSS shadow does; it also bleeds all
  the way to the edge of the frame, and after being packed into the sheet
  and resampled, that leaks into neighboring cells, showing up on the page
  as an unexplained gray fringe around the model. The contact shadow is
  therefore left to CSS's `.spatial-scene__floor`.
- The sprite sheet needs a sampling safety margin between cells (the
  `GUTTER` in `pack-turntable.py`), otherwise edge pixels bleed into
  neighboring cells when scaled.
- The camera needs to be pulled back far enough to avoid clipping the object
  at extreme angles, which leaves large transparent margins around the front
  frame. That margin must be cropped uniformly at packing time: it wastes
  space, and it also makes the model look smaller on the page than it
  should — CSS can only place things by cell size and has no idea how much
  of a cell is empty. The crop window is the union of all frames; cropping
  each frame individually would make the object jitter between frames.

## App icon

The base plate is white rather than the brand's deep green: the dock is a
row of icons side by side, and a solid dark plate would shrink into a single
color block competing for attention with its neighbors. The shape uses a
superellipse (squircle) rather than a rounded rectangle — that's what
macOS's own icon mask uses. `--ios-color` is required; iOS icons don't allow
transparency, and without specifying it the background gets filled white.

The three ratio constants in `make-app-icon.py` were measured against a
local reference, `/Applications/CleanMyMac_5_MAS.app`'s `AppIcon.icns`
(extracted with `iconutil -c iconset`, the 256px tier — the ratio is
consistent across tiers). Method: take the bounding box where `alpha>0.5` as
the plate bbox, inset it by 4.5% to exclude the squircle's own
anti-aliasing edge, then take the bounding box of pixels with "distance from
white > 30" as the subject bbox.

|                          | CleanMyMac                    | Ours                          |
| ------------------------ | ------------------------------ | ------------------------------ |
| Plate / canvas           | 0.8047                        | 0.8047                        |
| Subject width / plate    | 0.840                         | 0.790                         |
| Ink area / plate         | 0.4245                        | 0.5496                        |
| Plate top → bottom       | (249,247,252) → (254,254,255) | (247,250,248) → (255,255,255) |

Two places deliberately not copied verbatim:

- **Width ratio is 0.790, not their 0.840.** Their display is wide and flat
  (aspect ratio 1.33); ours is nearly square (1.03). Copying their width
  ratio would push the ink area to 1.43x theirs, with the base crowding the
  squircle's corner taper; matching ink area instead would give 0.70,
  smaller than the original — a square shape naturally covers more area
  than a flat one to begin with. Where the two criteria conflict, side-by-side
  visual comparison wins.
- **The top color tint uses mint rather than their lavender.** Their tint
  follows their subject's purple body; by the same logic, ours follows our
  own machine's body color. It's about 5/255 — not distinguishable as a
  color on its own, just enough to keep the plate from reading as flat dead
  white.

The plate gradient's **direction** is counterintuitive, and was only found
by measuring it: it gets brighter going down, a total spread of only 6/255
over the whole gradient, purely vertical with no diagonal component.
Previously ours went the other way (white at top, (236,237,240) at bottom,
a 16/255 spread) — that 16 is exactly why the plate read as "off-white" in
the dock instead of white.

`src-tauri/icons/tray/macos/statusbar_template_3x.png` is **not part of this
pipeline**. The macOS menu bar icon must be a monochrome template image,
recolored by the system for light/dark mode.

The asset is decorative in the UI: the page title and status copy remain the
sole source of truth for accessibility.
