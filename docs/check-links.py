#!/usr/bin/env python3
"""Fails if any relative link in the book points at a file that is not there.

    docs/check-links.py

mdBook builds a broken link into a 404 without complaining, so a page that has
quietly rotted looks exactly like a page that is fine until somebody clicks it.
This is the cheap version of noticing: resolve every relative target against the
file it appears in, and exit non-zero on the first one that does not exist.

External URLs are left alone. Checking those needs the network, which makes the
docs build fail for reasons that have nothing to do with the docs — and this is
a repo whose entire argument is that it does not need the network.

Standard library only, and it never enters Cargo.toml. It is a build-time
script for the book, not part of the tool.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "src"
LINK = re.compile(r"\]\(([^)]+)\)")
EXTERNAL = ("http://", "https://", "#", "mailto:")

PAGES = sorted(SRC.rglob("*.md")) + [ROOT / "README.md", ROOT / "DECISIONS.md", ROOT / "STDLIB.md"]


def targets(page):
    for raw in LINK.findall(page.read_text(encoding="utf-8")):
        target = raw.split()[0].strip()
        if target.startswith(EXTERNAL) or not target:
            continue
        yield target.split("#", 1)[0]


def main():
    broken = []
    for page in PAGES:
        if not page.exists():
            continue
        for target in targets(page):
            if not target:
                continue
            if not (page.parent / target).resolve().exists():
                broken.append(f"{page.relative_to(ROOT)} -> {target}")

    if broken:
        print("broken relative links:", file=sys.stderr)
        for b in broken:
            print(f"  {b}", file=sys.stderr)
        return 1
    print(f"checked {len(PAGES)} pages, no broken relative links")
    return 0


if __name__ == "__main__":
    sys.exit(main())
