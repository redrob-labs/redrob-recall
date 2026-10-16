#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Write and check the release manifest Redrob Launcher reads, and the icon it names.

    tools/release_manifest.py --write <dir>     write <dir>/redrob-recall.png and <dir>/redrob-launcher.json
    tools/release_manifest.py --check <dir>     refuse a <dir> whose manifest or icon is not as written

redrob-launcher `docs/release-manifest.md` (schema 1): one asset `redrob-launcher.json` beside the
installers, `{ "schema": 1, "sharedAccount": true, "icon": "redrob-recall.png" }`. `sharedAccount` is
true because this app reads the launcher's shared `account.json` (apps/redrob-recall/src/account.rs);
`icon` names a square PNG asset of the SAME release, 256 px or more and under 1 MiB, which the
launcher shows in its catalog in place of the icon it was built with. The PNG is the 512 px render
of the brand pipeline (scripts/build-brand-assets.py, pinned in brand/SOURCES.json).

Both files are written by the Linux release leg into its artifacts, before the publish job writes
`SHA256SUMS.txt` over every asset, so the launcher's quota-free path (which reads the sums file to
learn the asset names) sees them too. The shared publish job is told `launcher_manifest: false`,
because the manifest is this repository's: the shared job would otherwise write one without the icon
over this one. The check runs in the verify job over the merged artifacts, before anything is
published; it needs no third-party module.
"""

import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ICON_SOURCE = ROOT / "resources" / "icons" / "hicolor" / "512x512" / "apps" / "redrob-recall.png"
ICON_NAME = "redrob-recall.png"
MANIFEST_NAME = "redrob-launcher.json"
MANIFEST = {"schema": 1, "sharedAccount": True, "icon": ICON_NAME}
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
MIN_SIDE = 256
MAX_BYTES = 1 << 20


def png_size(data):
    """(width, height) of a PNG, or None when the bytes are not one."""
    if len(data) < 24 or data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", data[16:24])


def check_icon(path):
    errors = []
    if not path.is_file():
        return [f"{path.name}: missing"]
    data = path.read_bytes()
    size = png_size(data)
    if size is None:
        errors.append(f"{path.name}: not a PNG")
    else:
        width, height = size
        if width != height:
            errors.append(f"{path.name}: {width}x{height} is not square")
        if min(width, height) < MIN_SIDE:
            errors.append(f"{path.name}: {width}x{height} is under {MIN_SIDE} px")
    if len(data) >= MAX_BYTES:
        errors.append(f"{path.name}: {len(data)} bytes is 1 MiB or more")
    return errors


def write(out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    errors = check_icon(ICON_SOURCE)
    if errors:
        sys.exit("\n".join(errors))
    (out / ICON_NAME).write_bytes(ICON_SOURCE.read_bytes())
    (out / MANIFEST_NAME).write_text(json.dumps(MANIFEST) + "\n", encoding="utf-8")
    print(f"wrote {out / ICON_NAME} and {out / MANIFEST_NAME}: {json.dumps(MANIFEST)}")


def check(directory):
    directory = Path(directory)
    errors = []
    manifest_path = directory / MANIFEST_NAME
    if not manifest_path.is_file():
        errors.append(f"{MANIFEST_NAME}: missing")
    else:
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except ValueError as error:
            manifest = None
            errors.append(f"{MANIFEST_NAME}: not JSON ({error})")
        if manifest is not None:
            for key, want in MANIFEST.items():
                if manifest.get(key) != want:
                    errors.append(f"{MANIFEST_NAME}: {key} is {manifest.get(key)!r}, must be {want!r}")
    errors.extend(check_icon(directory / ICON_NAME))
    if errors:
        sys.exit("\n".join(errors))
    print(f"{MANIFEST_NAME} and {ICON_NAME} are as the launcher expects")


def main(argv):
    if len(argv) == 3 and argv[1] == "--write":
        write(argv[2])
    elif len(argv) == 3 and argv[1] == "--check":
        check(argv[2])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
