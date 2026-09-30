#!/usr/bin/env python3
"""The sites role classification cannot decide, fixed by hand and recorded here.

Each one needed a meaning the CSS does not carry:

  The status dot's glow is a ring the same hue as the dot, which a classifier reads as a focus ring
  because both are `0 0 0 Npx`. A focus ring on a status dot would move with the keyboard.

  The pulse dot marks indexing in progress. It was red, which reads as a failure; `status-info` is the
  colour for work happening.

  The violet was a second brand signal running beside the red. 20-color.md: "Accents never replace
  Redrob Blue as the brand signal; in product, prefer status-*." So it becomes the action colour and
  the alias is deleted, or the next edit reaches for it again.
"""
import pathlib
import sys

SRC = pathlib.Path("src/styles.css")
text = SRC.read_text()


def sub(old: str, new: str, *, count: int = 0, why: str) -> None:
    global text
    n = text.count(old)
    if n == 0:
        sys.exit(f"pattern absent, nothing changed: {why}\n  {old!r}")
    if count and n != count:
        sys.exit(f"expected {count} of {old!r}, found {n}: {why}")
    text = text.replace(old, new)
    print(f"{n:3} x  {why}")


sub(
    "  background: var(--status-success);\n"
    "  box-shadow: 0 0 0 var(--focus-ring-width) var(--focus-ring);\n",
    "  background: var(--status-success);\n"
    "  box-shadow: 0 0 0 3px color-mix(in oklab, var(--status-success) 18%, transparent);\n",
    count=1,
    why="status dot keeps a glow of its own hue, not a focus ring",
)
sub(
    ".pulse-dot {\n  background: var(--action-primary);\n  animation: pulse 1.6s infinite;\n}",
    ".pulse-dot {\n  background: var(--status-info);\n"
    "  box-shadow: 0 0 0 3px color-mix(in oklab, var(--status-info) 18%, transparent);\n"
    "  animation: pulse 1.6s infinite;\n}",
    count=1,
    why="indexing in progress is status-info, not a failure colour and not the action colour",
)
sub(
    "  background: linear-gradient(135deg, var(--brand), var(--violet));",
    "  background: var(--gradient-deep);",
    count=1,
    why="two brand signals in one gradient -> the shipped brand gradient",
)

# A control is not a panel. system.css draws its own primary as `.rr-btn--primary { background:
# var(--action-primary) }` with `--action-primary-hover` on hover -- flat, no gradient. `gradient-deep`
# is documented "for a filled panel that still has to carry white text", so it stays on the decorative
# orbs (.local-core, .thinking-mark) and comes off the button and the two meter fills.
sub(
    ".primary {\n  min-height: 39px;\n  padding: 0 15px;\n  border-radius: 9px;\n"
    "  color: var(--ink-on-brand);\n  background: var(--gradient-deep);",
    ".primary {\n  min-height: 39px;\n  padding: 0 15px;\n  border-radius: 9px;\n"
    "  color: var(--ink-on-brand);\n  background: var(--action-primary);",
    count=1,
    why="primary button takes the flat action fill, as system.css does",
)
sub(
    ".primary:hover:not(:disabled) {\n  transform: translateY(-1px);\n"
    "  box-shadow: var(--shadow-md);",
    ".primary:hover:not(:disabled) {\n  transform: translateY(-1px);\n"
    "  background: var(--action-primary-hover);\n  box-shadow: var(--shadow-md);",
    count=1,
    why="hover moves to action-primary-hover instead of only lifting",
)
for meter in (".match-meter i", ".progress-track i"):
    marker = f"{meter} {{\n"
    start = text.index(marker)
    end = text.index("}", start)
    block = text[start:end]
    if "var(--gradient-deep)" not in block:
        sys.exit(f"{meter} no longer carries a gradient")
    text = text[:start] + block.replace(
        "var(--gradient-deep)", "var(--action-primary)"
    ) + text[end:]
    print(f"  1 x  {meter} meter fill is the action colour, not a panel gradient")
sub("var(--violet)", "var(--action-primary)", why="violet was a second brand signal")
sub(
    "  --violet: var(--accent-violet-3);\n",
    "",
    count=1,
    why="alias deleted so nothing reintroduces the second brand signal",
)

SRC.write_text(text)
print("\nremaining --violet references:", text.count("--violet"))
