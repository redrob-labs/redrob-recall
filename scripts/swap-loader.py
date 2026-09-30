#!/usr/bin/env python3
"""Replace the hand-rolled spinning glyph with the design system's Loader.

45-icons.md has no spinner and says nothing in the set may be animated by the host; the delivery
ships `Loader` for "something is happening and the share is unknown" instead. Loader also keeps its
label in the DOM, so a screen reader is told what is happening -- a bare spinning icon says nothing.
Keyed by line number because several sites are byte-identical.
"""
import pathlib
import sys

# line (1-based, pre-edit) -> (size, label)
SITES = {
    231: ("sm", '"Adding folders"'),
    366: ("sm", "{statusLabel(snapshot.stats.status)}"),
    404: ("sm", '"Searching"'),
    666: ("sm", '"Reading the strongest evidence"'),
    696: ("sm", '"Asking Redrob"'),
    909: ("md", '"Loading files"'),
    1086: ("sm", '"Saving"'),
    1402: ("sm", '"Connecting this device"'),
    1510: ("sm", "{progress.message}"),
    1570: ("sm", '"Opening your local library"'),
}

path = pathlib.Path("src/App.tsx")
lines = path.read_text().splitlines(keepends=True)

for lineno, (size, label) in SITES.items():
    old = lines[lineno - 1]
    if "LoaderCircle" not in old:
        sys.exit(f"line {lineno} is not a spinner site: {old!r}")
    indent = old[: len(old) - len(old.lstrip())]
    tail = old.split("/>", 1)[1].rstrip("\n")
    quoted = label if label.startswith("{") else label
    lines[lineno - 1] = f'{indent}<Loader size="{size}" label={quoted} />{tail}\n'

text = "".join(lines)
if "LoaderCircle" in text:
    sys.exit("a spinner site was missed")
text = text.replace(
    'import { Icon } from "./ui/Icon";\n',
    'import { Loader } from "@redrob-labs/ui";\nimport { Icon } from "./ui/Icon";\n',
    1,
)
path.write_text(text)
print(f"replaced {len(SITES)} spinner sites with Loader")
