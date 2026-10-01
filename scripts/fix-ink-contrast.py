#!/usr/bin/env python3
"""Fix the ink-on-ground pairs that measured below 3:1.

Found by research/find-ink-on-ground-collisions.py, not by looking: the same class can sit on a light
ground in one screen and a dark one in another -- `.search-submit` is inside the dark onboarding AND
inside the light search page -- so classifying by selector name was right in one place and invisible in
the other.

  .search-submit     ink-primary on ink-primary, 1.00:1. The glyph was the same colour as its button.
                     It is an action, so it takes the action fill and the ink that belongs on it.
  .source-number     ink-on-brand on ink-primary, 1.05:1 on dark. Same fix, same reason.
  the four .*success tints  status-success on accent-green-1, 1.55:1 on dark. accent-green-1 is a light
                     tint that does not flip with the theme, so on a dark ground it stayed a pale green
                     block. Mixing the status colour into the CURRENT surface gives a tint that follows
                     the theme instead of fighting it.
"""
import pathlib
import sys

SRC = pathlib.Path("src/styles.css")
text = SRC.read_text()


def sub(old: str, new: str, *, why: str, count: int = 1) -> None:
    global text
    found = text.count(old)
    if found != count:
        sys.exit(f"expected {count} of:\n  {old}\nfound {found} ({why})")
    text = text.replace(old, new)
    print(f"  {found} x  {why}")


sub(
    "  color: var(--ink-primary);\n  background: var(--ink);\n}\n.search-submit:hover",
    "  color: var(--ink-on-brand);\n  background: var(--action-primary);\n}\n.search-submit:hover",
    why="search submit: an action fill, with the ink that belongs on it",
)
sub(
    "  color: var(--ink-on-brand);\n  background: var(--ink);\n  font-size: 9px;",
    "  color: var(--ink-on-brand);\n  background: var(--action-primary);\n  font-size: 9px;",
    why="source number badge: readable on both grounds",
)
sub(
    "  --green-soft: var(--accent-green-1);",
    "  --green-soft: color-mix(in oklab, var(--status-success) 16%, var(--surface-raised));",
    why="success tint follows the theme instead of staying a light block on dark",
)

SRC.write_text(text)
print("\ndone")
