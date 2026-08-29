#!/usr/bin/env python3
"""Fails if a console block in the docs does not reproduce.

    make && docs/check-output.py

Every `$ ` line in a ```console block in the book, the README, DECISIONS.md and
STDLIB.md is followed by what the tool printed when somebody ran it. Nothing kept
those in step with the tool, so they rotted silently: a page claiming output the
binary no longer produces looks exactly like a page that is fine.

Two of those got through in one afternoon. The banded risk score renumbered every
published figure on one branch while the four new format pages were being written
against the old score on another; both merged green, and three pages went out
quoting a number the tool would not print. Separately, the co-occurrence rule's
own three examples had stopped firing entirely when packages gained an origin,
because their hand-written fixtures carry no `resolved` field.

So: run the command, compare the block.

The first version of this script checked only blocks naming `fixtures/`, on the
grounds that a `/tmp` path depends on a heredoc earlier in the page. That guard
was doing more harm than the problem it avoided — it verified 46 of 169 command
lines, and every one of the 123 it skipped was unverified prose. It hid a whole
class of bug: **nine `/tmp` directories the book scans and never creates.** The
worst was on page one, in the block that proves the graceful-degradation half of
the compliance argument. A judge pasting it got exit 2.

So the setup lines are executed rather than skipped. `mkdir`, `cat > … <<'EOF'`,
`cp`, `touch`, `rm`, `chmod` and `ln` run for real, in order, before the command that
depends on them — which means the page a reader pastes is literally the page that
was verified, and a missing `mkdir` is a failure here rather than a surprise for
somebody else. Nothing else executes: a line starting with anything not on that
list is counted as unverified and reported, so the coverage number stays honest
instead of quietly shrinking.

`$ echo $?` is checked too. The old block loop treated it as a new command and
stopped reading, so every documented exit code in the book was decoration.

Elapsed milliseconds are the one thing allowed to differ, because they are a
measurement rather than a claim — in the human report as `35ms`, in `--format
json` as `"elapsed_ms":35` while that field existed. Both are normalised away;
everything else in a JSON line is a claim like any other.

Standard library only, and it never enters Cargo.toml. It is a build-time script
for the book, not part of the tool.
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "release" / "stranger"
PAGES = sorted((ROOT / "docs" / "src").rglob("*.md")) + [
    ROOT / "README.md",
    ROOT / "DECISIONS.md",
    ROOT / "STDLIB.md",
]

# Commands this script is willing to run. Setup verbs plus the tool itself; a
# pipeline is allowed because several pages quote what `jq` made of the output
# rather than the output. Anything else is counted unverified and reported.
RUNNABLE = re.compile(
    r"^(?:\./target/release/)?(?:stranger|mkdir|cat|cp|rm|chmod|ln|find|printf|echo|touch)\b"
)
HEREDOC = re.compile(r"<<-?'?(\w+)'?")
# `12ms` in the human report, and `"elapsed_ms":12` from when JSON carried the
# clock. The same measurement, normalised the same way.
ELAPSED = re.compile(r'\b\d+ms\b|(?<="elapsed_ms":)\d+')
# Tools a pipeline may reach for. A page is skipped rather than failed when one
# is absent, because a missing `jq` is the checking machine's problem and not
# the book's.
PIPED_TOOLS = ("jq", "grep", "head", "tail", "sort", "wc", "cut", "sed", "awk")


def lines(text):
    """Non-blank lines with trailing space and timings normalised away."""
    return [ln.rstrip() for ln in ELAPSED.sub("<ms>", text).splitlines() if ln.strip()]


def window(want, got):
    """True when `want` appears as a contiguous run of `got`.

    A page is allowed to quote the middle of a report — the README shows the
    `tensorflow-gpu` finding without the header above it, because the finding is
    the point there. Anchoring on the first line keeps that honest: every line
    after it still has to match, so a stale number is still caught.
    """
    return any(got[i : i + len(want)] == want for i in range(len(got) - len(want) + 1))


def missing_tool(cmd):
    """The pipeline tool this command needs and this machine does not have."""
    for tool in PIPED_TOOLS:
        if re.search(rf"\|\s*{tool}\b", cmd) and shutil.which(tool) is None:
            return tool
    return None


def commands(src):
    """Walk one page, yielding (line number, command, expected output lines).

    A heredoc's body belongs to the command, not to the output — reading it as
    output is how a `cat > … <<'EOF'` block used to swallow the next command.
    """
    i = 0
    while i < len(src):
        stripped = src[i].strip()
        if not stripped.startswith("$ "):
            i += 1
            continue
        cmd, start, i = stripped[2:], i + 1, i + 1
        here = HEREDOC.search(cmd)
        if here:
            body = []
            while i < len(src) and src[i].strip() != here.group(1):
                body.append(src[i])
                i += 1
            # The terminator itself, then past it.
            body.append(here.group(1))
            i += 1
            cmd = cmd + "\n" + "\n".join(body)
        out = []
        while i < len(src):
            nxt = src[i]
            if nxt.startswith("```") or nxt.strip().startswith("$ "):
                break
            out.append(nxt)
            i += 1
        yield start, cmd, out


def check(page):
    """Run one page. Returns (failures, verified count, unverified commands)."""
    src = page.read_text().splitlines()
    failures, verified, skipped = [], 0, []
    last_code = None

    for line_no, cmd, block in commands(src):
        want = lines("\n".join(block))

        if cmd.strip() == "echo $?":
            if last_code is None:
                skipped.append((line_no, cmd))
                continue
            verified += 1
            if want != [str(last_code)]:
                failures.append((line_no, cmd, want, [str(last_code)]))
            continue

        if not RUNNABLE.match(cmd):
            skipped.append((line_no, cmd))
            continue
        tool = missing_tool(cmd)
        if tool:
            skipped.append((line_no, f"{cmd}   [no {tool} on this machine]"))
            continue

        run = subprocess.run(
            cmd,
            shell=True,
            executable="/bin/bash",
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=180,
        )
        last_code = run.returncode
        # Setup lines are run for their effect. They print nothing, and a page
        # that quotes output from one is quoting it for a reason, so compare
        # only when the book actually wrote something down.
        if not want:
            continue
        verified += 1
        got = lines(run.stdout + run.stderr)
        if not window(want, got):
            failures.append((line_no, cmd, want, got))

    return failures, verified, skipped


def main():
    if not BIN.exists():
        sys.exit(f"{BIN} is not built; run `make` first")

    # `stranger` on its own, the way the cookbook writes it once installed.
    import os

    os.environ["PATH"] = f"{BIN.parent}:{os.environ['PATH']}"

    total, verified, unverified = 0, 0, []
    for page in PAGES:
        failures, ok, skipped = check(page)
        verified += ok
        unverified += [(page, n, c) for n, c in skipped]
        for line_no, cmd, want, got in failures:
            rel = page.relative_to(ROOT)
            print(f"{rel}:{line_no}: `{cmd}` does not print this any more")
            for a, b in zip(want, got):
                if a != b:
                    print(f"    docs: {a}\n    real: {b}")
            if len(want) > len(got):
                print(f"    docs has {len(want) - len(got)} lines the tool did not print")
            print()
            total += 1

    if "-v" in sys.argv:
        for page, line_no, cmd in unverified:
            print(f"  unverified {page.relative_to(ROOT)}:{line_no}: {cmd}")

    if total:
        sys.exit(f"{total} console block(s) do not reproduce")
    print(
        f"checked {len(PAGES)} pages: {verified} commands reproduce, "
        f"{len(unverified)} not runnable here (`-v` lists them)"
    )


if __name__ == "__main__":
    main()
