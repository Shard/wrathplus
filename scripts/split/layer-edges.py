#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""Upward edges under scripts/split/layers.txt, from source text and without cargo: every
`crate::<module>` a module names whose layer sits above its own (core < net < world < ui < tools <
root). Paths through a re-export count where they point, so it reads the tree after
`canonical-paths.py --apply`; scripts/split/move-layers.sh runs it there and stops on any edge.
Before a dry run, on a scratch tree:

  uv run scripts/split/canonical-paths.py --apply && uv run scripts/split/layer-edges.py
  git checkout -- crates/

Every `ui_*` module layers.txt does not list is ui; the rest of lib.rs is the root. Comments are
skipped; `#[cfg(test)]` code is not, since a dependent crate's tests cannot reach upward either.
"""

import os
import re
import subprocess
import sys

ORDER = ["core", "net", "world", "ui", "tools", "root"]


def main():
    root = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
    crates = os.path.join(root, "crates")
    app = "wrathplus-app" if os.path.isdir(os.path.join(crates, "wrathplus-app")) else "wrathplus-app"
    src = os.path.join(crates, app, "src")
    layer = {}
    for line in open(os.path.join(root, "scripts", "split", "layers.txt")):
        if ":" in line and not line.lstrip().startswith("#"):
            name, mods = line.split(":", 1)
            for m in mods.split():
                layer[m] = name.strip()
    lib = open(os.path.join(src, "lib.rs")).read()
    for m in re.findall(r"^\s*(?:pub(?:\(crate\))?\s+)?mod\s+(\w+);", lib, re.M):
        layer.setdefault(m, "ui" if m.startswith("ui_") else "root")
    edges = []
    for dp, _, fs in os.walk(src):
        for f in sorted(fs):
            if not f.endswith(".rs"):
                continue
            path = os.path.join(dp, f)
            rel = os.path.relpath(path, src)
            own = layer.get(rel.split(os.sep)[0].removesuffix(".rs"), "root")
            for n, text in enumerate(open(path).read().split("\n"), 1):
                code = text.split("//")[0]
                for target in re.findall(r"\bcrate::(\w+)", code):
                    up = layer.get(target)
                    if up and ORDER.index(up) > ORDER.index(own):
                        edges.append(f"{app}/src/{rel}:{n} ({own}) -> crate::{target} ({up})")
    for e in edges:
        print(e)
    print(f"layer-edges: {len(edges)} upward edge(s)", file=sys.stderr)
    sys.exit(1 if edges else 0)


if __name__ == "__main__":
    main()
