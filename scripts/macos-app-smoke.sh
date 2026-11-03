#!/usr/bin/env bash
# End-to-end run of the *installed-style* macOS app on a CI runner.
#
#   scripts/macos-app-smoke.sh /path/to/CodeCarbon.app [seconds]
#
# Copies the bundle out of the DMG to a temporary "Applications" folder,
# starts the real app binary (as Finder would, with a minimal environment
# and the working directory set to /), lets it start a measurement on
# launch, sends SIGTERM (graceful quit) and checks the log. This exercises
# what the sidecar-only self-test cannot: Tauri spawning the signed sidecar
# with --credentials under the app's hardened runtime.
set -euo pipefail

src_app="$1"
seconds="${2:-20}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/Applications"
ditto "$src_app" "$work/Applications/CodeCarbon.app"
app="$work/Applications/CodeCarbon.app"
bin="$app/Contents/MacOS/codecarbon-desktop"

# Fresh settings: measure on launch, offline country (no geolocation call).
support="$HOME/Library/Application Support/io.codecarbon.desktop"
rm -rf "$support"
mkdir -p "$support"
cat > "$support/settings.json" <<'JSON'
{ "start_tracking_on_launch": true, "interval_secs": 1, "country_iso_code": "FRA",
  "welcome_dismissed": true, "notify_on_stop": false, "check_updates": false }
JSON

log="$work/app.log"
# Finder starts apps with a minimal environment and cwd "/".
( cd / && env -i HOME="$HOME" USER="${USER:-runner}" TMPDIR="${TMPDIR:-/tmp}" \
    PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    RUST_LOG="info,codecarbon_desktop_lib=debug" \
    "$bin" > "$log" 2>&1 ) &
pid=$!
sleep "$seconds"
kill -TERM "$pid" 2> /dev/null || true
for _ in $(seq 1 150); do kill -0 "$pid" 2> /dev/null || break; sleep 0.1; done
if kill -0 "$pid" 2> /dev/null; then
  echo "SMOKE: app did not exit after SIGTERM" >> "$log"
  kill -KILL "$pid" || true
fi
set +e
wait "$pid"
echo "SMOKE: app exit code $?" >> "$log"
set -e

failures=0
check() { # check <description> <grep -E pattern> [min count]
  local count
  count="$(grep -cE "$2" "$log" || true)"
  if (( count >= ${3:-1} )); then
    printf '  ok    %-28s (%s)\n' "$1" "$count"
  else
    printf '  FAIL  %-28s (%s, expected >= %s)\n' "$1" "$count" "${3:-1}"
    failures=$((failures + 1))
  fi
}

echo "macOS app smoke test:"
check "sidecar spawned" "agent spawned \(pid [0-9]+\)"
check "ready received" "agent ready:"
check "started received" '← agent: \{"event": ?"started"'
check "measurements received" '← agent: \{"event": ?"measurement"' 3
check "stopped on quit" '← agent: \{"event": ?"stopped"'
if grep -q "agent exited unexpectedly" "$log"; then
  echo "  FAIL  agent crashed (see log)"
  failures=$((failures + 1))
fi
if [[ -s "$support/measurements/emissions.csv" ]]; then
  echo "  ok    emissions.csv written"
else
  echo "  FAIL  emissions.csv written"
  failures=$((failures + 1))
fi

echo "---- app log (agent lines) ----"
grep -E "agent|SMOKE|updater|panicked" "$log" | head -80 || true
if (( failures > 0 )); then
  echo "---- full app log ----"
  cat "$log"
  echo "macOS app smoke test FAILED ($failures)"
  exit 1
fi
echo "macOS app smoke test passed"
