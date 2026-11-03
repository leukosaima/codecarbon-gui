#!/usr/bin/env bash
# Headless end-to-end smoke test of the desktop app (Linux, needs xvfb-run).
#
# It runs the debug build under a virtual X server with a throw-away settings
# file ("start measuring on launch", 1 s interval, country FRA so no network
# is needed), lets it measure for a few seconds, sends SIGTERM (which runs the
# same graceful quit as the tray's Quit item) and checks the logs:
#
#   agent spawned → ready → started → measurements → stopped → quit → exit 0
#
# and that the updater is either "not configured" (placeholder public key:
# no update check at all) or enabled.
#
# Needs: xvfb-run (packages `xvfb` and `xauth` on Debian/Ubuntu).
#
# Usage:
#   scripts/smoke-test.sh            # uses the existing debug build (builds it if missing)
#   scripts/smoke-test.sh --build    # rebuilds it first (tauri build --debug --no-bundle)
#
# Environment:
#   SMOKE_SECONDS=15        how long the app runs before SIGTERM
#   SMOKE_SCREENSHOT=path   also save a PNG of the panel (needs ImageMagick `import`)
#   APP_BIN=path            app binary (default: src-tauri/target/debug/codecarbon-desktop)
#
# If no sidecar is present in src-tauri/binaries/, the fake agent
# (scripts/fake-agent.py) is installed first.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"
seconds="${SMOKE_SECONDS:-15}"
bin="${APP_BIN:-$root/src-tauri/target/debug/codecarbon-desktop}"
triple="$("$here/host-triple.sh")"
sidecar="$root/src-tauri/binaries/codecarbon-agent-$triple"

if [[ ! -x "$sidecar" ]]; then
  "$here/use-fake-agent.sh"
fi
if [[ "${1:-}" == "--build" || ! -x "$bin" ]]; then
  # Debug build with the front-end embedded (runs `npm run build` first).
  (cd "$root" && npx tauri build --debug --no-bundle)
fi

# The build copies the sidecar next to the app binary: that copy is what runs.
running_sidecar="$(dirname "$bin")/codecarbon-agent"
if head -c 2 "$running_sidecar" | grep -q '#!'; then
  agent_kind="fake"
else
  agent_kind="real"
fi
echo "smoke test: using the $agent_kind agent ($running_sidecar)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
mkdir -p "$XDG_CONFIG_HOME/io.codecarbon.desktop"
cat > "$XDG_CONFIG_HOME/io.codecarbon.desktop/settings.json" <<'JSON'
{ "start_tracking_on_launch": true, "interval_secs": 1, "country_iso_code": "FRA" }
JSON
export RUST_LOG="${RUST_LOG:-info,codecarbon_desktop_lib=debug}"
log="$work/app.log"

# Runs inside the virtual X server.
cat > "$work/session.sh" <<'SH'
#!/usr/bin/env bash
"$APP_BIN" > "$APP_LOG" 2>&1 &
pid=$!
sleep "$SMOKE_SECONDS"
if [[ -n "${SMOKE_SCREENSHOT:-}" ]] && command -v import > /dev/null; then
  import -window root "$SMOKE_SCREENSHOT" || true
fi
kill -TERM "$pid" 2> /dev/null || true
# Graceful quit takes at most ~6 s (3 s for `stopped`, 3 s for the agent to exit).
for _ in $(seq 1 100); do kill -0 "$pid" 2> /dev/null || break; sleep 0.1; done
if kill -0 "$pid" 2> /dev/null; then
  echo "SMOKE: app did not exit after SIGTERM" >> "$APP_LOG"
  kill -KILL "$pid"
fi
wait "$pid"
echo "SMOKE: app exit code $?" >> "$APP_LOG"
SH
chmod +x "$work/session.sh"

APP_BIN="$bin" APP_LOG="$log" SMOKE_SECONDS="$seconds" \
  CODECARBON_SHOW_PANEL="${SMOKE_SCREENSHOT:+1}" SMOKE_SCREENSHOT="${SMOKE_SCREENSHOT:-}" \
  xvfb-run -a -s "-screen 0 1024x768x24" "$work/session.sh" || true

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

echo "smoke test results:"
check "sidecar spawned" "agent spawned \(pid [0-9]+\)"
check "ready received" "agent ready:"
check "start sent on launch" '→ agent: \{"cmd": ?"start"'
check "started received" '← agent: \{"event": ?"started"'
check "measurements received" '← agent: \{"event": ?"measurement"' 3
check "stopped on quit" '← agent: \{"event": ?"stopped"'
check "agent quit cleanly" "agent exited \(exit code 0\)"
check "app exited with 0" "SMOKE: app exit code 0"
# Updater: a build with the placeholder public key never checks (no network).
check "updater set up" "updater: (not configured|enabled)"
if grep -q "updater: not configured" "$log"; then
  if grep -qE "update available|no update available|update check failed|checking for updates" "$log"; then
    echo "  FAIL  unconfigured updater made a check"
    failures=$((failures + 1))
  else
    echo "  ok    unconfigured updater made no check"
  fi
fi
if [[ "$agent_kind" == "real" ]]; then
  if [[ -s "$XDG_DATA_HOME/io.codecarbon.desktop/measurements/emissions.csv" ]]; then
    echo "  ok    emissions.csv written"
  else
    echo "  FAIL  emissions.csv written"
    failures=$((failures + 1))
  fi
fi
if pgrep -f "$(dirname "$bin")/codecarbon-agent" > /dev/null; then
  echo "  FAIL  agent process left behind"
  failures=$((failures + 1))
else
  echo "  ok    no agent process left behind"
fi
if grep -qE "Traceback|ImportError|ModuleNotFoundError" "$log"; then
  echo "  FAIL  Python error in the sidecar (see log)"
  failures=$((failures + 1))
fi
if grep -qE "panicked|SMOKE: app did not exit" "$log"; then
  echo "  FAIL  panic or hang (see log)"
  failures=$((failures + 1))
fi

if (( failures > 0 )); then
  echo "---- app log ----"
  cat "$log"
  echo "smoke test FAILED ($failures)"
  exit 1
fi
echo "smoke test passed"
