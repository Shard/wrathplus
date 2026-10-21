#!/usr/bin/env bash
# Move benilla-app's modules into one crate per layer (docs/APP-CRATE-SPLIT-PLAN.md), in the
# checkout $PWD is in. The layers and their modules are scripts/split/layers.txt; every ui_*
# module not listed elsewhere is ui; the rest stays in benilla-app, the root.
#
#   scripts/split/move-layers.sh [--layers "core net"] [--drop-reexports] [--widen]
#
# For each layer, in dependency order: crates/benilla-app-<layer> with benilla-app's package,
# dependencies and features, a path dependency on every lower layer crate (and a dev-dependency
# with `test-support`, so a lower crate's test helpers reach this crate's tests), its modules
# `git mv`'d with their `#[path]` siblings, and a lib.rs that declares them `pub mod` and
# re-exports every lower layer's modules at its root (`pub(crate) use benilla_app_core::net;`),
# so a moved `crate::net::…` path still resolves. The root declares `pub use benilla_app_x::m;`
# in place of each moved `mod m;` and depends on every layer crate, forwarding `dev` and
# `test-support`.
#
# First, `canonical-paths.py --apply` rewrites every path through a re-export to its home;
# `--drop-reexports` then deletes each module's re-exports (a glob `use super::*` that relied on
# one breaks; the dry run counts them). `--widen` turns every `pub(crate)` in the layer crates
# into `pub`, the blanket widening; without it the compiler names each item a crate boundary hides.
#
# It never commits and never builds. Run it on a throwaway worktree for a dry run, or on the move
# commit's tree, then `cargo check` bottom-up: `-p benilla-app-core`, `-net`, `-world`, `-ui`,
# `-tools`, then `-p benilla-app`, each `--lib --tests`.
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
cd "$root"
app=crates/benilla-app
src=$app/src
layers="core net world ui tools"
drop=0
widen=0
while [ $# -gt 0 ]; do
    case "$1" in
        --layers) layers="$2"; shift 2 ;;
        --drop-reexports) drop=1; shift ;;
        --widen) widen=1; shift ;;
        *) echo "usage: $0 [--layers \"core net …\"] [--drop-reexports] [--widen]" >&2; exit 2 ;;
    esac
done

mods_of() { grep "^$1:" scripts/split/layers.txt | sed "s/^$1: //"; }
all_mods="$(grep -E '^(pub(\(crate\))? )?mod [a-z_0-9]+;' $src/lib.rs | sed -E 's/.*mod ([a-z_0-9]+);/\1/')"
listed=" $(for l in core net world ui tools; do mods_of $l; done | tr '\n' ' ') "
extra_ui=""
for m in $all_mods; do
    case $m in ui_*) [[ "$listed" == *" $m "* ]] || extra_ui+=" $m" ;; esac
done
# A layer's modules: its line, and for ui every unlisted ui_* module.
of() { mods_of "$1"; [ "$1" = ui ] && echo "$extra_ui"; true; }
# The `#[cfg(…)]` line above a module's declaration in the root lib.rs, if any.
cfg_of() { grep -B1 -E "^(pub(\(crate\))? )?mod $1;" $src/lib.rs | head -1 | grep '^#\[cfg' || true; }

uv run scripts/split/canonical-paths.py --apply
if [ $drop = 1 ]; then
    for m in $all_mods; do
        uv run scripts/split/canonical-paths.py --via "$m" --drop-reexports --apply 2>/dev/null || true
    done
fi

