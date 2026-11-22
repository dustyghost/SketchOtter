#!/usr/bin/env bash
# Isolate input injection and app settings from the artist's active desktop.
set -euo pipefail
if [[ ${1:-} != --inside ]]; then
    if (( $# < 2 || $# > 3 )); then
        echo "Usage: bash scripts/profile-desktop-session.sh RELEASE_BINARY FRESH_OUTPUT [PROJECT.clay]" >&2
        exit 2
    fi
    self=$(realpath "$0")
    binary=$(realpath -e "$1")
    output=$(realpath -m "$2")
    project=()
    if (( $# == 3 )); then project=("$(realpath -e "$3")"); fi
    mkdir "$output"
    cc -std=c11 -O2 -Wall -Wextra -Werror "$(dirname "$self")/profile-x11-present.c" \
        -lxcb -lxcb-present -o "$output/present-observer"
    exec xvfb-run -a -s '-screen 0 1920x1080x24' bash "$self" --inside "$binary" "$output" "${project[@]}"
fi
shift
binary=$1
output=$2
shift 2
scratch=$(mktemp -d)
export XDG_RUNTIME_DIR="$scratch/runtime" XDG_CONFIG_HOME="$scratch/config"
export XDG_CACHE_HOME="$scratch/cache" XDG_DATA_HOME="$scratch/data"
install -d -m 700 "$XDG_RUNTIME_DIR"
# Suppress the pre-release notice in this isolated profile only.
python3 - "$(dirname "$0")/../Cargo.toml" "$XDG_CONFIG_HOME/clay-form/preferences.json" <<'PYCONF'
import json, sys, tomllib
from pathlib import Path
version = tomllib.loads(Path(sys.argv[1]).read_text())['package']['version']
prefs = Path(sys.argv[2])
prefs.parent.mkdir(parents=True, exist_ok=True)
prefs.write_text(json.dumps({'dismissed_startup_version': version}))
PYCONF
unset WAYLAND_DISPLAY
openbox >"$output/window-manager.log" 2>&1 &
wm=$!
app=
watcher=
cleanup() {
    for pid in "$watcher" "$app" "$wm"; do
        if [[ -n $pid ]]; then kill "$pid" 2>/dev/null || true; fi
    done
    rm -rf -- "$scratch"
}
trap cleanup EXIT
available_kib() { awk '/^MemAvailable:/{print $2}' /proc/meminfo; }
budget_kib=$((3000 * 1024))
reserve_kib=$((1200 * 1024))
if (( $(available_kib) < budget_kib + reserve_kib )); then
    echo "Desktop workload skipped by the RAM preflight" >&2
    exit 1
fi
"$binary" "$@" >"$output/application.log" 2>&1 &
app=$!
(
    while kill -0 "$app" 2>/dev/null; do
        rss=$(awk '/^VmRSS:/{print $2}' "/proc/$app/status" 2>/dev/null || true)
        if (( ${rss:-0} > budget_kib || $(available_kib) < reserve_kib )); then
            echo "Application stopped by the RSS or system reserve guard" >"$output/guard.log"
            kill "$app" 2>/dev/null || true
            break
        fi
        sleep 0.2
    done
) &
watcher=$!
window=
for _ in {1..120}; do
    if ! kill -0 "$app" 2>/dev/null; then cat "$output/application.log" >&2; exit 1; fi
    window=$(xdotool search --onlyvisible --name SketchOtter 2>/dev/null | head -1 || true)
    [[ -z $window ]] || break
    sleep 0.25
done
if [[ -z $window ]]; then cat "$output/application.log" >&2; exit 1; fi
timeout 12 xdotool windowactivate --sync "$window"
wmctrl -ir "$window" -b add,maximized_vert,maximized_horz
sleep 5
if command -v import >/dev/null; then
    import -window root -resize 640x360 "$output/desktop-before.png"
fi
# The saved two-model workload opens with the right-hand bottle active.
# An arbitrary project can select another exposed surface with the two variables.
target_x=0.53125
target_y=0.50
if (( $# )); then target_x=0.621; target_y=0.45; fi
timeout 120 /usr/bin/python3 "$(dirname "$0")/profile-x11-pointer.py" "$output/pointer.json" \
    --target "${CLAY_DESKTOP_X:-$target_x}" "${CLAY_DESKTOP_Y:-$target_y}" --window "$window" --executable "$binary" --present-helper "$output/present-observer"
