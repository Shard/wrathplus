#!/usr/bin/env bash
# Explicit opt-in only. Heavy commands take the shared lock and own target dir.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
check=${1:?usage: run.sh loc|fmt|clippy|crap|machete|typos [paths or -p packages]}
shift
case "$check" in
loc|fmt) exec python3 scripts/quality/gate.py "$check" "$@" ;;
machete) exec cargo machete "$@" ;;
typos) exec typos "$@" ;;
clippy|crap) ;;
*) echo 'unknown check' >&2; exit 2 ;;
esac
: "${CARGO_TARGET_DIR:=/tmp/azb-target-experience}"
export CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
# Coverage can create a fresh instrumented tree; refuse to start above the shared cap.
if [[ -d $CARGO_TARGET_DIR ]] && (( $(du -sk "$CARGO_TARGET_DIR" | cut -f1) > 41943040 )); then
  echo 'quality: own target exceeds 40 GiB; clean it while idle before running' >&2
  exit 2
fi
# Re-enter once under the lock so sequential coverage + scoring remain one operation.
if [[ ${AZB_QUALITY_LOCKED:-0} != 1 ]]; then
  exec lockf -k /tmp/azb-heavy.lock env AZB_QUALITY_LOCKED=1 nice -n 15 "$0" "$check" "$@"
fi
# Test assumptions must not accidentally point vanilla fixtures at Wrath data.
unset WOW_CLIENT_PROFILE WOW_LOCALE
export WOW_DATA=
if [[ $# == 0 ]]; then set -- --workspace; fi
report=$(mktemp -d /tmp/azb-quality-report.XXXXXX)
echo "quality artifacts: $report" >&2
if [[ $check == clippy ]]; then
  # Rust diagnostics promoted to errors are allowed only by exact committed allowlist.
  set +e
  cargo clippy --locked --all-targets --message-format=json "$@" -- -D warnings > "$report/clippy.json"
  status=$?
  set -e
  python3 scripts/quality/gate.py clippy-report "$report/clippy.json"
  # Cargo emits build-finished=false for promoted legacy warnings too. Require at least
  # one diagnostic when it failed; crashes/config/tool failures cannot silently pass.
  if (( status != 0 )) && ! grep -q '"reason":"compiler-message"' "$report/clippy.json"; then exit "$status"; fi
else
  for tool in cargo-llvm-cov cargo-crap; do command -v "$tool" >/dev/null; done
  # No report reuse: a failed test or coverage export aborts before any scoring.
  cargo llvm-cov --locked --lib --no-default-ignore-filename-regex \
    --ignore-filename-regex '^$' --lcov --output-path "$report/lcov.info" "$@" -- --test-threads 4
  cargo crap --lcov "$report/lcov.info" --format json --sort file --jobs 4 \
    --no-default-excludes --missing pessimistic "$@" > "$report/crap.json"
  python3 scripts/quality/gate.py crap-report "$report/crap.json"
fi
