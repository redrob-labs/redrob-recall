#!/usr/bin/env python3
"""Convert every colour literal in styles.css to a design system token, by role.

Not a literal-for-literal table: the old palette was improvised, so `#817b89` has no token and never
will. What a token exists for is the ROLE -- body ink, muted ink, a raised surface, a subtle border, a
danger tint -- so each site is classified by the property it sets, the ground it sits on, and the
colour's own lightness and hue family, then given the token for that role.

WHOLE DECLARATIONS FIRST, and this is the part a per-colour pass gets wrong. A gradient's stops are not
three independent colours; substituting each one separately collapsed a three-stop gradient into three
copies of the same token, i.e. a flat fill that still said `linear-gradient`. A shadow is a distance
and a colour together, and the design system ships the pair as `shadow-sm/md/lg`. A focus glow is not a
shadow at all -- it is `focus-ring` at `focus-ring-width`. So gradients, shadows and outlines are
replaced as complete values, and only what is left goes through the per-colour classifier.

Two design system rules drive the unusual choices:

  20-color.md, accents: "Accents never replace Redrob Blue as the brand signal; in product, prefer
  status-*." This sheet ran a violet as a second brand signal beside the red, so violet ink and violet
  fills become the brand blue or a status colour -- never an accent step.

  The dark panels stop being hardcoded. `.sidebar`, `.onboarding`, `.modal`, `.progress-toast` and
  `.toast` carry `data-theme="dark"`, so inside them `surface-base` and `ink-primary` already MEAN the
  dark values. That is the mechanism tokens.css ships; a second hardcoded dark palette beside it is how
  the two drift apart.

Run from the repo root. Prints every unconverted site so nothing passes silently.
"""
from __future__ import annotations

import pathlib
import re
import sys

SRC = pathlib.Path("src/styles.css")

# Selector prefixes whose block sits on a dark ground. Ink and surface tokens flip for these.
DARK_ROOTS = (
    ".sidebar", ".nav-item", ".library-mini", ".privacy-foot", ".logo", ".pulse-dot",
    ".onboarding", ".hero", ".visual-orbit", ".local-core", ".local-badge", ".format-list",
    ".search-card", ".search-submit", ".suggestion-grid", ".result-card", ".tiny-file",
    ".ask-examples", ".ask-stage", ".ask-orb", ".ask-intro", ".document-name",
    ".modal", ".progress-toast", ".progress-track", ".progress-copy", ".toast", ".saving",
    ".icon-button", ".status-dot", ".boot-screen",
)

LITERAL = re.compile(r"#[0-9a-fA-F]{3,8}\b|rgba?\([^)]*\)")


def srgb_to_linear(c: float) -> float:
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def parse(lit: str) -> tuple[float, float, float, float] | None:
    lit = lit.strip()
    if lit.startswith("#"):
        h = lit[1:]
        if len(h) == 3:
            h = "".join(c * 2 for c in h)
        if len(h) not in (6, 8):
            return None
        vals = [int(h[i : i + 2], 16) / 255 for i in range(0, 6, 2)]
        a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
        return (*vals, a)  # type: ignore[return-value]
    m = re.match(r"rgba?\(([^)]*)\)", lit)
    if not m:
        return None
    nums = [p.strip() for p in m.group(1).replace("/", ",").split(",")]
    try:
        r, g, b = (float(n.rstrip("%")) / (100 if n.endswith("%") else 255) for n in nums[:3])
        a = float(nums[3]) if len(nums) > 3 else 1.0
    except ValueError:
        return None
    return r, g, b, a


def luminance(r: float, g: float, b: float) -> float:
    return 0.2126 * srgb_to_linear(r) + 0.7152 * srgb_to_linear(g) + 0.0722 * srgb_to_linear(b)


