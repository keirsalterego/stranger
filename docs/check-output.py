#!/usr/bin/env python3
"""Fails if a console block in the docs does not reproduce.

    make && docs/check-output.py

Every `$ stranger scan fixtures/...` line in the book and the README is followed
by what the tool printed when somebody ran it. Nothing kept those in step with
the tool, so they rotted silently: a page claiming output the binary no longer
produces looks exactly like a page that is fine.

Two of those got through in one afternoon. The banded risk score renumbered every
published figure on one branch while the four new format pages were being written
against the old score on another; both merged green, and three pages went out
quoting a number the tool would not print. Separately, the co-occurrence rule's
own three examples had stopped firing entirely when packages gained an origin,
because their hand-written fixtures carry no `resolved` field.

So: run the command, compare the block. Elapsed milliseconds are the one thing
allowed to differ, because they are a measurement rather than a claim.

Only blocks naming `fixtures/` are checked, because a `/tmp` path depends on a
heredoc earlier in the page. stdout and stderr are compared together, since a
usage error is output too, and a block may quote any contiguous run of the real
output rather than only the opening lines — the README elides the header when the
finding is the point.

Standard library only, and it never enters Cargo.toml. It is a build-time script
for the book, not part of the tool.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "release" / "stranger"
PAGES = sorted((ROOT / "docs" / "src").rglob("*.md")) + [ROOT / "README.md"]

# `stranger scan …` optionally prefixed with the path the docs use. A pipe or a
# `--format json` means the block is an excerpt or a single long line, neither of
# which compares usefully.
INVOCATION = re.compile(r"^\$ (?:\./target/release/)?stranger (scan [^|$]*)$")
ELAPSED = re.compile(r"\b\d+ms\b")


def lines(text):
    """Non-blank lines with trailing space and timings normalised away."""
    return [l.rstrip() for l in ELAPSED.sub("<ms>", text).splitlines() if l.strip()]


def window(want, got):
    """True when `want` appears as a contiguous run of `got`.

    A page is allowed to quote the middle of a report — the README shows the
    `tensorflow-gpu` finding without the header above it, because the finding is
    the point there. Anchoring on the first line keeps that honest: every line
    after it still has to match, so a stale number is still caught.
    """
    return any(got[i : i + len(want)] == want for i in range(len(got) - len(want) + 1))


def check(page):
    src = page.read_text().splitlines()
    failures = []
    i = 0
    while i < len(src):
        m = INVOCATION.match(src[i].strip())
        if not m or "--format json" not in src[i] and "fixtures/" not in src[i]:
            i += 1
            continue
        if "--format json" in src[i]:
            i += 1
            continue

        args = m.group(1).split()
        args.insert(1, "--no-color")
        run = subprocess.run([str(BIN), *args], cwd=ROOT, capture_output=True, text=True)

        block, j = [], i + 1
        while j < len(src) and not src[j].startswith("```") and not src[j].strip().startswith("$ "):
            block.append(src[j])
            j += 1

        # stderr as well as stdout: a usage error is output, and the pages that
        # document exit 2 quote it.
        want, got = lines("\n".join(block)), lines(run.stdout + run.stderr)
        if want and not window(want, got):
            failures.append((i + 1, " ".join(args), want, got))
        i = j
    return failures


def main():
    if not BIN.exists():
        sys.exit(f"{BIN} is not built; run `make` first")

    total = 0
    for page in PAGES:
        for line_no, cmd, want, got in check(page):
            rel = page.relative_to(ROOT)
            print(f"{rel}:{line_no}: `stranger {cmd}` does not print this any more")
            for a, b in zip(want, got):
                if a != b:
                    print(f"    docs: {a}\n    real: {b}")
            if len(want) > len(got):
                print(f"    docs has {len(want) - len(got)} lines the tool did not print")
            print()
            total += 1

    if total:
        sys.exit(f"{total} console block(s) do not reproduce")
    print(f"checked {len(PAGES)} pages, every fixture console block reproduces")


if __name__ == "__main__":
    main()
