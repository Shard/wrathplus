#!/usr/bin/env bash
# Manager-only: invokes an already-built dev-enabled client, never cargo/builds or servers.
set -euo pipefail
if [[ $# != 3 ]]; then
  echo "usage: $0 /absolute/path/to/benilla path.toml NEW-output-root" >&2
  exit 2
fi
bin=$1
path=$2
out=$3
[[ -x "$bin" && -f "$path" ]] || { echo 'missing executable/path' >&2; exit 2; }
# Refuse any existing root: neither failed nor finished sequences may be overwritten.
mkdir "$out"
out=$(cd "$out" && pwd)
path=$(cd "$(dirname "$path")" && printf '%s/%s' "$PWD" "$(basename "$path")")
# Both runs inherit the same installed data, locale, MSAA and lighting/feature configuration.
# Offline recorder's startup guard prevents network connection; clear eval capture explicitly.
for profile in classic plus; do
  env -u WOW_CAPTURE WOW_EXPERIENCE="$profile" WOW_SHOWCASE_PATH="$path" \
    WOW_SHOWCASE_OUT="$out/$profile" "$bin"
done
# Write (do not execute) the encoder command; %q preserves spaces/shell metacharacters.
printf 'ffmpeg -framerate 60 -start_number 0 -i %q -framerate 60 -start_number 0 -i %q -filter_complex %q -map "[v]" -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart %q\n' \
  "$out/classic/frame%06d.png" "$out/plus/frame%06d.png" \
  '[0:v][1:v]hstack=inputs=2:shortest=1[v]' "$out/side-by-side.mp4" \
  > "$out/encode-side-by-side.sh"
cat "$out/encode-side-by-side.sh"
