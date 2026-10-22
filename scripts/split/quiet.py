#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""The crate split's warnings, after scripts/split/widen-all.sh: check each app crate's lib and its
test build, then

- remove each unused import the test build also calls unused, and move one only the lib build
  calls unused (tests still use it) to a `#[cfg(test)]` import of the same path and visibility,
  right after its statement, as the codebase's "Test-only" re-exports do;
- give `#[cfg(test)]` to what only the `test-support` lib build flags (the build a dependent
  crate's tests compile): a helper in an opened block that only its own crate's tests use, dead
  there, with its struct's `impl`s; an import only such helpers use moves to a test-only import;
- widen the type a `private_interfaces` warning names ("more private than the item"), never
  narrowing the item;
- list everything else (dead code above all) as `file:line lint message`, for a reviewed
  deletion: nothing is deleted here;
- last, check the player build (`--no-default-features --lib --tests`, what `cargo play`'s
  players run without the instruments) and list its warnings too: it fixes none, and any makes
  the exit status 1.

  uv run scripts/split/quiet.py              # dry run: what it would do
  uv run scripts/split/quiet.py --apply      # until a pass changes nothing

Python on this machine runs only through `uv run` (stdlib only). It never commits.
"""

import argparse
import collections
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import widen  # noqa: E402  (widen_line, app_prefix)

LAYERS = ["core", "net", "world", "ui", "tools"]


def warnings(crate, test, features=None, player=False):
    """`(file, line, lint, message)` -> the compiler message, for the warnings in `crate`'s own
    files from its lib build (`test=False`), with `features` if given, or its test build; `player`
    checks the lib and tests without default features (no `dev`)."""
    cmd = ["cargo", "check", "-p", crate, "--lib", "--message-format", "json"]
    if test:
        cmd += ["--profile", "test"]
    if features:
        cmd += ["--features", features]
    if player:
        cmd += ["--tests", "--no-default-features"]
    out = subprocess.run(cmd, capture_output=True, text=True).stdout
    own = f"crates/{crate}/"
    found = {}
    for line in out.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        m = msg.get("message") or {}
        if msg.get("reason") != "compiler-message" or m.get("level") != "warning":
            continue
        sp = next((s for s in m.get("spans", []) if s.get("is_primary")), None)
        if not sp or not sp["file_name"].startswith(own):
            continue
        lint = (m.get("code") or {}).get("code") or "other"
        found[(sp["file_name"], sp["line_start"], lint, m["message"])] = m
    return found


def removal_edits(m):
    """The byte ranges the compiler's machine-applicable suggestion replaces, with their text."""
    edits = []
    for child in m.get("children", []):
        for sp in child.get("spans", []):
            if sp.get("suggestion_applicability") == "MachineApplicable":
                edits.append((sp["file_name"], sp["byte_start"], sp["byte_end"], sp["suggested_replacement"]))
    return edits


def apply_edits(root, edits):
    """Apply byte-range edits, last first per file, skipping overlaps; the number applied."""
    by_file = collections.defaultdict(list)
    for f, a, b, text in set(edits):
        by_file[f].append((a, b, text))
    n = 0
    for f, es in by_file.items():
        path = os.path.join(root, f)
        data = open(path, "rb").read()
        floor = len(data) + 1
        for a, b, text in sorted(es, reverse=True):
            if b > floor:
                continue
            data = data[:a] + text.encode() + data[b:]
            floor = a
            n += 1
        open(path, "wb").write(data)
    return n


def test_only_edits(root, m):
    """Edits moving the names a lib-only `unused_imports` warning flags into a `#[cfg(test)]`
    import after their statement: the compiler's removal, plus the insertion. `[]` for a statement
    this does not parse (a nested group), which is then listed instead."""
    prim = [s for s in m.get("spans", []) if s.get("is_primary")]
    removal = removal_edits(m)
    if not prim or not removal:
        return []
    f = prim[0]["file_name"]
    data = open(os.path.join(root, f), "rb").read()
    # The statement: its `use` line back from the first name, through its `;`.
    start = data.rfind(b"\n", 0, prim[0]["byte_start"]) + 1
    while not re.match(rb"\s*(?:pub(?:\([^)]*\))?\s+)?use\b", data[start:start + 200]):
        if start == 0:
            return []
        start = data.rfind(b"\n", 0, start - 1) + 1
    end = data.index(b";", prim[0]["byte_end"]) + 1
    stmt = data[start:end].decode()
    head = re.match(r"(\s*)((?:pub(?:\([^)]*\))?\s+)?)use\s+", stmt)
    names = [data[s["byte_start"]:s["byte_end"]].decode() for s in prim]
    body = stmt[head.end():-1]
    if "{" in body:
        prefix, group = body.split("{", 1)
        if "{" in group:
            return []  # a nested group
        path = prefix + ("{" + ", ".join(names) + "}" if len(names) > 1 else names[0])
    else:
        path = body
    line_end = data.index(b"\n", end - 1) + 1 if b"\n" in data[end - 1:] else len(data)
    insert = f"{head.group(1)}#[cfg(test)]\n{head.group(1)}{head.group(2)}use {path};\n"
    return removal + [(f, line_end, line_end, insert)]


def test_gate_edits(root, m):
    """Insertions of `#[cfg(test)]` above each item a dead-code warning names, and above the
    same file's `impl`s of a struct, enum or union among them."""
    edits = []
    for sp in (s for s in m.get("spans", []) if s.get("is_primary")):
        f = sp["file_name"]
        data = open(os.path.join(root, f), "rb").read()
        lines = data.split(b"\n")
        offsets = [0]
        for l in lines:
            offsets.append(offsets[-1] + len(l) + 1)
        i = sp["line_start"] - 1
        indent = re.match(rb"\s*", lines[i]).group(0).decode()
        edits.append((f, offsets[i], offsets[i], f"{indent}#[cfg(test)]\n"))
        kind = re.search(rb"\b(struct|enum|union)\s+(\w+)", lines[i])
        if kind:
            name = re.escape(kind.group(2))
            for j, l in enumerate(lines):
                if re.match(rb"\s*impl\b(?:<[^>]*>)?\s+(?:[\w:<>, ]+\s+for\s+)?" + name + rb"\b", l):
                    ind = re.match(rb"\s*", l).group(0).decode()
                    edits.append((f, offsets[j], offsets[j], f"{ind}#[cfg(test)]\n"))
    return edits


def private_type_span(m):
    """The definition of the type a `private_interfaces` warning says is too private."""
    for child in m.get("children", []):
        if "only usable at visibility" in (child.get("message") or ""):
            for sp in child.get("spans", []):
                return sp["file_name"], sp["line_start"]
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--passes", type=int, default=4)
    args = ap.parse_args()
    root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
    os.chdir(root)
    app = widen.app_prefix(root)
    crates = [c for c in [f"{app}-{l}" for l in LAYERS] + [app] if os.path.isfile(f"crates/{c}/Cargo.toml")]
    left = {}
    for npass in range(1, args.passes + 1):
        changed = 0
        left = {}
        for crate in crates:
            lib, test = warnings(crate, False), warnings(crate, True)
            feat = warnings(crate, False, "test-support")
            edits, kept = [], []
            # Only the test-support lib build compiles these: their own crate's tests use them.
            for key, m in feat.items():
                if key in lib:
                    continue
                if key[2] == "dead_code":
                    edits += test_gate_edits(root, m)
                elif key[2] == "unused_imports" and (moved := test_only_edits(root, m)):
                    edits += moved
                else:
                    kept.append(key)
            for key, m in {**lib, **test}.items():
                lint = key[2]
                if lint == "unused_imports":
                    if key in test:
                        edits += removal_edits(m)
                    elif moved := test_only_edits(root, m):
                        edits += moved  # unused in the lib build only: tests use it
                    else:
                        kept.append(key)
                elif lint == "private_interfaces" and (span := private_type_span(m)):
                    if args.apply and widen.widen_line(root, *span):
                        changed += 1
                    else:
                        kept.append(key)
                else:
                    kept.append(key)
            if args.apply and edits:
                changed += apply_edits(root, edits)
            print(f"pass {npass}: {crate}: {len(lib)} lib / {len(feat)} test-support / {len(test)} test warnings, "
                  f"{len(edits)} import removals, {len(kept)} left", file=sys.stderr)
            for key in kept:
                left[key] = crate
        if not args.apply or changed == 0:
            break
    player = {}
    for crate in crates:
        found = warnings(crate, False, player=True)
        print(f"player build: {crate}: {len(found)} warnings", file=sys.stderr)
        player.update(found)
    for (f, line, lint, message), crate in sorted(left.items()):
        print(f"{f}:{line} {lint} {message}")
    for f, line, lint, message in sorted(player):
        print(f"{f}:{line} {lint} (player build) {message}")
    print(f"quiet: {len(left)} warnings left, {len(player)} in the player build", file=sys.stderr)
    sys.exit(1 if player else 0)


if __name__ == "__main__":
    main()
