#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""Widen exactly what a crate boundary hides: check one crate, find every item, field, method or
module the compiler calls private (E0603, E0616, E0624, E0451; a `pub use` past its item's
visibility, E0364 and E0365), make its declaration `pub`, and repeat until none is left. The crate
split's move step (scripts/split/move-layers.sh), one layer crate at a time, bottom-up:

  uv run scripts/split/widen.py -p benilla-app-net        # dry run: what it would widen
  uv run scripts/split/widen.py -p benilla-app-net --apply

Python on this machine runs only through `uv run` (stdlib only; no environment to install).

A declaration is widened in place: `pub(crate)`/`pub(super)`/`pub(in …)` becomes `pub`, and an
unqualified one gains `pub`. A field the compiler names without a definition span is found by its
struct's name in the crates under `crates/benilla-app*/src`. It never commits. At the end it prints
what it widened by error code and the errors that are not privacy (E0116, E0117, …), which a
widening cannot fix, grouped by code.
"""

import argparse
import collections
import json
import os
import re
import subprocess
import sys

PRIVACY = {"E0603", "E0616", "E0624", "E0451", "E0364", "E0365"}
VIS_RE = re.compile(r"\bpub\s*\((?:crate|super|self|in [^)]*)\)\s*")
DECL_RE = re.compile(
    r"^(\s*)(?:#\[[^\]]*\]\s*)*((?:pub(?:\s*\([^)]*\))?\s+)?)"
    r"((?:(?:async|const|unsafe|extern \"C\")\s+)*(?:fn|struct|enum|trait|type|const|static|mod|use|union|macro_rules!)\b)"
)


def check(crate, env):
    """The compiler's diagnostics for one crate, lib and tests."""
    cmd = ["cargo", "check", "-p", crate, "--lib", "--tests", "--message-format", "json"]
    out = subprocess.run(cmd, capture_output=True, text=True, env=env).stdout
    diags = []
    for line in out.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        m = msg["message"]
        if m.get("level") != "error":
            continue
        diags.append(m)
    return diags


def code_of(m):
    return (m.get("code") or {}).get("code") or "other"


def def_span(m):
    """The span a privacy error points at as the item's definition, if it gives one."""
    for child in m.get("children", []):
        for sp in child.get("spans", []):
            if "defined here" in (child.get("message") or "") or (sp.get("label") or "").endswith("defined here"):
                return sp["file_name"], sp["line_start"]
    for sp in m.get("spans", []):
        if not sp.get("is_primary") and "defined here" in (sp.get("label") or ""):
            return sp["file_name"], sp["line_start"]
    return None


def find_reexported(root, d):
    """An item a `pub use` re-exports past its own visibility (E0364, E0365): its declaration,
    searched under the re-exporting file's directory by the name the message gives."""
    m = re.match(r"`(\w+)`", d["message"])
    sp = next((s for s in d.get("spans", []) if s.get("is_primary")), None)
    if not m or not sp:
        return None
    decl = re.compile(
        r"^\s*pub\s*\([^)]*\)\s+(?:const\s+|unsafe\s+|async\s+)*(?:fn|const|static|struct|enum|type|trait|mod|union)\s+"
        + re.escape(m.group(1))
        + r"\b"
    )
    base = os.path.dirname(os.path.join(root, sp["file_name"]))
    for dp, _, fs in os.walk(base):
        for f in sorted(fs):
            if not f.endswith(".rs"):
                continue
            path = os.path.join(dp, f)
            for i, l in enumerate(open(path).read().split("\n")):
                if decl.match(l):
                    return os.path.relpath(path, root), i + 1
    return None


