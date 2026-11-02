# CodeCarbon Desktop

A tiny tray / menu-bar application that measures the power draw and CO₂
emissions of your computer with [CodeCarbon](https://github.com/mlco2/codecarbon),
for people who do not want to open a terminal.

* **Install** with a double-click (Windows `.exe`, macOS `.dmg`, Linux `.AppImage` / `.deb` / `.rpm`).
* **Start / Stop** measuring from the tray icon.
* **See** live power (W), energy (Wh) and CO₂e (g), plus a summary of each session.
* **Keep** a history in a CSV file written by CodeCarbon itself.
* Later: log in to the CodeCarbon API and upload your sessions (see the [roadmap](docs/ROADMAP.md)).

## How it works

The app is a small [Tauri 2](https://tauri.app) (Rust) shell that bundles a
frozen Python program, `codecarbon-agent`, which drives the real CodeCarbon
library. The numbers you see are exactly the ones the Python package computes.
Read [docs/DESIGN.md](docs/DESIGN.md) for the architecture and the reasoning,
and [docs/PROTOCOL.md](docs/PROTOCOL.md) for the way the two halves talk.

## Status

Work in progress — see [docs/ROADMAP.md](docs/ROADMAP.md).

## Licence

MIT — see [LICENSE](LICENSE).
