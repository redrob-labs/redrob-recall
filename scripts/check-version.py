#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""The workspace version has a released CHANGELOG section, and the tag (when given) names it.

    scripts/check-version.py [vX.Y.Z]

Reads `version` from the root Cargo.toml's [workspace.package] and requires `## X.Y.Z - YYYY-MM-DD` in
CHANGELOG.md (not `- unreleased`). With a tag argument, the tag must equal `v<version>`. The release
notes say "See CHANGELOG", so a version with no entry there ships notes that describe nothing.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def workspace_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    block = re.search(r"\[workspace\.package\](.*?)(?:\n\[|\Z)", text, re.S)
    if not block:
        sys.exit("Cargo.toml has no [workspace.package]")
    match = re.search(r'^version = "([^"]+)"', block.group(1), re.M)
    if not match:
        sys.exit("[workspace.package] has no version")
    return match.group(1)


def main():
    version = workspace_version()
    if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?", version):
        sys.exit(f"workspace version {version!r} is not X.Y.Z")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    heading = re.search(rf"^## {re.escape(version)} - (\S+)", changelog, re.M)
    if not heading:
        sys.exit(f"CHANGELOG.md has no '## {version} - <date>' section")
    if heading.group(1) == "unreleased":
        sys.exit(f"CHANGELOG.md still marks {version} as unreleased")
    if len(sys.argv) > 1 and sys.argv[1]:
        tag = sys.argv[1]
        if tag != f"v{version}":
            sys.exit(f"tag {tag} does not match the workspace version {version}")
    print(f"version {version}: CHANGELOG section dated {heading.group(1)}")


if __name__ == "__main__":
    main()