def find_field(root, struct, field):
    """`field`'s declaration line inside `struct`'s body, searched in the layer crates."""
    pat = re.compile(r"\b(?:struct|enum|union)\s+" + re.escape(struct) + r"\b")
    for base in sorted(os.listdir(os.path.join(root, "crates"))):
        if not base.startswith("benilla-app"):
            continue
        for dp, _, fs in os.walk(os.path.join(root, "crates", base, "src")):
            for f in fs:
                if not f.endswith(".rs"):
                    continue
                path = os.path.join(dp, f)
                lines = open(path).read().split("\n")
                for i, l in enumerate(lines):
                    if pat.search(l):
                        depth = 0
                        for j in range(i, min(i + 400, len(lines))):
                            depth += lines[j].count("{") - lines[j].count("}")
                            if j > i and re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?" + re.escape(field) + r"\s*:", lines[j]):
                                return os.path.relpath(path, root), j + 1
                            if j > i and depth <= 0:
                                break
    return None


def widen_line(root, rel, line_no):
    """Make the declaration starting at `line_no` (or the next declaration below its attributes)
    `pub`; `True` when the file changed."""
    path = os.path.join(root, rel)
    lines = open(path).read().split("\n")
    for k in range(line_no - 1, min(line_no + 8, len(lines))):
        l = lines[k]
        if l.strip().startswith("#[") or l.strip().startswith("///") or not l.strip():
            continue
        if VIS_RE.search(l):
            new = VIS_RE.sub("pub ", l, count=1)
        elif re.match(r"\s*pub\s", l):
            return False
        else:
            mfield = re.match(r"(\s*)([a-z_][a-z0-9_]*\s*:)", l)
            mdecl = DECL_RE.match(l)
            if mdecl:
                new = l[: mdecl.start(3)] + "pub " + l[mdecl.start(3):]
            elif mfield:
                new = mfield.group(1) + "pub " + l[len(mfield.group(1)):]
            else:
                new = re.sub(r"^(\s*)", r"\1pub ", l, count=1)
        if new != l:
            lines[k] = new
            open(path, "w").write("\n".join(lines))
            return True
        return False
    return False


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("-p", "--package", required=True)
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--rounds", type=int, default=12)
    args = ap.parse_args()
    root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
    os.chdir(root)
    env = dict(os.environ)
    widened = collections.Counter()
    seen = set()
    for rnd in range(args.rounds):
        diags = check(args.package, env)
        priv = [d for d in diags if code_of(d) in PRIVACY]
        if not priv:
            break
        progress = 0
        for d in priv:
            span = def_span(d)
            if span is None and code_of(d) in ("E0364", "E0365"):
                span = find_reexported(root, d)
            if span is None and code_of(d) in ("E0616", "E0451"):
                mm = re.search(r"field[s]? `([^`]+)`(?:.*?)of (?:struct|union) `(?:[\w:]*::)?(\w+)", d["message"])
                if mm:
                    for field in re.findall(r"`([^`]+)`", d["message"].split(" of ")[0]):
                        span = find_field(root, mm.group(2), field)
                        if span and (span not in seen):
                            seen.add(span)
                            if args.apply and widen_line(root, *span):
                                progress += 1
                            widened[code_of(d)] += 1
                    continue
            if span is None:
                print(f"no definition span: {d['message']}", file=sys.stderr)
                continue
            key = (os.path.relpath(os.path.join(root, span[0]), root), span[1])
            if key in seen:
                continue
            seen.add(key)
            widened[code_of(d)] += 1
            if args.apply and widen_line(root, *key):
                progress += 1
        print(f"round {rnd + 1}: {len(priv)} privacy errors, {progress} declarations widened", file=sys.stderr)
        if not args.apply or progress == 0:
            break
    diags = check(args.package, env)
    rest = collections.Counter(code_of(d) for d in diags if code_of(d) not in PRIVACY)
    left = sum(1 for d in diags if code_of(d) in PRIVACY)
    verb = "widened" if args.apply else "would widen (first round)"
    print(f"{args.package}: {verb} {len(seen)} declarations ({dict(widened)}); privacy errors left {left}")
    print(f"{args.package}: other errors {dict(rest)}")
    for d in diags:
        if code_of(d) not in PRIVACY:
            sp = next((s for s in d.get("spans", []) if s.get("is_primary")), None)
            where = f"{sp['file_name']}:{sp['line_start']}" if sp else "?"
            print(f"  {code_of(d)} {where}: {d['message'][:140]}")


if __name__ == "__main__":
    main()
