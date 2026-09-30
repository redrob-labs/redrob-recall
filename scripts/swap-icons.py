#!/usr/bin/env python3
"""Swap recall's lucide glyphs for the design system's, snapping to its two sanctioned sizes."""
import pathlib
import re
import sys

# lucide name -> design-system glyph. Decisions that are not a plain rename:
#   Bot, Sparkles -> sparkle   45-icons.md gives the AI group two glyphs; `sparkle` is the one
#                              for generated content, which is what both marked.
#   CircleHelp    -> warning   both sites are an error banner and a "Needs attention" stat, not a
#                              question. "One meaning, one icon."
#   HardDrive     -> stack     stored bytes on this device; the set has no drive glyph.
#   Unplug        -> plug      one glyph per meaning; the off-state is carried by the label.
MAP = {
    "Archive": "archive",
    "ArrowRight": "arrowRight",
    "Bot": "sparkle",
    "Check": "check",
    "ChevronRight": "chevronRight",
    "CircleHelp": "warning",
    "Database": "database",
    "ExternalLink": "external",
    "File": "file",
    "FileCode2": "fileCode",
    "FileText": "fileText",
    "Folder": "folder",
    "FolderOpen": "folderOpen",
    "HardDrive": "stack",
    "KeyRound": "key",
    "LockKeyhole": "lock",
    "Plus": "plus",
    "RefreshCw": "refresh",
    "Search": "search",
    "Settings": "settings",
    "ShieldCheck": "shieldCheck",
    "Sparkles": "sparkle",
    "Trash2": "trash",
    "Unplug": "plug",
    "X": "close",
}

path = pathlib.Path("src/App.tsx")
text = path.read_text()
before = text

# 1. The import block goes; the wrapper comes in.
text = re.sub(
    r'import \{\n(?:  \w+,\n)+\} from "lucide-react";\n',
    'import { Icon } from "./ui/Icon";\n',
    text,
    count=1,
)
if 'from "lucide-react"' in text:
    sys.exit("lucide import block not replaced")


def snap(px: str | None) -> str:
    """16 beside body/label, 24 beside body-lg. Nothing between, nothing outside."""
    if px is None:
        return ""
    return ' size={24}' if int(px) > 16 else ""


def rewriter(glyph: str):
    def one(m: re.Match[str]) -> str:
        attrs = m.group(1) or ""
        px = re.search(r'size=\{(\d+)\}', attrs)
        rest = re.sub(r'\s*size=\{\d+\}', "", attrs).rstrip()
        return f'<Icon name="{glyph}"{snap(px.group(1) if px else None)}{rest} />'

    return one


count = 0
for lucide, glyph in MAP.items():
    text, n = re.subn(rf'<{lucide}((?:\s+[^<>]*?)?)\s*/>', rewriter(glyph), text)
    count += n

path.write_text(text)
print(f"rewrote {count} glyph sites, {len(before.splitlines())} -> {len(text.splitlines())} lines")
leftover = sorted(set(re.findall(r'<(' + "|".join(MAP) + r')[\s/>]', text)))
print("leftover lucide tags:", leftover or "none")
