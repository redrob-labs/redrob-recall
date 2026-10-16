#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Build every Redrob Recall brand asset from the design system's published files, and pin them.

    scripts/build-brand-assets.py --design-system ~/workplace/redrob-design-system

Nothing here redraws the Redrob symbol. The symbol path is LIFTED from a shipped product icon
(assets/Products/redrob-desk-icon.svg) rather than retyped, the tile colour is read from tokens.css, and
the files those come from are pinned by sha256 in brand/SOURCES.json together with the sha256 of every
output. scripts/check-brand.py, which CI runs, refuses an output tree that differs from the pins, so a
hand edit, a re-trace or a recolor fails the gate. Ported from redrob-canvas's scripts/build-brand-assets.py.

The icon (assets/Products/README.md, not invention):
  Tile        square top-left corner, 22% radius on the other three from 48px up; 19% at 32px.
  Symbol      the shipped file, Solid White, on every tile. Never redrawn or recoloured.
  Doorway     "Redrob's doorway is empty. Something in the doorway means a product." Recall's is a
              lens: 45-icons.md keeps a ring that IS the thing closed, so the lens is a closed ring with
              its handle on the set's 40 degree rake.
  Placement   glyph ink 12% of the door's width in from each side, centred on the gate's waist, no
              taller than 64% of the door.
  Tile colour Desk's band (tokens.json `product-desk`). 20-color.md closes the spectrum at seven and states there
              is no eighth product colour; 10-logo.md's way out is that a new product ships as a feature
              of the product whose band it fits, and Recall's job -- your own files on your own machine --
              is Desk's.
  Small       `-32.svg and -16.svg carry tile and symbol only`: the 16/24/32/48 renders come from a
              second master without the lens, which would be a smudge at that size.

The lockups follow assets/Products/README.md's product-lockup rule: the Redrob Primary lockup placed
untouched, 400px tall, with the name in Pretendard SemiBold at 70% of the height, set 40% of the height
after the wordmark, on the wordmark's baseline. The construction is proven before anything is written:
the shipped Browser lockup is rebuilt the same way and must match. The splash / About image is Redrob
Black with the white lockup unit at 56px. The installer PNGs are the 256 icon and the white lockup scaled.

