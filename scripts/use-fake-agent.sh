#!/usr/bin/env bash
# Install scripts/fake-agent.py as the Tauri sidecar so the app can be built
# and run without the real PyInstaller agent (UI development, CI smoke test).
#
#   scripts/use-fake-agent.sh          # refuses to replace a real agent binary
#   scripts/use-fake-agent.sh --force  # replaces whatever is there
#
# On Windows the sidecar must be a real .exe, so this script is for
# Linux and macOS only.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"
triple="$("$here/host-triple.sh")"
dest="$root/src-tauri/binaries/codecarbon-agent-$triple"

mkdir -p "$(dirname "$dest")"
if [[ -e "$dest" && "${1:-}" != "--force" ]]; then
  if head -c 64 "$dest" | grep -q "fake-agent\|python3"; then
    : # already a script (probably the fake agent): refresh it
  else
    echo "A real agent binary already exists at $dest; not overwriting (use --force)." >&2
    exit 0
  fi
fi
cp "$here/fake-agent.py" "$dest"
chmod +x "$dest"
echo "Fake agent installed at $dest"
