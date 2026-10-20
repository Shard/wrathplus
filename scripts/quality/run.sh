#!/usr/bin/env bash
# Explicit opt-in only. Heavy commands take the shared lock and own target dir.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
check=${1:?usage: run.sh loc|fmt|clippy|crap|machete|typos [paths or -p packages]}
shift
case "$check" in
loc|fmt) exec python3 scripts/quality/gate.py "$check" "$@" ;;
# The workspace crates only: vendored third_party/ code is not ours to prune.
machete) if [[ $# == 0 ]]; then set -- crates; fi; exec cargo machete "$@" ;;
typos) exec typos "$@" ;;
clippy|crap) ;;
*) echo 'unknown check' >&2; exit 2 ;;
esac
# Each worker builds in its own target directory; never default into another's.
: "${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR to your own target directory (e.g. /tmp/azb-target-<you>)}"
export CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
# Coverage can create a fresh instrumented tree; refuse to start above the shared cap.
if [[ -d $CARGO_TARGET_DIR ]] && (( $(du -sk "$CARGO_TARGET_DIR" | cut -f1) > 41943040 )); then
  echo 'quality: own target exceeds 40 GiB; clean it while idle before running' >&2
  exit 2
fi
# Re-enter once under a heavy-build slot so sequential coverage + scoring remain one operation:
# the machine's two-slot `heavy.sh` where it is installed, else the single lock. A caller that
# already holds a slot (gates.sh, check.sh, CI) sets AZB_QUALITY_LOCKED=1.
if [[ ${AZB_QUALITY_LOCKED:-0} != 1 ]]; then
  heavy=${AZB_HEAVY:-$HOME/Workspace/artivus/azerothbench-client/target/azb-live/heavy.sh}
  if [[ -x $heavy ]]; then
    exec "$heavy" env AZB_QUALITY_LOCKED=1 nice -n 15 "$0" "$check" "$@"
  fi
  exec lockf -k /tmp/azb-heavy.lock env AZB_QUALITY_LOCKED=1 nice -n 15 "$0" "$check" "$@"
fi
# Test assumptions must not accidentally point vanilla fixtures at Wrath data.
unset WOW_CLIENT_PROFILE WOW_LOCALE
export WOW_DATA=
if [[ $# == 0 ]]; then set -- --workspace; fi
report=$(mktemp -d /tmp/azb-quality-report.XXXXXX)
echo "quality artifacts: $report" >&2
if [[ $check == clippy ]]; then
  # Every warning fails unless the exact committed allowlist names it. Warnings mode, not
  # `-D warnings`: a promoted warning stops its dependents' check, hiding their diagnostics.
  set +e
  cargo clippy --locked --all-targets --message-format=json "$@" > "$report/clippy.json"
  status=$?
  set -e
  python3 scripts/quality/gate.py clippy-report "$report/clippy.json"
  # Warnings do not fail cargo here, so any failure is an error or a tool/config failure.
  if (( status != 0 )); then exit "$status"; fi
else
  for tool in cargo-llvm-cov cargo-crap; do command -v "$tool" >/dev/null; done
  # cargo-llvm-cov otherwise stops at an interactive install prompt, hanging an unattended run.
  if ! rustup component list --installed 2>/dev/null | grep -q '^llvm-tools'; then
    echo 'quality: crap needs `rustup component add llvm-tools-preview` for this toolchain' >&2
    exit 2
  fi
  # No report reuse: a failed test or coverage export aborts before any scoring.
  cargo llvm-cov --locked --lib --no-default-ignore-filename-regex \
    --ignore-filename-regex '^$' --lcov --output-path "$report/lcov.info" "$@" -- --test-threads 4
  cargo crap --lcov "$report/lcov.info" --format json --sort file --jobs 4 \
    --no-default-excludes --missing pessimistic "$@" > "$report/crap.json"
  python3 scripts/quality/gate.py crap-report "$report/crap.json"
fi
