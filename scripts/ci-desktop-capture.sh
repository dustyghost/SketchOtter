set -euo pipefail
openbox > target/desktop-results/wm.log 2>&1 &
wm=$!
trap 'kill "$wm" 2>/dev/null || true' EXIT
export XDG_CONFIG_HOME="$RUNNER_TEMP/sculpt-desktop-config"
for scene in sphere bottle; do
  args=()
  if [[ $scene == bottle ]]; then
    test -f target/desktop-asset/mixed-session.clay
    args=(target/desktop-asset/mixed-session.clay)
  fi
  target/release/sketchotter "${args[@]}" > "target/desktop-results/$scene-app.log" 2>&1 &
  app=$!
  (while kill -0 "$app" 2>/dev/null; do avail=$(awk '/^MemAvailable:/{print $2}' /proc/meminfo); rss=$(awk '/^VmRSS:/{print $2}' "/proc/$app/status" || true); if (( avail < 1228800 || ${rss:-0} > 3072000 )); then echo "Desktop memory guard"; kill "$app"; break; fi; sleep .20; done) &
  watcher=$!
  window=$(timeout 45 xdotool search --onlyvisible --sync --name SketchOtter | head -1)
  xdotool windowactivate --sync "$window"
  wmctrl -ir "$window" -b add,maximized_vert,maximized_horz
  sleep 5
  import -window root -resize 640x360 "target/desktop-results/$scene-before.png"
  /usr/bin/python3 scripts/profile-x11-pointer.py "target/desktop-results/$scene.json" --window "$window" || failed=1
  import -window root -resize 640x360 "target/desktop-results/$scene-after.png"
  wmctrl -ic "$window" || true
  kill "$app" 2>/dev/null || true
  wait "$app" || true
  wait "$watcher" || true
done
exit "${failed:-0}"