Needs ImageMagick 7 (`magick`) and `resvg` for rendering; the check does not.
"""

import argparse
import hashlib
import json
import math
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES_JSON = ROOT / "brand" / "SOURCES.json"

SOURCES = {
    "desk-icon": "assets/Products/redrob-desk-icon.svg",
    "lockup": "assets/Logos/redrob-logo-primary.png",
    "lockup-white": "assets/Logos/redrob-logo-primary-white.png",
    "browser-lockup-white": "assets/Products/redrob-browser-lockup-white.png",
    "pretendard-semibold": "fonts/Pretendard-SemiBold.woff2",
    "tokens": "tokens.css",
    "tokens-json": "tokens.json",
}

# Outputs, relative to the repository root.
ICONS = "resources/icons"
BRAND = "resources/brand"
OUT_MASTER = f"{BRAND}/redrob-recall-icon.svg"
OUT_MASTER_SMALL = f"{BRAND}/redrob-recall-icon-small.svg"
OUT_ICO = f"{ICONS}/redrob-recall.ico"
OUT_ICNS = f"{ICONS}/redrob-recall.icns"
OUT_SVG_APP = f"{ICONS}/redrob-recall.svg"
HICOLOR_SIZES = (16, 32, 48, 64, 128, 256, 512)
OUT_LOCKUP = f"{BRAND}/redrob-recall-lockup.png"
OUT_LOCKUP_WHITE = f"{BRAND}/redrob-recall-lockup-white.png"
OUT_SPLASH = f"{BRAND}/redrob-recall-splash.png"
OUT_WIN_ICON = "installer/windows/res/icon.png"
OUT_WIN_LOCKUP = "installer/windows/res/lockup.png"
OUT_MAC_ICON = "installer/macos/res/icon.png"

PRODUCT = "Recall"
LOCKUP_HEIGHT = 400  # assets/Products/README.md: 400px tall, transparent
SPLASH_SIZE = (1000, 500)
SPLASH_LOCKUP_HEIGHT = 56  # px; the lockup's minimum on screen is 18
# The setup window draws the lockup 34 dip tall (above the 18px minimum); 68px covers 200% displays.
SETUP_LOCKUP_HEIGHT = 68
# Measured from redrob-browser-lockup-white.png (3218 x 400): the name's ink spans rows 128..328 and
# "Browser" has no descender, so the baseline is row 327. Pretendard SemiBold at 280pt (70% of 400)
# reproduces the shipped name's width to the pixel with this tracking.
NAME_BASELINE = 327 / 400
NAME_KERNING = -4.5 / 280  # per point
NAME_TAIL = 6  # the shipped lockups end 6px after the name's ink
BROWSER_TOLERANCE = 0.0075

# 22% of 512 = 112.64; 19% of 512 = 97.28 (the 32px radius, assets/Products/README.md).
TILE = ("M0 0H399.36A112.64 112.64 0 0 1 512 112.64V399.36A112.64 112.64 0 0 1 399.36 512"
        "H112.64A112.64 112.64 0 0 1 0 399.36Z")
TILE_32 = ("M0 0H414.72A97.28 97.28 0 0 1 512 97.28V414.72A97.28 97.28 0 0 1 414.72 512"
           "H97.28A97.28 97.28 0 0 1 0 414.72Z")


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def magick(*args, capture=False):
    result = subprocess.run(["magick", *map(str, args)], check=True, capture_output=True)
    return result.stdout if capture else None


def resvg_bin():
    for candidate in ("resvg", str(Path.home() / ".cargo" / "bin" / "resvg")):
        if shutil.which(candidate) or Path(candidate).is_file():
            return candidate
    sys.exit("resvg is needed to render the icon masters (cargo install resvg)")


def render(svg, size, out):
    subprocess.run([resvg_bin(), "-w", str(size), "-h", str(size), str(svg), str(out)], check=True)
    magick(out, "-define", "png:exclude-chunks=date,time", "-strip", f"PNG32:{out}")
    return out.read_bytes()


def png_size(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    return struct.unpack(">II", data[16:24])


def token(tokens_css, name):
    """A token's hex value as the light theme resolves it (its block comes first in tokens.css)."""
    match = re.search(rf"--{re.escape(name)}:\s*(#[0-9a-fA-F]{{6}}|var\(--[\w-]+\))", tokens_css)
    if not match:
        sys.exit(f"tokens.css has no --{name}")
    value = match.group(1)
    if value.startswith("var("):
        return token(tokens_css, value[6:-1])
    return value.lower()


def product_token(tokens_json, name):
    """A product mark colour from tokens.json (tokens.css carries no product colours)."""
    for entry in json.loads(tokens_json)["color"]["tokens"]:
        if entry.get("name") == name:
            return entry["value"].lower()
    sys.exit(f"tokens.json has no color {name}")


