#!/usr/bin/env python3
"""Read the same files a second way, and disagree loudly.

`sweep.sh` asks whether a reader got through a file. That catches a refusal
and misses the worse failure: a reader that gets through and returns the wrong
number. Nothing in the fixtures can catch that either, because the expected
counts in `tests/` were themselves produced by this reader — a test written
that way pins the behaviour, it does not check it.

So this counts the same files independently, in a different language, from
rules written against each format's spec rather than against `src/`:

    Cargo.lock          [[package]] blocks; one with no `source` is a
                        workspace member and lands in `workspace`
    package-lock.json   entries under `packages`, minus the root and minus
                        anything that is a workspace directory or a `link`
    yarn.lock           entry headers, which are the only lines at column 0
    drift rule          names holding more than one version, recomputed from
                        the raw file rather than from the reader's tree

Python's `json` module doing the npm parse is the point of that row: the
hand-rolled `src/json.rs` and a mature implementation have to agree on a
hundred real files, or one of them is wrong.

Run through `make sweep`, or directly with paths:

    ./scripts/crosscheck.py ~/src ~/go
"""

import json
import os
import re
import subprocess
import sys
from collections import defaultdict

BIN = "./target/release/stranger"
NAMES = ("Cargo.lock", "package-lock.json", "yarn.lock")


def scan(path):
    """The reader's answer, or None if it declined the file."""
    r = subprocess.run(
        [BIN, "scan", path, "--format", "json", "-v"],
        capture_output=True,
        text=True,
    )
    line = r.stdout.splitlines()
    if not line:
        return None
    try:
        return json.loads(line[0])
    except json.JSONDecodeError:
        return None


def cargo(text):
    """`[[package]]` blocks, split into third-party and workspace members.

    A block with no `source` key was built from this repository rather than
    fetched, which is what `first_party` means for Cargo.
    """
    blocks = text.split("[[package]]")[1:]
    if not blocks:
        return None
    versions = defaultdict(set)
    third = 0
    for b in blocks:
        if not re.search(r"^source\s*=", b, re.M):
            continue
        third += 1
        n = re.search(r'^name\s*=\s*"([^"]*)"', b, re.M)
        v = re.search(r'^version\s*=\s*"([^"]*)"', b, re.M)
        if n and v:
            versions[n.group(1)].add(v.group(1))
    return {
        "packages": third,
        "workspace": len(blocks) - third,
        "drift": {k: len(s) for k, s in versions.items() if len(s) > 1},
    }


def npm(text):
    """Entries under `packages`, by npm's own rules for what is a dependency.

    The empty key is the root project. A key with no `node_modules/` in it is
    a workspace directory, and `"link": true` is the symlink npm leaves in
    `node_modules` pointing at one; neither is a third-party package.
    """
    d = json.loads(text)
    if d.get("lockfileVersion") not in (2, 3) or "packages" not in d:
        return None
    versions = defaultdict(set)
    n = 0
    for key, v in d["packages"].items():
        if key == "" or "node_modules/" not in key or v.get("link"):
            continue
        n += 1
        name = key.rsplit("node_modules/", 1)[1]
        if "version" in v:
            versions[name].add(v["version"])
    return {
        "packages": n,
        "drift": {k: len(s) for k, s in versions.items() if len(s) > 1},
    }


def yarn(text):
    """Entry headers, which are the only lines a v1 lockfile puts at column 0.

    Berry is a real YAML document wearing the same filename and is skipped
    here the way the reader refuses it.
    """
    if any(l.startswith("__metadata:") for l in text.splitlines()):
        return None
    n = sum(
        1
        for l in text.splitlines()
        if l and not l[0].isspace() and not l.startswith("#") and l.endswith(":")
    )
    return {"packages": n} if n else None


ORACLES = {"Cargo.lock": cargo, "package-lock.json": npm, "yarn.lock": yarn}


def main(roots):
    files = []
    here = os.getcwd() + os.sep
    for root in roots:
        for dirpath, _, filenames in os.walk(root, onerror=lambda e: None):
            if dirpath.startswith(here):  # our own deliberately-broken fixtures
                continue
            for fn in filenames:
                if fn in NAMES:
                    files.append(os.path.join(dirpath, fn))

    checked = skipped = mismatch = 0
    for path in sorted(set(files)):
        try:
            text = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        try:
            want = ORACLES[os.path.basename(path)](text)
        except Exception:
            want = None
        if want is None:
            skipped += 1
            continue
        got = scan(path)
        if got is None:
            skipped += 1  # a refused version; sweep.sh is what reports those
            continue
        checked += 1

        bad = []
        for field in ("packages", "workspace"):
            if field in want and got.get(field) != want[field]:
                bad.append(f"{field}: reader {got.get(field)}, oracle {want[field]}")
        if "drift" in want:
            mine = {
                f["package"]: int(re.match(r"(\d+) versions", f["detail"]).group(1))
                for f in got.get("findings", ())
                if f["rule"] == "drift"
            }
            if mine != want["drift"]:
                only = {k: v for k, v in mine.items() if want["drift"].get(k) != v}
                miss = {k: v for k, v in want["drift"].items() if mine.get(k) != v}
                bad.append(f"drift: reader-only {only}, oracle-only {miss}")
        if bad:
            mismatch += 1
            print(f"MISMATCH {path}")
            for b in bad:
                print(f"         {b}")

    print()
    print(f"crosschecked {checked} lockfiles against an independent count")
    print(f"skipped      {skipped} (a version or format the oracle does not do)")
    print(f"mismatches   {mismatch}")
    return 1 if mismatch else 0


if __name__ == "__main__":
    if not os.path.exists(BIN):
        subprocess.run(["cargo", "build", "--release", "--quiet"], check=True)
    sys.exit(main(sys.argv[1:] or [os.path.expanduser("~")]))
