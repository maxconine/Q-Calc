# dmgbuild settings for the Q Calc install window (make-dmg.sh passes app, volume and background with -D).
# Finder's layout is written straight into the image's .DS_Store, so no Finder scripting is needed on CI.
# The window and icon positions match background.png, drawn by background.swift: change them together.
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
app_name = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
hide_extension = [app_name]

background = defines["background"]  # noqa: F821; its @2x sibling becomes a retina TIFF
window_rect = ((200, 140), (640, 450))  # 450 tall: Finder's title bar and bottom bar take 50, leaving the 640 x 400 background
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False
arrange_by = None
icon_size = 128
text_size = 13
icon_locations = {
    app_name: (160, 210),
    "Applications": (480, 210),
}
