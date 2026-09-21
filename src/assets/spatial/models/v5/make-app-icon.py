"""Compose a 1024-square app icon source from terminal.png.

The app icon has to be the same object as the one shown in the UI, so it's
not drawn separately — it's the same render used for the hero, placed on a
base plate. The plate uses a superellipse (squircle) rather than an ordinary
rounded rectangle: macOS's own icon mask is a superellipse, and a shape built
from border-radius arcs stitched to straight edges reads half a pixel off
from the neighboring system icons in the dock — "not quite right" without
being able to say why.

    python3 src/assets/spatial/models/v5/make-app-icon.py

The output, app-icon.png, is fed to `pnpm tauri icon` to generate every
platform size.

The constants below were measured, not estimated. The reference is the local
CleanMyMac AppIcon.icns (the 256px tier — the ratio is consistent across
tiers). Method: take the bounding box where alpha>0.5 as the plate bbox,
inset it 4.5% to exclude the squircle's own anti-aliasing edge, then take the
bounding box of pixels with "distance from white > 30" as the subject bbox.
"""

import pathlib

import numpy as np
from PIL import Image, ImageFilter

HERE = pathlib.Path(__file__).parent
SIDE = 1024

# The plate covers 80.47% of the canvas, matching CleanMyMac's measured
# value exactly — the margin around it is part of the macOS icon spec; the
# dock relies on that margin for breathing room between adjacent icons.
PLATE = 824
SQUIRCLE_N = 5.0
SUPERSAMPLE = 4

# Subject width as a fraction of the plate. CleanMyMac uses 0.840, but its
# display is wide and flat (aspect ratio 1.33) while ours is nearly square
# (1.03) — copying its width ratio would push our actual ink area to 1.43x
# theirs, with the base crowding right up against the squircle's corner
# taper. Matching ink area instead gives 0.70, smaller than this, because a
# square shape naturally covers more area than a flat one to begin with.
# Where the two criteria conflict, side-by-side visual comparison wins: at
# 0.790 the two read as visually equivalent in weight, the horizontal margin
# is 10.5% (theirs is 8%), and the base still has room before the plate edge.
OBJECT_WIDTH = 0.790

# How far the subject's center sits below the canvas center, as a fraction —
# CleanMyMac measures +0.0156. Nudging a bottom-heavy object down slightly
# makes the top/bottom margins land around a 15%/11% ratio, which reads as
# "resting on the plate" rather than "floating in the middle of it."
OBJECT_DROP = 0.0156


def build_plate() -> Image.Image:
    """A near-white base plate, brightening very slightly from top to bottom.

    The plate is white rather than the brand's deep green: the dock is a row
    of icons side by side, and a solid dark plate against the dock's light
    background would shrink into a single color block competing for
    attention with its neighbors. A white plate hands attention back to the
    object itself.

    The gradient direction is counterintuitive and was only found by
    measuring CleanMyMac: its top is (249,247,252), bottom (254,254,255) —
    it gets brighter going down, a total spread of only 6/255 over the whole
    gradient, purely vertical with no diagonal component. Ours previously
    went the other way (white at top, (236,237,240) at bottom), a 16/255
    spread — that 16 is exactly why the plate read as "off-white/beige" in
    the dock instead of white.

    The slight tint at the top is lavender in CleanMyMac's case, because its
    subject is purple; by the same logic ours uses mint, so the plate reads
    as belonging to our own machine. At this magnitude (about 5/255) it's
    not distinguishable as a color on its own — its only job is to keep the
    plate from reading as flat dead white.
    """
    n = PLATE * SUPERSAMPLE
    y, x = np.mgrid[0:n, 0:n] / (n - 1)
    top, bottom = (0.969, 0.982, 0.973), (1.0, 1.0, 1.0)
    rgb = np.zeros((n, n, 3), dtype=np.float32)
    for i in range(3):
        rgb[..., i] = top[i] + (bottom[i] - top[i]) * y

    u, v = (x - 0.5) * 2, (y - 0.5) * 2
    mask = (np.abs(u) ** SQUIRCLE_N + np.abs(v) ** SQUIRCLE_N) <= 1.0
    rgba = np.dstack([rgb, mask.astype(np.float32)])
    plate = Image.fromarray((rgba * 255).round().astype(np.uint8), "RGBA")
    return plate.resize((PLATE, PLATE), Image.LANCZOS)


def object_bbox(alpha: np.ndarray) -> tuple[int, int, int, int]:
    """The source art is tightly cropped as a square, but the object inside
    it neither fills the frame nor sits centered.

    Counts opaque pixels per row/column rather than using getbbox(): the
    rendered alpha edge has scattered noise pixels, and a single stray pixel
    can stretch the bbox to the whole canvas, throwing off every ratio-based
    placement that follows.
    """
    rows = (alpha > 40).sum(axis=1)
    cols = (alpha > 40).sum(axis=0)
    (ys,) = np.nonzero(rows >= 4)
    (xs,) = np.nonzero(cols >= 4)
    return xs.min(), ys.min(), xs.max(), ys.max()


def main() -> None:
    src = Image.open(HERE / "terminal.png").convert("RGBA")
    x0, y0, x1, y1 = object_bbox(np.asarray(src)[..., 3])

    scale = (PLATE * OBJECT_WIDTH) / (x1 - x0 + 1)
    obj = src.resize(
        (round(src.width * scale), round(src.height * scale)), Image.LANCZOS
    )
    # Positioned by the object's own bbox center, not the source canvas
    # center — the two differ by a whole ring of margin.
    x = round(SIDE / 2 - (x0 + x1) / 2 * scale)
    y = round(SIDE / 2 + OBJECT_DROP * SIDE - (y0 + y1) / 2 * scale)

    canvas = Image.new("RGBA", (SIDE, SIDE), (0, 0, 0, 0))
    canvas.alpha_composite(build_plate(), ((SIDE - PLATE) // 2,) * 2)

    contact = Image.new("RGBA", (SIDE, SIDE), (0, 0, 0, 0))
    contact.alpha_composite(obj, (x, y + round(PLATE * OBJECT_WIDTH * 0.030)))
    contact = Image.fromarray(
        np.dstack(
            [
                np.zeros((SIDE, SIDE, 3), np.uint8),
                (np.asarray(contact)[..., 3] * 0.22).astype(np.uint8),
            ]
        ),
        "RGBA",
    ).filter(ImageFilter.GaussianBlur(11))

    # The shadow may only fall on the plate, not smear into the transparent
    # margin outside it.
    plate_alpha = np.asarray(canvas)[..., 3].astype(np.float32) / 255.0
    shadow = np.asarray(contact).copy()
    shadow[..., 3] = (shadow[..., 3] * plate_alpha).astype(np.uint8)
    canvas.alpha_composite(Image.fromarray(shadow, "RGBA"))
    canvas.alpha_composite(obj, (x, y))

    out = HERE / "app-icon.png"
    canvas.save(out, optimize=True)
    print(f"{out}: {canvas.size}")


if __name__ == "__main__":
    main()