def write_ico(path, pngs):
    """ICO with PNG entries (Windows Vista and later)."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    entries, blobs = b"", b""
    offset = 6 + 16 * len(pngs)
    for data in pngs:
        width, height = png_size(data)
        entries += struct.pack(
            "<BBBBHHII", width % 256, height % 256, 0, 0, 1, 32, len(data), offset + len(blobs)
        )
        blobs += data
    path.write_bytes(header + entries + blobs)


def write_icns(path, entries):
    """ICNS of PNG elements: (type, png bytes) pairs."""
    body = b"".join(kind.encode("ascii") + struct.pack(">I", 8 + len(data)) + data for kind, data in entries)
    path.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


def symbol_element(desk_icon_svg):
    """The white symbol <path> from the shipped Desk icon, unchanged (with its transform)."""
    match = re.search(r'(<path d="M54\.4[^/]*?/>)', desk_icon_svg)
    if not match:
        sys.exit("could not lift the symbol path out of redrob-desk-icon.svg")
    return match.group(1)


def lens():
    """The doorway glyph: a filled annulus with its handle on the 40 degree rake. Desk's folder
    occupies x 212..300, y 207..314 in the shipped master, and every shipped glyph is FILLED white."""
    cx, cy, outer, inner = 252.0, 250.0, 32.0, 20.0
    rake = math.radians(40.0)
    dx, dy = math.sin(rake), math.cos(rake)
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


def master_svg(tile_path, tile_colour, symbol, doorway):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"'
        ' role="img" aria-label="Redrob Recall"><title>Redrob Recall</title>'
        f'<path d="{tile_path}" fill="{tile_colour}"/>{symbol}{doorway}</svg>\n'
    )


def compose_lockup(logo, name, colour, height, font, out, scratch):
    """The product-lockup rule: the Primary lockup untouched, the name after it on its baseline."""
    mark = scratch / f"mark-{height}-{out.name}"
    magick(logo, "-filter", "Lanczos", "-resize", f"x{height}", "-strip",
           "-define", "png:exclude-chunks=date,time", f"PNG32:{mark}")
    mark_width = int(magick(mark, "-format", "%w", "info:", capture=True))
    size = 0.7 * height
    gap = round(0.4 * height)
    kerning = NAME_KERNING * size
    text = ["-font", font, "-pointsize", f"{size:g}", "-kerning", f"{kerning:g}", "-fill", colour]
    canvas_width = mark_width + gap + int(size * len(name) * 1.2)
    magick(
        "-size", f"{canvas_width}x{height}", "xc:none",
        mark, "-geometry", "+0+0", "-composite",
        *text, "-annotate", f"+{mark_width + gap}+{round(NAME_BASELINE * height)}", name,
        "-define", "trim:edges=east", "-trim", "+repage",
        "-gravity", "east", "-background", "none", "-splice", f"{NAME_TAIL}x0", "+gravity",
        "-strip", "-define", "png:exclude-chunks=date,time", f"PNG32:{out}",
    )


def verify_construction(design, font, scratch):
    """Rebuild the shipped Browser lockup this way; refuse to write anything if it differs."""
    shipped = design / SOURCES["browser-lockup-white"]
    rebuilt = scratch / "browser.png"
    compose_lockup(design / SOURCES["lockup-white"], "Browser", "white", LOCKUP_HEIGHT, font, rebuilt, scratch)
    width, height = magick(shipped, "-format", "%w %h", "info:", capture=True).decode().split()
    padded = scratch / "browser-padded.png"
    magick(rebuilt, "-background", "none", "-extent", f"{width}x{height}", f"PNG32:{padded}")
    share = float(magick(
        "(", shipped, "-alpha", "extract", ")", "(", padded, "-alpha", "extract", ")",
        "-compose", "difference", "-composite", "-threshold", "50%", "-format", "%[fx:mean]", "info:",
        capture=True,
    ))
    if share > BROWSER_TOLERANCE:
        sys.exit(f"rebuilt Browser lockup differs from the shipped one in {share:.2%} of pixels; "
                 "the construction is not the delivery's, so nothing was written")
    print(f"construction check: rebuilt Browser lockup matches the shipped one ({share:.3%} pixels differ)")


def build_splash(path, design, black, font, scratch):
    width, height = SPLASH_SIZE
    unit = scratch / "splash-unit.png"
    compose_lockup(design / SOURCES["lockup-white"], PRODUCT, "white", SPLASH_LOCKUP_HEIGHT, font, unit, scratch)
    unit_width = int(magick(unit, "-format", "%w", "info:", capture=True))
    left = (width - unit_width) // 2
    top = (height - SPLASH_LOCKUP_HEIGHT) // 2
    magick(
        "-size", f"{width}x{height}", f"xc:{black}",
        unit, "-geometry", f"+{left}+{top}", "-composite",
        "-alpha", "off", "-define", "png:exclude-chunks=date,time", "-strip", f"PNG24:{path}",
    )


def build(design):
    missing = [rel for rel in SOURCES.values() if not (design / rel).is_file()]
    if missing:
        sys.exit(f"design system files missing: {missing}")
    read = {key: (design / rel).read_bytes() for key, rel in SOURCES.items()}
    tokens_css = read["tokens"].decode("utf-8")
    tile = product_token(read["tokens-json"].decode("utf-8"), "product-desk")
    black, ink = token(tokens_css, "redrob-black"), token(tokens_css, "ink-primary")
    desk_svg = read["desk-icon"].decode("utf-8")
    if tile.upper() not in desk_svg.upper():
        sys.exit("redrob-desk-icon.svg's tile is not tokens.css --product-desk")
    symbol = symbol_element(desk_svg)
    font = design / SOURCES["pretendard-semibold"]

    with tempfile.TemporaryDirectory() as temporary:
        scratch = Path(temporary)
        verify_construction(design, font, scratch)
        master = scratch / "master.svg"
        small = scratch / "small.svg"
        master.write_text(master_svg(TILE, tile, symbol, lens()), encoding="utf-8")
        small.write_text(master_svg(TILE_32, tile, symbol, ""), encoding="utf-8")
        # 16..48 from the lens-less master, 64 and up from the full one.
        pngs = {s: render(small, s, scratch / f"icon-{s}.png") for s in (16, 24, 32, 48)}
        pngs.update({s: render(master, s, scratch / f"icon-{s}.png") for s in (64, 128, 256, 512, 1024)})

        def write_lockup(logo_key, colour):
            return lambda p: compose_lockup(design / SOURCES[logo_key], PRODUCT, colour, LOCKUP_HEIGHT, font, p, scratch)

        def scale_lockup_white(p):
            magick(ROOT / OUT_LOCKUP_WHITE, "-filter", "Lanczos", "-resize", f"x{SETUP_LOCKUP_HEIGHT}",
                   "-strip", "-define", "png:exclude-chunks=date,time", f"PNG32:{p}")

        outputs = {
            OUT_MASTER: lambda p: p.write_bytes(master.read_bytes()),
            OUT_MASTER_SMALL: lambda p: p.write_bytes(small.read_bytes()),
            OUT_SVG_APP: lambda p: p.write_bytes(master.read_bytes()),
            OUT_ICO: lambda p: write_ico(p, [pngs[16], pngs[24], pngs[32], pngs[48], pngs[64], pngs[128], pngs[256]]),
            OUT_ICNS: lambda p: write_icns(p, [
                ("icp4", pngs[16]), ("icp5", pngs[32]), ("icp6", pngs[64]),
                ("ic07", pngs[128]), ("ic08", pngs[256]), ("ic09", pngs[512]), ("ic10", pngs[1024]),
                ("ic11", pngs[32]), ("ic12", pngs[64]), ("ic13", pngs[256]), ("ic14", pngs[512]),
            ]),
            **{f"{ICONS}/hicolor/{s}x{s}/apps/redrob-recall.png": (lambda data: lambda p: p.write_bytes(data))(pngs[s])
               for s in HICOLOR_SIZES},
            OUT_LOCKUP: write_lockup("lockup", ink),
            OUT_LOCKUP_WHITE: write_lockup("lockup-white", "white"),
            OUT_SPLASH: lambda p: build_splash(p, design, black, font, scratch),
            OUT_WIN_ICON: lambda p: p.write_bytes(pngs[256]),
            OUT_MAC_ICON: lambda p: p.write_bytes(pngs[256]),
            OUT_WIN_LOCKUP: scale_lockup_white,
        }
        for rel, make in outputs.items():
            target = ROOT / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            make(target)

    pins = {
        "//": "Written by scripts/build-brand-assets.py; checked by scripts/check-brand.py. Do not edit.",
        "design_system": "redrob-design-system (the published distribution)",
        "sources": {rel: sha256(design / rel) for rel in SOURCES.values()},
        "symbol_element_sha256": hashlib.sha256(symbol.encode("utf-8")).hexdigest(),
        "tokens": {"product-desk": tile, "redrob-black": black, "ink-primary": ink},
        "product_band": {
            "band": "desk",
            "tile": tile,
            "why": "20-color.md closes the product spectrum at seven; 10-logo.md ships a new product in the "
                   "band it fits, and Recall's job (your own files on your own machine) is Desk's.",
        },
        "owned_dirs": [ICONS, BRAND],
        "outputs": {rel: sha256(ROOT / rel) for rel in sorted(outputs)},
    }
    SOURCES_JSON.parent.mkdir(parents=True, exist_ok=True)
    SOURCES_JSON.write_text(json.dumps(pins, indent=2) + "\n", encoding="utf-8")
    for rel in sorted(outputs):
        print(f"  {rel}  {pins['outputs'][rel][:12]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--design-system", required=True, type=Path)
    args = parser.parse_args()
    build(args.design_system.expanduser().resolve())


if __name__ == "__main__":
    main()