def family(r: float, g: float, b: float) -> str:
    mx, mn = max(r, g, b), min(r, g, b)
    if mx - mn < 0.10:
        return "neutral"
    if mx == r:
        h = 60 * (((g - b) / (mx - mn)) % 6)
    elif mx == g:
        h = 60 * ((b - r) / (mx - mn) + 2)
    else:
        h = 60 * ((r - g) / (mx - mn) + 4)
    if h < 18 or h >= 330:
        return "red"
    if h < 45:
        return "orange"
    if h < 70:
        return "amber"
    if h < 165:
        return "green"
    if h < 200:
        return "teal"
    if h < 255:
        return "blue"
    return "violet"


# Saturated ink. A hue that carries meaning keeps it; the improvised violet and red brand go to blue.
STATUS_INK = {"red": "--status-danger", "green": "--status-success",
              "orange": "--status-warning", "amber": "--status-warning",
              "blue": "--status-info", "teal": "--status-info", "violet": "--ink-brand"}
# Saturated fill. Only success, warning and danger are states; everything else is the action colour.
FILL = {"red": "--action-primary", "violet": "--action-primary", "blue": "--action-primary",
        "teal": "--action-primary", "green": "--status-success",
        "orange": "--status-warning", "amber": "--status-warning"}
TINT = {"red": "--accent-red-1", "green": "--accent-green-1", "orange": "--accent-orange-1",
        "amber": "--accent-yellow-1", "blue": "--accent-sky-1", "teal": "--accent-teal-1",
        "violet": "--accent-violet-1"}


def colour_token(prop: str, lit: str, dark: bool, focus: bool) -> str | None:
    rgba = parse(lit)
    if rgba is None:
        return None
    r, g, b, a = rgba
    lum, fam = luminance(r, g, b), family(r, g, b)
    # A hue thin enough to see through is decoration, not a state. `status-danger` on a decorative
    # ring is a page telling the user something failed; the old sheet drew several such rings in its
    # brand red at 15% alpha purely as ornament.
    if a < 0.35:
        fam = "neutral"

    if prop in ("color", "-webkit-text-fill-color", "fill", "stroke"):
        if fam != "neutral":
            return STATUS_INK[fam]
        if dark:
            # On a dark ground the ramp runs the other way: near-white is the body ink.
            return ("--ink-primary" if lum > 0.62 else
                    "--ink-secondary" if lum > 0.28 else "--ink-muted")
        return ("--ink-primary" if lum < 0.22 else
                "--ink-secondary" if lum < 0.40 else
                "--ink-muted" if lum < 0.72 else "--ink-on-brand")

    if prop.startswith("border"):
        if focus:
            return "--focus-ring"
        if fam != "neutral":
            return STATUS_INK[fam]
        if a < 0.9:
            return "--border-subtle" if a < 0.13 else "--border-strong"
        return "--border-subtle" if lum > 0.70 or dark else "--border-strong"

    if prop in ("background", "background-color"):
        if fam != "neutral":
            return FILL[fam] if lum < 0.45 else TINT[fam]
        if a < 0.9:
            return "--surface-material"
        if dark:
            return "--surface-base" if lum < 0.12 else "--surface-raised"
        return ("--surface-raised" if lum > 0.96 else
                "--surface-sunken" if lum > 0.85 else
                "--border-subtle" if lum > 0.60 else "--surface-base")
    return None


def shadow_value(value: str, dark: bool) -> str:
    """One shadow token per layer. A zero-blur ring is a focus ring, not a shadow."""
    out = []
    for layer in re.split(r",(?![^(]*\))", value):
        layer = layer.strip()
        if not LITERAL.search(layer):
            out.append(layer)
            continue
        if re.match(r"^0\s+0\s+0\s+\d", layer):
            out.append("0 0 0 var(--focus-ring-width) var(--focus-ring)")
            continue
        blurs = [int(n) for n in re.findall(r"(\d+)px", layer)]
        blur = max(blurs) if blurs else 0
        token = "--shadow-sm" if blur < 12 else "--shadow-md" if blur < 45 else "--shadow-lg"
        out.append(f"var({token})")
    # Several layers that all collapsed to the same shadow token say it once.
    seen = [x for i, x in enumerate(out) if x not in out[:i] or not x.startswith("var(--shadow")]
    return ", ".join(seen)


