#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""Widen exactly what a crate boundary hides: check one crate, find every item, field, method or
module the compiler calls private (E0603, E0616, E0624, E0451; a `pub use` past its item's
visibility, E0364 and E0365), make its declaration `pub`, and repeat until none is left. A
`#[cfg(test)]` item another crate's code reaches (the compiler's "found an item that was configured
out ... the item is gated here") is opened to `#[cfg(any(test, feature = "test-support"))]`: only
what is reached, so a helper only its own crate's tests use stays `cfg(test)`. The crate
split's move step (scripts/split/move-layers.sh), one layer crate at a time, bottom-up:

  uv run scripts/split/widen.py -p wrathplus-app-net        # dry run: what it would widen
  uv run scripts/split/widen.py -p wrathplus-app-net --apply

Python on this machine runs only through `uv run` (stdlib only; no environment to install).

A declaration is widened in place: `pub(crate)`/`pub(super)`/`pub(in …)` becomes `pub`, and an
unqualified one gains `pub`. A field the compiler names without a definition span is found by its
struct's name in the crates under `crates/wrathplus-app*/src`. It never commits. At the end it prints
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

PRIVACY = {"E0603", "E0616", "E0624", "E0451", "E0364", "E0365", "E0423", "private-type"}
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
    code = (m.get("code") or {}).get("code")
    if not code and re.match(r"type `[^`]+` is private", m.get("message", "")):
        return "private-type"
    return code or "other"


TEST_GATE = "#[cfg(test)]"
OPEN_GATE = '#[cfg(any(test, feature = "test-support"))]'


def gate_spans(m):
    """The `#[cfg(test)]` lines gating the items an error found configured out."""
    out = []
    for child in m.get("children", []):
        if "configured out" not in (child.get("message") or ""):
            continue
        for sp in child.get("spans", []):
            if (sp.get("label") or "") == "the item is gated here":
                out.append((sp["file_name"], sp["line_start"]))
    return out


UNRESOLVED = {"E0425", "E0412", "E0422", "E0432", "E0433", "E0531", "E0599"}


