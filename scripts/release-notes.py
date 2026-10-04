#!/usr/bin/env python3
"""Print the CHANGELOG.md section for a tag, for use as GitHub release notes.

`gh release create --generate-notes` collects merged pull requests, and this
repository pushes every release commit straight to main, so that generates a
body holding nothing but a compare link. The section commit-and-tag-version
already wrote when it cut the release is the real record, so it is read back
from the tagged tree.

Exits non-zero and prints nothing on stdout when the section is missing or
empty, so a release is never published with an empty body — a failure the
release list hides.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


def section_for(version: str, changelog: str) -> str:
    """The body under the `## [<version>]` heading, without the heading."""
    pattern = rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|\Z)"
    found = re.search(pattern, changelog, re.S | re.M)
    if not found:
        raise SystemExit(f"no CHANGELOG.md section for {version}")
    # Collapse the blank-line runs the generator leaves between headings.
    body = re.sub(r"\n{3,}", "\n\n", found.group(1)).strip()
    if not body:
        raise SystemExit(f"the CHANGELOG.md section for {version} is empty")
    return body


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: release-notes.py <tag>")
    tag = sys.argv[1]
    version = tag[1:] if tag.startswith("v") else tag

    path = Path("CHANGELOG.md")
    if not path.exists():
        raise SystemExit("no CHANGELOG.md; the release config did not run")
    sys.stdout.write(section_for(version, path.read_text()) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
