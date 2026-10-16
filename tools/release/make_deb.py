#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Package a staged Linux tree (tools/release/stage.py) as a .deb.

    make_deb.py <stage> <version> <out.deb>

  /opt/redrob-recall/...                     the tree as is (bin, share/redrob-recall/licenses)
  /usr/bin/redrob-recall                     a symlink, so a terminal finds it
  /usr/share/applications/redrob-recall.desktop   the menu entry
  /usr/share/icons/hicolor/{scalable,16x16..512x512}/apps/redrob-recall.{svg,png}
  /usr/share/doc/redrob-recall/copyright     the LICENSE and NOTICE files

Uninstalling is the system's own: `sudo apt remove redrob-recall`, or the software centre. Nothing is
written outside the package's own files, so removal leaves nothing behind; a person's settings in
~/.config/redrob-recall stay, as for every other app.

Depends is worked out the way dpkg-shlibdeps would: every library the ELF binary needs is looked up
with `dpkg -S` on this build machine. The ones egui/winit/wgpu/rfd load at run time rather than link
(X11, Wayland, GL, GTK for the file dialog) cannot be seen in NEEDED, so they come from a fixed table.
Package names are this build machine's (e.g. the t64 names), which matches the archive's own promise:
it runs on distributions at least as new as the one it was built on.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
ICON_SVG = ROOT / "resources" / "icons" / "redrob-recall.svg"
HICOLOR = ROOT / "resources" / "icons" / "hicolor"  # scripts/build-brand-assets.py, pinned in brand/SOURCES.json
# Libraries the binary links against, by soname, when dpkg -S cannot name them on the build machine
# (GL is often diverted by a vendor driver package there; a user's machine gets it from the
# distribution's GL packages).
KNOWN = {
    "libGL.so.1": "libgl1",
    "libEGL.so.1": "libegl1",
    "libGLX.so.0": "libglx0",
    "libOpenGL.so.0": "libopengl0",
    "libz.so.1": "zlib1g",
}
# Loaded at run time by winit (X11 and Wayland), wgpu (GL/Vulkan) and rfd (the GTK file dialog), so no
# NEEDED entry names them, but the app does not start or cannot open a file without them.
ALWAYS = [
    "libx11-6", "libxcursor1", "libxrandr2", "libxi6", "libxkbcommon0", "libxkbcommon-x11-0",
    "libwayland-client0", "libwayland-cursor0", "libwayland-egl1",
    "libgl1", "libegl1", "libvulkan1", "libfontconfig1", "libgtk-3-0t64 | libgtk-3-0",
]

DESKTOP = """[Desktop Entry]
Type=Application
Name=Redrob Recall
GenericName=Local Search
Comment=Search and ask across your own files, indexed locally
Exec=redrob-recall
Icon=redrob-recall
Terminal=false
Categories=Utility;Office;
StartupWMClass=redrob-recall
"""


def elf_files(stage: Path):
    for p in stage.rglob("*"):
        if p.is_file() and not p.is_symlink():
            with open(p, "rb") as f:
                if f.read(4) == b"\x7fELF":
                    yield p


def depends(stage: Path) -> list[str]:
    lib = stage / "lib"
    carried = {p.name for p in lib.iterdir()} if lib.is_dir() else set()
    needed: set[str] = set()
    for elf in elf_files(stage):
        out = subprocess.run(["readelf", "-d", str(elf)], capture_output=True, text=True).stdout
        needed.update(re.findall(r"\(NEEDED\)\s+Shared library: \[(.+?)\]", out))
    system = sorted(n for n in needed if n not in carried)
    pkgs: set[str] = set(ALWAYS)
    unresolved = []
    for name in system:
        if name in KNOWN:
            pkgs.add(KNOWN[name])
            continue
        r = subprocess.run(["dpkg", "-S", f"*/{name}"], capture_output=True, text=True)
        owners = set()
        for line in r.stdout.splitlines():
            # "pkg[:arch][, pkg...]: /path"; dpkg also prints "diversion by ... from/to: ..." lines.
            if line.startswith("diversion ") or ": /" not in line:
                continue
            for owner in line.split(": /", 1)[0].split(","):
                owners.add(owner.strip().split(":")[0])
        # Only distribution library packages: anything else is this build machine's own software.
        owners = {o for o in owners if o.startswith("lib") and not o.endswith("-dev")}
        if not owners:
            unresolved.append(name)
            continue
        pkgs.add(sorted(owners)[0])
    if unresolved:
        sys.exit("no package found for: " + ", ".join(unresolved) + " (add it to KNOWN)")
    return sorted(pkgs)


