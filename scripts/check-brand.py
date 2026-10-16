#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Refuse brand assets that are not exactly what build-brand-assets.py made from the design system.

    scripts/check-brand.py

No dependencies: CI runs it without ImageMagick and without the design system. A build reads no
pixels, so a hand-drawn or recolored mark passes every other check for as long as nobody looks; this
one compares bytes against the pins brand/SOURCES.json recorded when the files were built from the
design system, and refuses any file under resources/icons or resources/brand that the pins do not
account for. Ported from redrob-canvas's scripts/check-brand.py.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES_JSON = ROOT / "brand" / "SOURCES.json"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    errors = []
    pins = json.loads(SOURCES_JSON.read_text(encoding="utf-8"))
    outputs = pins["outputs"]

    present = set()
    for owned in pins["owned_dirs"]:
        for path in (ROOT / owned).rglob("*"):
            if path.is_file():
                present.add(path.relative_to(ROOT).as_posix())
    for rel in sorted(present - set(outputs)):
        errors.append(f"{rel} is not built from the design system (not in brand/SOURCES.json)")
    for rel, digest in sorted(outputs.items()):
        path = ROOT / rel
        if not path.is_file():
            errors.append(f"{rel} is missing")
        elif sha256(path) != digest:
            errors.append(f"{rel} was changed by hand: rebuild it with scripts/build-brand-assets.py")

    # Every SVG master carries the shipped Desk icon's own symbol element, untouched, on the Desk tile,
    # and no effect on it.
    for rel in outputs:
        if not rel.endswith(".svg") or not (ROOT / rel).is_file():
            continue
        text = (ROOT / rel).read_text(encoding="utf-8")
        match = re.search(r'(<path d="M54\.4[^/]*?/>)', text)
        if not match or hashlib.sha256(match.group(1).encode()).hexdigest() != pins["symbol_element_sha256"]:
            errors.append(f"{rel} carries a symbol that is not the design system's")
        if pins["tokens"]["product-desk"] not in text.lower():
            errors.append(f"{rel} is not on the Desk tile ({pins['tokens']['product-desk']})")
        if re.search(r"\b(filter|mask|opacity)=", text):
            errors.append(f"{rel} applies an effect to the mark (filter, mask or opacity)")
        if re.search(r"GPL|GNU General Public", text):
            errors.append(f"{rel} carries GPL text; this tree is Apache-2.0")

    if errors:
        print("\n".join(f"error: {error}" for error in errors), file=sys.stderr)
        sys.exit(1)
    print(f"brand assets match the design system pins: {len(outputs)} files")


if __name__ == "__main__":
    main()
