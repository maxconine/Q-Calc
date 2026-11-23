# Runs dmgbuild without the background bookmark (.DS_Store "pBBk"): macOS 26's Finder follows that bookmark first,
# it points at the scratch volume dmgbuild mounted while building, and the window then shows no background at all.
# Without it Finder resolves the alias ("backgroundImageAlias"), which finds .background.tiff on any mount.
#
#   python build_dmg.py <settings.py> <volume name> <out.dmg> <app> <background.png>
import sys

import dmgbuild
import dmgbuild.core

dmgbuild.core.Bookmark.for_file = staticmethod(lambda path: None)

settings, volume, out, app, background = sys.argv[1:]
dmgbuild.build_dmg(out, volume, settings, defines={"app": app, "background": background})
