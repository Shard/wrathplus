#!/usr/bin/env bash
# Widen every layer crate, bottom-up, until a whole pass widens nothing: widening an upper crate's
# needs can make a lower crate's public signature name a still crate-private type, which the next
# pass fixes. The crate split's move step, after scripts/split/move-layers.sh:
#
#   scripts/split/widen-all.sh [max passes, default 4]
#
# Then `cargo check -p <crate> --lib --tests` each; what is left is not a visibility question.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
max="${1:-4}"

# A glob import (`use crate::ui_chat::feed::*;`) from another crate silently skips a crate-private
# item, so no privacy error names it: widen every `pub(crate)` in each module a glob from another
# crate reads.
g=0
for user in crates/benilla-app*/src; do
    for mp in $(grep -rhoE '^\s*(pub(\([^)]*\))?\s+)?use crate::[a-z_0-9:]+::\*;' "$user" | sed -E 's/.*use crate::([a-z_0-9:]+)::\*;/\1/' | sort -u); do
        rel="${mp//:://}"
        for def in crates/benilla-app*/src; do
            [ "$def" = "$user" ] && continue
            for f in "$def/$rel.rs" "$def/$rel/mod.rs"; do
                [ -f "$f" ] && grep -q 'pub(crate)' "$f" || continue
                perl -pi -e 's/\bpub\(crate\)/pub/g' "$f"
                g=$((g + 1))
            done
        done
    done
done
echo "globs: widened $g module files other crates glob-import"
for pass in $(seq 1 "$max"); do
    total=0
    for p in benilla-app-core benilla-app-net benilla-app-world benilla-app-ui benilla-app-tools benilla-app; do
        [ -f "crates/$p/Cargo.toml" ] || continue
        n="$(uv run scripts/split/widen.py -p "$p" --apply 2>/dev/null | sed -nE 's/^[^:]+: widened ([0-9]+) declarations.*/\1/p')"
        echo "pass $pass: $p widened ${n:-0}"
        total=$((total + ${n:-0}))
    done
    [ "$total" = 0 ] && { echo "fixpoint after $pass pass(es)"; exit 0; }
done
echo "no fixpoint after $max passes"
exit 1
