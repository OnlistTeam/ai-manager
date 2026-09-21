#!/usr/bin/env python3
"""Generate deterministic AI Manager tray, About, and DMG companion assets."""

from pathlib import Path
from shutil import copyfile

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/assets/spatial/models/v5/app-icon.png"
ICONS = ROOT / "src-tauri/icons"
WARM_WHITE = "#FAFAF8"
EVERGREEN = "#2F6D5A"
MUTED = "#6B7070"


def load_font(size: int, *, bold: bool = False) -> ImageFont.FreeTypeFont:
    candidates = [
        Path("/System/Library/Fonts/SFNS.ttf"),
        Path("/System/Library/Fonts/Supplemental/Arial Bold.ttf")
        if bold
        else Path("/System/Library/Fonts/Supplemental/Arial.ttf"),
        Path("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
        if bold
        else Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ]
    for candidate in candidates:
        if candidate.exists():
            return ImageFont.truetype(candidate, size)
    raise RuntimeError("No supported SF, Arial, or DejaVu font was found")


def generate_dmg_background(source: Image.Image) -> None:
    canvas_2x = Image.new("RGB", (1320, 800), WARM_WHITE)
    draw = ImageDraw.Draw(canvas_2x)

    mark = source.convert("RGBA").resize((176, 176), Image.Resampling.LANCZOS)
    canvas_2x.paste(mark, ((canvas_2x.width - mark.width) // 2, 20), mark)

    draw.text(
        (canvas_2x.width // 2, 205),
        "AI Manager",
        fill=EVERGREEN,
        font=load_font(48, bold=True),
        anchor="mm",
    )

    # dmgbuild places the app and Applications icons at logical coordinates
    # (180, 220) and (480, 220); this canvas is the 2x representation.
    arrow_y = 440
    draw.line((520, arrow_y, 795, arrow_y), fill=EVERGREEN, width=8)
    draw.polygon(
        ((795, arrow_y - 25), (835, arrow_y), (795, arrow_y + 25)),
        fill=EVERGREEN,
    )
    draw.text(
        (canvas_2x.width // 2, 635),
        "Drag AI Manager to Applications",
        fill=MUTED,
        font=load_font(30),
        anchor="mm",
    )
    canvas_2x.save(ICONS / "dmg-background@2x.png", format="PNG", optimize=True)
    canvas_2x.resize((660, 400), Image.Resampling.LANCZOS).save(
        ICONS / "dmg-background.png",
        format="PNG",
        optimize=True,
    )


def generate_tray_template(source: Image.Image) -> None:
    rgb = source.convert("RGB")
    mask = Image.new("L", rgb.size)
    # The source has a warm neutral field and a green mark. Green dominance
    # extracts the mark while preserving antialiased edges and rejecting the
    # generated background texture.
    pixels = (
        rgb.get_flattened_data() if hasattr(rgb, "get_flattened_data") else rgb.getdata()
    )
    mask.putdata(
        [max(0, min(255, (green - red - 5) * 6)) for red, green, _blue in pixels]
    )
    bounds = mask.getbbox()
    if bounds is None:
        raise RuntimeError("The source icon contains no detectable evergreen mark")

    cropped = mask.crop(bounds)
    cropped.thumbnail((56, 56), Image.Resampling.LANCZOS)
    alpha = Image.new("L", (72, 72))
    alpha.paste(cropped, ((72 - cropped.width) // 2, (72 - cropped.height) // 2))

    tray = Image.new("RGBA", (72, 72), (0, 0, 0, 0))
    tray.putalpha(alpha)
    tray.save(
        ICONS / "tray/macos/statusbar_template_3x.png",
        format="PNG",
        optimize=True,
    )


def main() -> None:
    source = Image.open(SOURCE)
    if source.size != (1024, 1024):
        raise RuntimeError(f"Expected a 1024x1024 source icon, got {source.size}")

    generated_icon = ICONS / "32x32.png"
    if not generated_icon.exists():
        raise RuntimeError("Run `pnpm tauri icon` before generating companion assets")

    generate_dmg_background(source)
    generate_tray_template(source)
    copyfile(generated_icon, ROOT / "src/assets/icons/app-icon.png")


if __name__ == "__main__":
    main()
