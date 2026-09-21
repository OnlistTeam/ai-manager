"""Pack Blender-rendered turntable frames into a sprite sheet.

    python3 src/assets/spatial/models/v5/pack-turntable.py <frames_dir> <out.webp> [cols] [rows]

Frames must be rendered with noshadow. A shadowed version has two problems:
Cycles' shadow catcher, under film_transparent, outputs **pure black** plus
alpha, which becomes a solid black blob over any background color instead of
tinting with the route color the way a CSS shadow does; and the shadow
bleeds all the way to the edge of the frame, so after packing into the
sheet, resampling leaks it into neighboring cells — showing up on the page
as an unexplained gray fringe around the model.

The contact shadow is therefore left to CSS's .spatial-scene__floor to draw.
"""

import glob
import os
import sys

import numpy as np
from PIL import Image

# Sampling safety margin: edge pixels bleed into neighboring cells when
# scaled, so leave a ring of empty pixels to block that.
GUTTER = 6


def union_bbox(ims):
    """All frames share one crop window.

    Cropping each frame individually would make the object jitter between
    frames — each frame's bounding box is naturally a different width, at
    its narrowest when turned to the side. Taking the union keeps the
    object's position within the cell stable.
    """
    lo_x = lo_y = 10**9
    hi_x = hi_y = 0
    for im in ims:
        a = np.asarray(im)[..., 3]
        rows, cols = (a > 24).sum(axis=1), (a > 24).sum(axis=0)
        ys, xs = np.nonzero(rows >= 3)[0], np.nonzero(cols >= 3)[0]
        lo_x, hi_x = min(lo_x, int(xs.min())), max(hi_x, int(xs.max()) + 1)
        lo_y, hi_y = min(lo_y, int(ys.min())), max(hi_y, int(ys.max()) + 1)
    return lo_x, lo_y, hi_x, hi_y


def main(src_dir, out_path, cols=5, rows=5, size=512):
    files = sorted(glob.glob(os.path.join(src_dir, "f*.png")))
    ims = [Image.open(f).convert("RGBA") for f in files]
    assert len(ims) <= cols * rows, f"{len(ims)} frames don't fit in {cols}x{rows}"

    # The camera has to be pulled back far enough to avoid clipping the
    # object at extreme turn angles, which leaves large transparent margins
    # around the front frame. Keeping that margin in the sheet wastes space
    # twice over: it costs bytes, and it also makes the model look smaller
    # on the page than it should — CSS can only place things by cell size
    # and has no idea how much of a cell is empty.
    lo_x, lo_y, hi_x, hi_y = union_bbox(ims)
    side = int(max(hi_x - lo_x, hi_y - lo_y) * 1.08)
    cx, cy = (lo_x + hi_x) // 2, (lo_y + hi_y) // 2
    win = (cx - side // 2, cy - side // 2, cx - side // 2 + side, cy - side // 2 + side)
    ims = [im.crop(win).resize((size, size), Image.LANCZOS) for im in ims]
    n = size

    cell = n + GUTTER * 2
    sheet = Image.new("RGBA", (cols * cell, rows * cell), (0, 0, 0, 0))
    for i, im in enumerate(ims):
        x = (i % cols) * cell + GUTTER
        y = (i // cols) * cell + GUTTER
        sheet.paste(im, (x, y))

    sheet.save(out_path, "WEBP", quality=86, method=6)

    a = np.asarray(sheet)[..., 3]
    worst = 0
    for r in range(rows):
        for c in range(cols):
            e = a[r * cell:(r + 1) * cell, c * cell:(c + 1) * cell]
            worst = max(worst, e[0].max(), e[-1].max(), e[:, 0].max(), e[:, -1].max())
    print(f"{out_path}: {sheet.size} | {len(ims)} frames {cols}x{rows} | cell {cell} "
          f"(content {n} + margin {GUTTER}) | edge alpha {worst} | "
          f"{os.path.getsize(out_path)/1024:.0f} KB")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2],
         int(sys.argv[3]) if len(sys.argv) > 3 else 5,
         int(sys.argv[4]) if len(sys.argv) > 4 else 5,
         int(sys.argv[5]) if len(sys.argv) > 5 else 512)
