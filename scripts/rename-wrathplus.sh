#!/usr/bin/env bash
# The one-shot `benilla-*` → `wrathplus-*` rename (docs/WRATHPLUS-RENAME-PLAN.md, policy in
# docs/WRATHPLUS-RENAME.md). Re-runnable and idempotent: it moves only paths that still carry an
# old name and substitutes only old tokens, so a second run changes nothing.
#
#   scripts/rename-wrathplus.sh            # dry run (default): what would move and change
#   scripts/rename-wrathplus.sh --apply    # git mv + substitutions in the working tree
#   scripts/rename-wrathplus.sh --apply --live-dir <dir>   # also the untracked launch scripts
#
# It never commits, never touches uppercase `BENILLA_*` (environment, build stamps, Lua globals),
# `benilla-config/`, Lua `benilla_*` registry keys, temp-file prefixes, product-name prose,
# `third_party/`, or the rename documents themselves.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

apply=0
live_dir=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply) apply=1 ;;
    --dry-run) apply=0 ;;
    --live-dir) live_dir=${2:?--live-dir needs a directory}; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

# The 18 suffixed packages; `crates/benilla` (package `benilla`) is the launcher.
crates=(adt app assets blp buildstamp bytes dbc formats m2 mpq protocol srp ui visual wdt wmo world
  worldview)
# Auto-discovered `src/bin` targets that carry the product name (mpqx/blpx are neutral).
tools=(extract auth)

if (( apply )) && [[ -n $(git status --porcelain --untracked-files=no) ]]; then
  echo "rename-wrathplus: the working tree has uncommitted changes; commit or stash first" >&2
  exit 1
fi

run() { if (( apply )); then "$@"; else echo "would: $*"; fi; }

# 1. Paths. Directories first, then the product-named `src/bin` targets inside them.
moves=0
move() {
  local from=$1 to=$2
  [[ -e $from ]] || return 0
  if [[ -e $to ]]; then echo "rename-wrathplus: both $from and $to exist" >&2; exit 1; fi
  run git mv "$from" "$to"
  moves=$((moves + 1))
}
for c in "${crates[@]}"; do move "crates/benilla-$c" "crates/wrathplus-$c"; done
move crates/benilla crates/wrathplus
# In a dry run the directories have not moved, so look under either name.
for dir in crates/wrathplus-formats/src/bin crates/benilla-formats/src/bin \
  crates/wrathplus-protocol/src/bin crates/benilla-protocol/src/bin; do
  [[ -d $dir ]] || continue
  for t in "${tools[@]}" world; do
    for ext in "" .rs; do move "$dir/benilla-$t$ext" "$dir/wrathplus-$t$ext"; done
  done
done

# 2. Contents. Exact old tokens only; `(?![\w-])` keeps `benilla-world` off `benilla-worldview`
# and `benilla_ui` off `benilla_ui_doc`. A recorded command result (`` `cmd`: exit 0 ``) is history and
# keeps the name it ran under.
alt=$(IFS='|'; echo "${crates[*]}")
tool_alt=$(IFS='|'; echo "${tools[*]}")
# shellcheck disable=SC2016 # Perl, not shell, expands these.
perl_rules='
  if (!/: exit \d/) {
  s/\bbenilla-('"$alt"')(?![\w-])/wrathplus-$1/g;
  s/\bbenilla_('"$alt"')(?!\w)/wrathplus_$1/g;
  s/\bbenilla-('"$tool_alt"')(?![\w-])/wrathplus-$1/g;
  s/(\bcrates\/)benilla(?=[\/"\x27\s)`:]|$)/${1}wrathplus/g;
  s/(-p |--package |--bin |\bbin\/|\bdebug\/|\brelease\/|\bplay\/)benilla(?![\w-])/${1}wrathplus/g;
  s/^(\s*name = )"benilla"/$1"wrathplus"/;
  s/(title: )"benilla(?= worldview"|")/$1"wrathplus/g;
  s/"benilla (build |crash report)/"wrathplus $1/g;
  s/"benilla: /"wrathplus: /g;
  }
'
excluded() {
  case "$1" in
    third_party/*|docs/WRATHPLUS-RENAME.md|docs/WRATHPLUS-RENAME-PLAN.md|scripts/rename-wrathplus.sh)
      return 0 ;;
  esac
  return 1
}
changed=0
while IFS= read -r -d '' f; do
  excluded "$f" && continue
  [[ -f $f ]] || continue
  # Binary files are not text to rewrite.
  grep -Iq . "$f" 2>/dev/null || continue
  n=$(perl -ne 'BEGIN { $c = 0 } $o = $_; '"$perl_rules"' $c++ if $_ ne $o; END { print $c }' "$f")
  (( n > 0 )) || continue
  changed=$((changed + 1))
  if (( apply )); then
    perl -pi -e "$perl_rules" "$f"
  else
    echo "would rewrite $n line(s): $f"
  fi
done < <(git ls-files -z)

# 3. The untracked live launchers (`target/azb-live/{launch,probe,cap}.sh`): binary path and
# tracing targets only.
if [[ -n $live_dir ]]; then
  for f in "$live_dir"/*.sh; do
    [[ -f $f ]] || continue
    n=$(perl -ne 'BEGIN { $c = 0 } $o = $_; '"$perl_rules"' $c++ if $_ ne $o; END { print $c }' "$f")
    (( n > 0 )) || continue
    changed=$((changed + 1))
    if (( apply )); then perl -pi -e "$perl_rules" "$f"; else echo "would rewrite $n line(s): $f"; fi
  done
fi

echo "rename-wrathplus: $moves path move(s), $changed file(s) $( (( apply )) && echo rewritten || echo to rewrite)"
if (( apply && (moves + changed) > 0 )); then
  cat <<'EOF'
Next (docs/WRATHPLUS-RENAME-PLAN.md, "After the script"):
  cargo metadata --offline --format-version 1 --filter-platform aarch64-apple-darwin >/dev/null
  scripts/rename-wrathplus.sh                              # idempotence: expect 0 and 0
  then the plan's gates, quality-baseline re-audit and the live binary rebuild.
EOF
fi
