#!/usr/bin/env python3
"""Build the Redrob Recall app icon master, then hand it to `tauri icon` for the platform sizes.

Geometry and rules are assets/Products/README.md, not invention:

  Tile        square top-left corner, 22% radius on the other three from 48px up.
  Symbol      the shipped file, Solid White, on every tile. Never redrawn or recoloured.
  Doorway     "Redrob's doorway is empty. Something in the doorway means a product." Recall's is a
              lens: 45-icons.md keeps a ring that IS the thing closed, so the lens is a closed ring.
  Placement   glyph ink 12% of the door's width in from each side, centred on the gate's waist, no
              taller than 64% of the door.
  Tile colour Desk's band. 20-color.md closes the spectrum at seven and states there is no eighth
              product colour; 10-logo.md's way out is that a new product ships as a feature of the
              product whose band it fits, and Recall's job -- your own files on your own machine --
              is Desk's. Pinned in DESIGN_SYSTEM_PIN.json.

The symbol path is LIFTED from a shipped product icon rather than retyped, so it cannot drift.
"""
import json
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
DELIVERY = pathlib.Path.home() / "workplace/redrob-design-system/assets/Products"
MASTER = REPO / "src-tauri/icons/app-icon.svg"
PNG = REPO / "src-tauri/icons/app-icon.png"

pin = json.loads((REPO / "DESIGN_SYSTEM_PIN.json").read_text())
TILE_COLOUR = pin["product_band"]["tile"]

# 22% of 512 = 112.64.
TILE = ("M0 0H399.36A112.64 112.64 0 0 1 512 112.64V399.36A112.64 112.64 0 0 1 399.36 512"
        "H112.64A112.64 112.64 0 0 1 0 399.36Z")

source = DELIVERY / "redrob-desk-icon.svg"
if not source.exists():
    sys.exit(f"design system delivery not found: {source}")
symbol = re.search(r'(<path d="M54\.4[^/]*?/>)', source.read_text())
if not symbol:
    sys.exit(f"could not lift the symbol path out of {source.name}")

# The doorway, measured off the shipped masters rather than guessed: Desk's folder occupies
# x 212..300, y 207..314, and every shipped glyph is FILLED white with no stroke. So the lens is a
# filled annulus, not a stroked circle -- a stroked ring is a different construction from the set.
#
# 45-icons.md: a ring that IS the thing stays closed, so the lens does not open; and its handle leaves
# the ring on the 40 degree rake, which is the angle the whole set is drawn on.
def lens() -> str:
    import math

    cx, cy, outer, inner = 252.0, 250.0, 32.0, 20.0
    rake = math.radians(40.0)
    dx, dy = math.sin(rake), math.cos(rake)
    # Start the handle inside the ring wall so the two read as one object, and stop inside the door.
    sx, sy = cx + dx * (outer - 6), cy + dy * (outer - 6)
    ex, ey = cx + dx * 66.0, cy + dy * 66.0
    half = 7.0
    px, py = dy * half, -dx * half  # normal to the handle
    ring = (
        f"M{cx - outer} {cy}A{outer} {outer} 0 1 0 {cx + outer} {cy}"
        f"A{outer} {outer} 0 1 0 {cx - outer} {cy}Z"
        f"M{cx - inner} {cy}A{inner} {inner} 0 1 1 {cx + inner} {cy}"
        f"A{inner} {inner} 0 1 1 {cx - inner} {cy}Z"
    )
    handle = (
        f"M{sx + px:.2f} {sy + py:.2f}L{ex + px:.2f} {ey + py:.2f}"
        f"L{ex - px:.2f} {ey - py:.2f}L{sx - px:.2f} {sy - py:.2f}Z"
    )
    return f'<path d="{ring}{handle}" fill="#FFFFFF" fill-rule="evenodd"/>'


LENS = lens()

MASTER.write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"'
    ' role="img" aria-label="Redrob Recall"><title>Redrob Recall</title>'
    f'<path d="{TILE}" fill="{TILE_COLOUR}"/>{symbol.group(1)}{LENS}</svg>\n'
)
print(f"wrote {MASTER.relative_to(REPO)}  tile {TILE_COLOUR}")

# assets/Products/README.md: "-32.svg and -16.svg carry tile and symbol only (the doorway is two or
# three pixels wide there); at 16 and 32 the tile color tells products apart." Rendering the full
# master down to 32 turns the lens into a smudge over the symbol, so the small sizes come from their
# own master. 19% radius at 32px, square at 16px -- also from the README.
TILE_32 = ("M0 0H414.72A97.28 97.28 0 0 1 512 97.28V414.72A97.28 97.28 0 0 1 414.72 512"
           "H97.28A97.28 97.28 0 0 1 0 414.72Z")
SMALL = REPO / "src-tauri/icons/app-icon-small.svg"
SMALL.write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"'
    ' role="img" aria-label="Redrob Recall"><title>Redrob Recall</title>'
    f'<path d="{TILE_32}" fill="{TILE_COLOUR}"/>{symbol.group(1)}</svg>\n'
)
print(f"wrote {SMALL.relative_to(REPO)}  tile and symbol only, for 32px and below")

subprocess.run(
    ["magick", "-background", "none", str(MASTER), "-resize", "1024x1024", str(PNG)],
    check=True,
)
print(f"wrote {PNG.relative_to(REPO)} at 1024x1024")

subprocess.run(["npx", "tauri", "icon", str(PNG.relative_to(REPO))], cwd=REPO, check=True)

# tauri icon derives every size from the one master, so the small ones inherit the smudged doorway.
# Re-render those from the small master, and rebuild the .ico so its 16 and 32 entries match.
ICONS = REPO / "src-tauri/icons"
for name, size in (("32x32.png", 32), ("Square30x30Logo.png", 30), ("Square44x44Logo.png", 44)):
    subprocess.run(
        ["magick", "-background", "none", str(SMALL), "-resize", f"{size}x{size}",
         str(ICONS / name)],
        check=True,
    )
    print(f"re-rendered {name} from the small master")

ico_parts = []
for src, sizes in ((SMALL, (16, 24, 32, 48)), (MASTER, (64, 128, 256))):
    for s in sizes:
        out = ICONS / f".ico-{s}.png"
        subprocess.run(
            ["magick", "-background", "none", str(src), "-resize", f"{s}x{s}", str(out)],
            check=True,
        )
        ico_parts.append(out)
subprocess.run(["magick", *map(str, ico_parts), str(ICONS / "icon.ico")], check=True)
for p in ico_parts:
    p.unlink()
print("rebuilt icon.ico: 16/24/32/48 without the doorway, 64/128/256 with it")