def main() -> int:
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    stage, version, out = Path(sys.argv[1]).resolve(), sys.argv[2], Path(sys.argv[3]).resolve()
    if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?", version):
        sys.exit("version must be X.Y.Z")
    if not (stage / "bin" / "redrob-recall").is_file():
        sys.exit("the tree has no bin/redrob-recall; run tools/release/stage.py first")
    deps = depends(stage)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "pkg"
        shutil.copytree(stage, root / "opt" / "redrob-recall", symlinks=True)
        (root / "usr" / "bin").mkdir(parents=True)
        os.symlink("/opt/redrob-recall/bin/redrob-recall", root / "usr" / "bin" / "redrob-recall")
        apps = root / "usr" / "share" / "applications"
        apps.mkdir(parents=True)
        (apps / "redrob-recall.desktop").write_text(DESKTOP)
        icons = root / "usr" / "share" / "icons" / "hicolor"
        (icons / "scalable" / "apps").mkdir(parents=True)
        shutil.copyfile(ICON_SVG, icons / "scalable" / "apps" / "redrob-recall.svg")
        for size_dir in sorted(HICOLOR.iterdir()):
            (icons / size_dir.name / "apps").mkdir(parents=True)
            shutil.copyfile(size_dir / "apps" / "redrob-recall.png", icons / size_dir.name / "apps" / "redrob-recall.png")
        docs = root / "usr" / "share" / "doc" / "redrob-recall"
        docs.mkdir(parents=True)
        shutil.copyfile(ROOT / "LICENSE", docs / "copyright")
        shutil.copyfile(ROOT / "NOTICE", docs / "NOTICE")
        size_kb = sum(f.stat().st_size for f in root.rglob("*") if f.is_file() and not f.is_symlink()) // 1024
        (root / "DEBIAN").mkdir()
        (root / "DEBIAN" / "control").write_text(
            "Package: redrob-recall\n"
            f"Version: {version}\n"
            "Section: utils\n"
            "Priority: optional\n"
            "Architecture: amd64\n"
            "Maintainer: Janghoon Lee (Redrob) <44862514+savagemanage@users.noreply.github.com>\n"
            f"Installed-Size: {size_kb}\n"
            f"Depends: {', '.join(deps)}\n"
            "Homepage: https://github.com/redrob-labs/redrob-recall\n"
            "Description: Redrob Recall local memory and search layer\n"
            " Redrob Recall indexes the folders you choose on this machine (text, PDF,\n"
            " DOCX, HTML, CSV and more), searches them by keyword and meaning, and serves\n"
            " the index to the other Redrob apps over a loopback API. Installed under\n"
            " /opt/redrob-recall.\n")
        for p in root.rglob("*"):
            if p.is_symlink():
                continue
            # Libraries are not run directly (Debian policy 10.9): 0644 for them, 0755 for programs.
            is_lib = ".so" in p.name
            p.chmod(0o755 if p.is_dir() or (os.access(p, os.X_OK) and not is_lib) else 0o644)
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["dpkg-deb", "--root-owner-group", "-Zxz", "--build", str(root), str(out)],
                       check=True, stdout=subprocess.DEVNULL)
    print(f"built {out} ({out.stat().st_size // (1024 * 1024)} MB); Depends: {', '.join(deps)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