lower=""   # the layers already moved, lowest first
for l in $layers; do
    crate=benilla-app-$l
    dst=crates/$crate
    mkdir -p "$dst/src"
    # Cargo.toml: benilla-app's, renamed, with the lower crates as dependencies.
    deps=""; devdeps=""; devf=""; tsf=""
    for b in $lower; do
        deps+="benilla-app-$b = { path = \"../benilla-app-$b\", default-features = false }"$'\n'
        devdeps+="benilla-app-$b = { path = \"../benilla-app-$b\", default-features = false, features = [\"test-support\"] }"$'\n'
        devf+=", \"benilla-app-$b/dev\""
        tsf+="\"benilla-app-$b/test-support\", "
    done
    CRATE="$crate" DEVF="$devf" TSF="${tsf%, }" DEPS="$deps" DEVDEPS="$devdeps" perl -0pe '
        s/^name = "benilla-app"/name = "$ENV{CRATE}"/m;
        s/^description = [^\n]*\n//m;
        s/^(dev = \[[^\]]*)\]/$1$ENV{DEVF}]/m;
        s/^test-support = \[\]/test-support = [$ENV{TSF}]/m;
        s/^\[dependencies\]\n/[dependencies]\n$ENV{DEPS}/m;
        s/^\[dev-dependencies\]\n/[dev-dependencies]\n$ENV{DEVDEPS}/m' $app/Cargo.toml > "$dst/Cargo.toml"
    {
        echo "//! The $l layer of benilla-app: see docs/APP-CRATE-SPLIT-PLAN.md."
        echo '#![cfg_attr(not(feature = "dev"), allow(dead_code))]'
        echo
    } > "$dst/src/lib.rs"
    for m in $(of $l); do
        c="$(cfg_of "$m")"
        [ -n "$c" ] && echo "$c" >> "$dst/src/lib.rs"
        echo "pub mod $m;" >> "$dst/src/lib.rs"
        # The module's files, and the `#[path]` siblings its root file names at src/.
        for f in $src/$m.rs $(find $src/$m -name '*.rs' 2>/dev/null); do
            [ -f "$f" ] || continue
            { grep -oE '#\[path = "[^"/]+\.rs"\]' "$f" || true; } | sed -E 's/#\[path = "(.*)"\]/\1/' | while read -r p; do
                d="$(dirname "$f")"
                [ "$d" = "$src" ] && [ -f "$d/$p" ] && git mv "$d/$p" "$dst/src/$p" || true
            done
        done
        [ -f $src/$m.rs ] && git mv $src/$m.rs "$dst/src/$m.rs"
        [ -d $src/$m ] && git mv $src/$m "$dst/src/$m"
        perl -0pi -e "s/^(?:pub(?:\(crate\))? )?mod $m;\n/pub use benilla_app_$l::$m;\n/m" $src/lib.rs
    done
    if [ -n "$lower" ]; then
        { echo; echo "// Every lower layer's modules at this crate's root, so \`crate::<module>::…\` resolves."; } >> "$dst/src/lib.rs"
        for b in $lower; do
            for m in $(of $b); do
                c="$(git show HEAD:$src/lib.rs | grep -B1 -E "^(pub(\(crate\))? )?mod $m;" | head -1 | grep '^#\[cfg' || true)"
                [ -n "$c" ] && echo "$c" >> "$dst/src/lib.rs"
                echo "#[allow(unused_imports)]" >> "$dst/src/lib.rs"
                echo "pub(crate) use benilla_app_${b}::$m;" >> "$dst/src/lib.rs"
            done
        done
    fi
    git add "$dst"
    lower+=" $l"
done

# The root depends on every layer crate and forwards its features.
deps=""; devf=""; tsf=""
for b in $lower; do
    deps+="benilla-app-$b = { path = \"../benilla-app-$b\", default-features = false }"$'\n'
    devf+=", \"benilla-app-$b/dev\""
    tsf+="\"benilla-app-$b/test-support\", "
done
DEVF="$devf" TSF="${tsf%, }" DEPS="$deps" perl -0pi -e '
    s/^(dev = \[[^\]]*)\]/$1$ENV{DEVF}]/m;
    s/^test-support = \[\]/test-support = [$ENV{TSF}]/m;
    s/^\[dependencies\]\n/[dependencies]\n$ENV{DEPS}/m' $app/Cargo.toml

if [ $widen = 1 ]; then
    for b in $lower; do
        find crates/benilla-app-$b/src -name '*.rs' -exec perl -pi -e 's/\bpub\(crate\)/pub/g' {} +
    done
fi
echo "moved: $(for b in $lower; do echo -n "$b=$(echo $(of $b) | wc -w | tr -d ' ') "; done)"
