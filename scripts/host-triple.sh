#!/usr/bin/env bash
# Print the Rust host target triple (e.g. x86_64-unknown-linux-gnu).
# Tauri looks for sidecars named `<name>-<target-triple>[.exe]`.
set -euo pipefail
rustc -vV | sed -n 's/^host: //p'
