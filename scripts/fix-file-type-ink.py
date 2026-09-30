#!/usr/bin/env python3
"""File type is carried by the glyph, never by a status colour.

The old sheet coloured PDF red, DOCX blue and Markdown violet. Converted by role those became
`status-danger`, `status-info` and `ink-brand`, so a perfectly good PDF rendered in the colour this app
uses for failure. 45-icons.md: "Color from the token, never the icon", and "One meaning, one icon" --
`success` is never a bare check. A file's type is not a state, and the set already ships a distinct
glyph for each type, which is the signal that belongs here.

Also fixes the orb: its label sat on the brand gradient in `status-info`, i.e. blue on blue.
"""
import pathlib
import sys

SRC = pathlib.Path("src/styles.css")
text = SRC.read_text()


def sub(old: str, new: str, *, why: str) -> None:
    global text
    if text.count(old) != 1:
        sys.exit(f"expected exactly one of {old!r} ({text.count(old)} found): {why}")
    text = text.replace(old, new)
    print(f"  {why}")


sub(
    ".file-icon,\n.tiny-file {\n  display: grid;\n  place-items: center;\n  border-radius: 9px;\n"
    "  color: var(--status-danger);\n  background: var(--surface-raised);\n}\n",
    ".file-icon,\n.tiny-file {\n  display: grid;\n  place-items: center;\n  border-radius: 9px;\n"
    "  color: var(--ink-secondary);\n  background: var(--surface-raised);\n}\n",
    why="one ink for every file tile; the glyph says which type it is",
)
sub(
    ".file-icon.docx,\n.tiny-file.docx {\n  color: var(--status-info);\n"
    "  background: var(--surface-raised);\n}\n",
    "",
    why="the DOCX colour rule goes; fileText already distinguishes it",
)
sub(
    ".file-icon.md,\n.file-icon.json,\n.file-icon.html,\n.tiny-file.md,\n.tiny-file.json,\n"
    ".tiny-file.html {\n  color: var(--ink-brand);\n  background: var(--surface-raised);\n}\n",
    "",
    why="the code-file colour rule goes; fileCode already distinguishes it",
)
sub(
    ".visual-card svg {\n  color: var(--status-danger);\n}",
    ".visual-card svg {\n  color: var(--ink-secondary);\n}",
    why="a file in a preview card is not an error",
)
sub(
    ".local-core small {\n  color: var(--status-info);",
    ".local-core small {\n  color: var(--ink-on-brand);",
    why="the orb's path label is ink on a brand fill, not blue on blue",
)
sub(
    ".local-core span {\n  font-family: \"Manrope\", sans-serif;",
    ".local-core span {",
    why="Manrope is not in the type system; the label inherits font-sans",
)

SRC.write_text(text)
print("\nremaining Manrope references:", text.count("Manrope"))
