#!/usr/bin/env python3
"""Reference JSON parser for the differential campaign in tests/json_conformance.rs.

Dev-time tooling. It is never invoked by the binary, it is not in Cargo.toml,
and `cargo test` skips the campaign that calls it unless you ask for it — a
machine with no Python still runs the whole suite.

CPython's `json` is not RFC 8259 out of the box: it accepts `NaN`, `Infinity`
and `-Infinity`, which section 6 does not have. `parse_constant` is the hook
that turns those three back into errors, so what runs here is the RFC's
grammar rather than Python's dialect of it. The rest of CPython's deviations
are not switchable and are the point of the exercise: they show up as
disagreements and get argued one at a time.

Protocol, both directions, so that a document may contain newlines:

    <decimal byte length>\\n<utf-8 bytes>\\n

and one output line per input, `OK <canonical form>` or `ERR <why>`.

The canonical form exists so two parsers written a decade apart in different
languages can be compared without arguing about float formatting or string
escaping:

    null            n
    true / false    T / F
    number          # then the IEEE-754 big-endian bits as hex
    string          s then the code points, decimal, dot-separated
    array           [a,b,c]
    object          {key:value,...} with keys sorted

Numbers go over as bits rather than as text because `repr(1e30)` is `1e+30` in
Python and `1e30` in Rust, and a differential harness that reports that as a
disagreement is reporting on itself.
"""

import json
import struct
import sys


def _no_constants(name):
    raise ValueError(f"RFC 8259 section 6 has no {name}")


def canon(v):
    # bool before float: `isinstance(True, int)` is true in Python, and with
    # parse_int=float every integer arrives as a float, so a bare isinstance
    # chain in the obvious order reports `true` as the number 1.
    if v is None:
        return "n"
    if v is True:
        return "T"
    if v is False:
        return "F"
    if isinstance(v, float):
        return "#" + struct.pack(">d", v).hex()
    if isinstance(v, str):
        return "s" + ".".join(str(ord(c)) for c in v)
    if isinstance(v, list):
        return "[" + ",".join(canon(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ",".join(f"{canon(k)}:{canon(v[k])}" for k in sorted(v)) + "}"
    raise TypeError(f"unexpected {type(v)}")


def frames(data):
    i = 0
    while i < len(data):
        nl = data.index(b"\n", i)
        n = int(data[i:nl])
        start = nl + 1
        yield data[start : start + n]
        i = start + n + 1


def main():
    if len(sys.argv) != 3:
        sys.exit(f"usage: {sys.argv[0]} <input frames> <output lines>")
    with open(sys.argv[1], "rb") as f:
        data = f.read()
    out = []
    for frame in frames(data):
        try:
            text = frame.decode("utf-8")
            out.append("OK " + canon(json.loads(
                text,
                parse_int=float,
                parse_float=float,
                parse_constant=_no_constants,
            )))
        # RecursionError is not a subclass of ValueError, and a nesting depth
        # CPython will not walk is a rejection here rather than a crash — the
        # RFC lets a parser set that limit, so both sides are allowed one.
        except (ValueError, RecursionError, UnicodeDecodeError) as e:
            # `e.msg` rather than `str(e)`: the full text carries "line 1
            # column 5 (char 4)", which makes every rejection its own reason
            # and turns a bucketed report into a list.
            reason = getattr(e, "msg", None) or type(e).__name__
            out.append("ERR " + " ".join(str(reason).split())[:80])
    with open(sys.argv[2], "w", encoding="utf-8") as f:
        f.write("\n".join(out))
        f.write("\n")


if __name__ == "__main__":
    main()