def find_gated(root, m):
    """A lookup in the same scope, a method call (E0599) or a constructor function sharing a type
    alias's name (E0423 "found type alias") reports no "configured out" note: find the
    `#[cfg(test)]` gating a declaration of the name the error cannot find (its own attribute or its
    `impl` block's), in the error's own file first, then its crate, then the other app crates. A
    method's hits are narrowed to `impl`s of the type the error names; `None` unless exactly one
    turns up at the first level that has any."""
    code = (m.get("code") or {}).get("code")
    msg = m.get("message", "")
    if code not in UNRESOLVED and not (code == "E0423" and "found type alias" in msg):
        return None
    names = re.findall(r"`(?:[\w:]*::)?(\w+)", msg)
    if code == "E0423":
        names = re.findall(r"found type alias `(?:[\w:]*::)?(\w+)`", msg)
    sp = next((s for s in m.get("spans", []) if s.get("is_primary")), None)
    if not names or not sp:
        return None
    name = names[0]
    owner = None
    if code == "E0599":
        # The type the method was looked up on, through ECS and smart-pointer wrappers:
        # `Mut<'_, LetterTexts>` is `LetterTexts`'s.
        quoted = re.findall(r"`([^`]*)`", msg)
        wrappers = {"Mut", "Ref", "Res", "ResMut", "Box", "Rc", "Arc", "Option", "NonSend", "NonSendMut"}
        types = [t for t in re.findall(r"\b([A-Z]\w*)", quoted[1] if len(quoted) > 1 else "") if t not in wrappers]
        owner = types[0] if types else None
    decl = re.compile(r"^(\s*)(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:const|unsafe|async)\s+)*"
                      r"(?:fn|struct|enum|type|trait|const|static|mod|union)\s+" + re.escape(name) + r"\b")

    def gate_above(lines, i):
        k = i - 1
        while k >= 0 and (lines[k].strip().startswith("#[") or lines[k].strip().startswith("///")):
            if lines[k].strip() == TEST_GATE:
                return k + 1
            k -= 1
        return None

    def scan(paths):
        hits = []
        for path in paths:
            lines = open(os.path.join(root, path)).read().split("\n")
            for i, l in enumerate(lines):
                d = decl.match(l)
                if not d:
                    # A gated `pub use` re-export of the name (a glob import reaches it).
                    if (code != "E0599" and re.match(r"^\s*pub(?:\s*\([^)]*\))?\s+use\b", l)
                            and re.search(r"\b" + re.escape(name) + r"\b", l) and gate_above(lines, i)):
                        hits.append((path, gate_above(lines, i)))
                    continue
                # The enclosing `impl`, if the declaration is indented inside one.
                impl = None
                for j in range(i - 1, -1, -1):
                    mi = re.match(r"^(\s*)impl\b", lines[j])
                    if mi and len(mi.group(1)) < len(d.group(1)):
                        impl = j
                        break
                if owner and (impl is None or not re.search(r"\b" + re.escape(owner) + r"\b", lines[impl])):
                    continue
                g = gate_above(lines, i) or (impl is not None and gate_above(lines, impl))
                if g:
                    hits.append((path, g))
        return list(dict.fromkeys(hits))

    def rs(top):
        return [os.path.relpath(os.path.join(dp, f), root) for dp, _, fs in os.walk(os.path.join(root, top))
                for f in fs if f.endswith(".rs")]

    # A `#[cfg(test)] use` of the name in the error's own file: imports are file-local.
    if code in UNRESOLVED and code != "E0599":
        lines = open(os.path.join(root, sp["file_name"])).read().split("\n")
        for i, l in enumerate(lines):
            if re.match(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?use\b", l) and re.search(r"\b" + re.escape(name) + r"\b", l):
                g = gate_above(lines, i)
                if g:
                    return sp["file_name"], g

    crate_src = "/".join(sp["file_name"].split("/")[:3])  # crates/<crate>/src
    app = app_prefix(root)
    others = [p for base in sorted(os.listdir(os.path.join(root, "crates"))) if base.startswith(app)
              and f"crates/{base}/src" != crate_src for p in rs(f"crates/{base}/src")]
    for paths in ([sp["file_name"]], rs(crate_src), others):
        hits = scan(paths)
        if hits:
            return hits[0] if len(hits) == 1 else None
    return None


def open_gate(root, rel, line_no):
    """Turn the `#[cfg(test)]` at `line_no` into the test-support gate; `True` when it changed.
    Any other gate (a `dev` feature, a platform) is not this tool's to open."""
    path = os.path.join(root, rel)
    lines = open(path).read().split("\n")
    if lines[line_no - 1].strip() != TEST_GATE:
        return False
    lines[line_no - 1] = lines[line_no - 1].replace(TEST_GATE, OPEN_GATE)
    open(path, "w").write("\n".join(lines))
    return True


def def_spans(m):
    """Every span a privacy error points at as the item's definition, most direct first: a tuple
    struct's field, then the "defined here" notes (which may name only a re-export of it)."""
    out = []
    for sp in m.get("spans", []):
        if "private if any of the fields" in (sp.get("label") or ""):
            out.append((sp["file_name"], sp["line_start"]))
    for child in m.get("children", []):
        for sp in child.get("spans", []):
            msg = child.get("message") or ""
            label = sp.get("label") or ""
            if "defined here" in msg or label.endswith("defined here") or label == "private field" or "not visible here" in msg:
                out.append((sp["file_name"], sp["line_start"]))
    for sp in m.get("spans", []):
        label = sp.get("label") or ""
        if not sp.get("is_primary") and "defined here" in label:
            out.append((sp["file_name"], sp["line_start"]))
    return list(dict.fromkeys(out))


def def_span(m):
    """The most direct of [`def_spans`], if any."""
    spans = def_spans(m)
    return spans[0] if spans else None


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
    # Or a narrower re-export of it, `pub(crate) use …::{…, NAME, …}`, one hop down a chain.
    reuse = re.compile(r"^[ \t]*pub\s*\([^)]*\)\s+use\b[^;]*?\b" + re.escape(m.group(1)) + r"\b[^;]*;", re.M)
    base = os.path.dirname(os.path.join(root, sp["file_name"]))
    for dp, _, fs in os.walk(base):
        for f in sorted(fs):
            if not f.endswith(".rs"):
                continue
            path = os.path.join(dp, f)
            for i, l in enumerate(open(path).read().split("\n")):
                if decl.match(l):
                    return os.path.relpath(path, root), i + 1
    for dp, _, fs in os.walk(base):
        for f in sorted(fs):
            if not f.endswith(".rs"):
                continue
            path = os.path.join(dp, f)
            text = open(path).read()
            hit = reuse.search(text)
            if hit:
                return os.path.relpath(path, root), text.count("\n", 0, hit.start()) + 1
    return None


def app_prefix(root):
    """The root crate's name, the layer crates' prefix: `wrathplus-app` once the rename has run."""
    renamed = os.path.isdir(os.path.join(root, "crates", "wrathplus-app"))
    return "wrathplus-app" if renamed else "wrathplus-app"


def find_type(root, name):
    """A crate-private type a public signature names (`type 'X' is private`), by name in the layer
    crates."""
    decl = re.compile(r"^\s*pub\s*\([^)]*\)\s+(?:struct|enum|type|trait|union)\s+" + re.escape(name) + r"\b")
    for base in sorted(os.listdir(os.path.join(root, "crates"))):
        if not base.startswith(app_prefix(root)):
            continue
        for dp, _, fs in os.walk(os.path.join(root, "crates", base, "src")):
            for f in sorted(fs):
                if f.endswith(".rs"):
                    path = os.path.join(dp, f)
                    for i, l in enumerate(open(path).read().split("\n")):
                        if decl.match(l):
                            return os.path.relpath(path, root), i + 1
    return None


def find_field(root, struct, field):
    """`field`'s declaration line inside `struct`'s body, searched in the layer crates."""
    pat = re.compile(r"\b(?:struct|enum|union)\s+" + re.escape(struct) + r"\b")
    for base in sorted(os.listdir(os.path.join(root, "crates"))):
        if not base.startswith(app_prefix(root)):
            continue
        for dp, _, fs in os.walk(os.path.join(root, "crates", base, "src")):
            for f in fs:
                if not f.endswith(".rs"):
                    continue
                path = os.path.join(dp, f)
                lines = open(path).read().split("\n")
                for i, l in enumerate(lines):
                    if pat.search(l):
                        if field.isdigit():
                            return os.path.relpath(path, root), i + 1
                        depth = 0
                        for j in range(i, min(i + 400, len(lines))):
                            depth += lines[j].count("{") - lines[j].count("}")
                            if j > i and re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?" + re.escape(field) + r"\s*:", lines[j]):
                                return os.path.relpath(path, root), j + 1
                            if j > i and depth <= 0:
                                break
    return None


def widen_line(root, rel, line_no, every=False):
    """Make the declaration starting at `line_no` (or the next declaration below its attributes)
    `pub`; `True` when the file changed. `every` widens each qualifier on the line: a tuple
    struct's fields (E0423)."""
    path = os.path.join(root, rel)
    lines = open(path).read().split("\n")
    if every:
        for k in range(line_no - 1, min(line_no + 8, len(lines))):
            if VIS_RE.search(lines[k]):
                lines[k] = VIS_RE.sub("pub ", lines[k])
                open(path, "w").write("\n".join(lines))
                return True
        return False
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
                # An entry of a multi-line `use` list: widen the statement it belongs to.
                for j in range(k - 1, max(k - 40, -1), -1):
                    if ";" in lines[j]:
                        return False
                    mu = re.match(r"(\s*)(pub\s*\([^)]*\)\s*)?use\b", lines[j])
                    if mu:
                        if mu.group(2):
                            lines[j] = VIS_RE.sub("pub ", lines[j], count=1)
                        else:
                            lines[j] = mu.group(1) + "pub " + lines[j][len(mu.group(1)):]
                        open(path, "w").write("\n".join(lines))
                        return True
                return False  # not a declaration: never guess
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
        gates = list(dict.fromkeys(g for d in diags for g in (gate_spans(d) or [find_gated(root, d)]) if g))
        if not priv and not gates:
            break
        progress = 0
        for g in gates:
            key = (os.path.relpath(os.path.join(root, g[0]), root), g[1])
            if key in seen:
                continue
            seen.add(key)
            if not args.apply or open_gate(root, *key):
                widened["configured-out"] += 1
                progress += args.apply
        for d in priv:
            span = def_span(d)
            if span is None and code_of(d) in ("E0364", "E0365"):
                span = find_reexported(root, d)
            if span is None and code_of(d) == "private-type":
                mt = re.match(r"type `(?:[\w:]*::)?(\w+)(?:<[^`]*>)?` is private", d["message"])
                span = find_type(root, mt.group(1)) if mt else None
            if span is None and code_of(d) in ("E0616", "E0451"):
                mm = re.search(r"field[s]? `([^`]+)`(?:.*?)of (?:struct|union) `(?:[\w:]*::)?(\w+)", d["message"])
                if mm:
                    for field in re.findall(r"`([^`]+)`", d["message"].split(" of ")[0]):
                        span = find_field(root, mm.group(2), field)
                        if span and (span not in seen):
                            seen.add(span)
                            if args.apply and widen_line(root, *span, every=field.isdigit()):
                                progress += 1
                            widened[code_of(d)] += 1
                    continue
            if span is None:
                print(f"no definition span: {d['message']}", file=sys.stderr)
                continue
            # The first candidate that changes a file: a field already `pub` leaves the
            # re-export the note names.
            every = code_of(d) == "E0423" or "tuple struct constructor" in d["message"]
            for cand in [span] + [s for s in def_spans(d) if s != span]:
                key = (os.path.relpath(os.path.join(root, cand[0]), root), cand[1])
                if key in seen:
                    continue
                seen.add(key)
                if not args.apply:
                    widened[code_of(d)] += 1
                    break
                if widen_line(root, *key, every=every):
                    widened[code_of(d)] += 1
                    progress += 1
                    break
        print(f"round {rnd + 1}: {len(priv)} privacy errors, {len(gates)} test gates, {progress} declarations widened or opened", file=sys.stderr)
        if not args.apply or progress == 0:
            break
    diags = check(args.package, env)
    rest = collections.Counter(code_of(d) for d in diags if code_of(d) not in PRIVACY and not gate_spans(d))
    left = sum(1 for d in diags if code_of(d) in PRIVACY)
    verb = "widened" if args.apply else "would widen (first round)"
    print(f"{args.package}: {verb} {sum(widened.values())} declarations ({dict(widened)}); privacy errors left {left}")
    print(f"{args.package}: other errors {dict(rest)}")
    for d in diags:
        if code_of(d) in PRIVACY:
            sp = next((s for s in d.get("spans", []) if s.get("is_primary")), None)
            where = f"{sp['file_name']}:{sp['line_start']}" if sp else "?"
            print(f"  left {code_of(d)} {where}: {d['message'][:120]} -> def {def_span(d)}")
    for d in diags:
        if code_of(d) not in PRIVACY:
            sp = next((s for s in d.get("spans", []) if s.get("is_primary")), None)
            where = f"{sp['file_name']}:{sp['line_start']}" if sp else "?"
            print(f"  {code_of(d)} {where}: {d['message'][:140]}")


if __name__ == "__main__":
    main()