def gradient_value(value: str, dark: bool) -> str:
    """A gradient is one decision, not one per stop."""
    stops = [parse(m.group(0)) for m in LITERAL.finditer(value)]
    solid = [s for s in stops if s and s[3] > 0.5]
    if not solid:
        return "var(--wash-quiet)" if not dark else "var(--wash-deep)"
    lums = [luminance(*s[:3]) for s in solid]
    fams = {family(*s[:3]) for s in solid}
    if max(lums) < 0.25 and fams <= {"neutral"}:
        # A dark panel. data-theme="dark" already makes surface-base this colour. The neutral test
        # matters: a saturated dark red is a filled BUTTON, and reading it as a panel turned the
        # primary action into the page background -- invisible, and the build still passed.
        return "var(--surface-base)"
    if min(lums) > 0.90 and fams <= {"neutral"}:
        return "var(--surface-raised)"
    if fams <= {"neutral"}:
        return "var(--surface-sunken)"
    return "var(--gradient-deep)"


lines = SRC.read_text().splitlines(keepends=True)
selector = ""
converted = skipped_count = 0
notes: list[str] = []

# Collapse multi-line declarations so a whole value is visible at once, then re-split.
text = "".join(lines)
blocks = re.split(r"(?<=[{}])", text)

out_lines: list[str] = []
i = 0
while i < len(lines):
    line = lines[i]
    m = re.match(r"^([.#\[a-zA-Z][^{}]*)\{\s*$", line)
    if m:
        selector = m.group(1).strip()
    dark = any(selector.startswith(r) or f" {r}" in selector for r in DARK_ROOTS)
    focus = ":focus" in selector

    decl = re.match(r"^(\s*)([a-z-]+)\s*:\s*(.*)$", line)
    if decl and not line.lstrip().startswith("--"):
        indent, prop, rest = decl.groups()
        # Gather a value that runs over several lines, up to its semicolon. A `{` means this was
        # never a declaration -- it is a multi-line selector list whose first selector happens to
        # carry a pseudo-class, e.g. `button:focus-visible,`. Crossing that brace once rewrote the
        # selector list itself into a declaration, so the boundary is checked, not assumed.
        value = rest
        span = 1
        while ";" not in value and "{" not in value and i + span < len(lines):
            value += " " + lines[i + span].strip()
            span += 1
        if "{" in value or ";" not in value:
            out_lines.append(line)
            i += 1
            continue
        value = value.rstrip()
        if LITERAL.search(value):
            body = value.rstrip(";").strip()
            if "shadow" in prop:
                new = shadow_value(body, dark)
            elif "gradient(" in body:
                new = gradient_value(body, dark)
            elif prop == "outline":
                new = re.sub(r"\d+px", "var(--focus-ring-width)", body, count=1)
                new = LITERAL.sub("var(--focus-ring)", new)
            else:
                def repl(mm: re.Match[str]) -> str:
                    global converted, skipped_count
                    tok = colour_token(prop, mm.group(0), dark, focus)
                    if tok is None:
                        skipped_count += 1
                        notes.append(f"L{i + 1} {prop:18} {mm.group(0):26} {selector[:40]}")
                        return mm.group(0)
                    converted += 1
                    return f"var({tok})"

                new = LITERAL.sub(repl, body)
            if "shadow" in prop or "gradient(" in body or prop == "outline":
                converted += len(LITERAL.findall(body))
            out_lines.append(f"{indent}{prop}: {new};\n")
            i += span
            continue
    out_lines.append(line)
    i += 1

SRC.write_text("".join(out_lines))
print(f"converted {converted} colour sites to tokens")
left = LITERAL.findall(SRC.read_text())
print(f"{len(left)} literals remain in the file")
for n in notes:
    print("  " + n)
sys.exit(0)
