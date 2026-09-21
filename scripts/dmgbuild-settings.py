"""Headless, deterministic Finder layout for the AI Manager release DMG."""

from pathlib import Path


application_path = Path(defines["app"]).resolve()  # type: ignore[name-defined]
background_path = Path(defines["background"]).resolve()  # type: ignore[name-defined]
volume_icon_path = Path(defines["icon"]).resolve()  # type: ignore[name-defined]

if application_path.suffix != ".app" or not application_path.is_dir():
    raise RuntimeError(f"Expected a macOS application bundle: {application_path}")
for label, path in (
    ("background", background_path),
    ("volume icon", volume_icon_path),
):
    if not path.is_file():
        raise RuntimeError(f"Missing DMG {label}: {path}")

application = str(application_path)
background = str(background_path)
icon = str(volume_icon_path)
app_name = application_path.name

format = "UDZO"
filesystem = "HFS+"
compression_level = 9

files = [application]
symlinks = {"Applications": "/Applications"}
# Do not ask dmgbuild to hide the app extension. That operation writes a
# com.apple.FinderInfo xattr onto the signed bundle, which makes a mounted DMG
# copy fail strict codesign verification. Finder already presents application
# bundles by their display name.
hide_extensions = []

window_rect = ((200, 120), (660, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = True

icon_size = 80
text_size = 14
icon_locations = {
    app_name: (180, 220),
    "Applications": (480, 220),
}
