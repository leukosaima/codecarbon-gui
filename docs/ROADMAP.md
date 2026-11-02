# Roadmap

## v0.1 — "it measures" (this iteration)
- [x] Design, protocol, stack decision
- [x] Python sidecar wrapping CodeCarbon, tested, frozen with PyInstaller
- [x] Tauri tray app: start/stop, live power / energy / CO₂e, session summary
- [x] Settings: interval, country, CSV history, start on launch, launch at login
- [ ] Installers for Windows (.exe/.msi), macOS (.dmg, Intel + Apple Silicon), Linux (.AppImage/.deb/.rpm) built by CI
- [x] Installation guide for non-technical users

## v0.2 — "share it"
- [x] **CodeCarbon dashboard account** (v0.1.1): browser login (OpenID
  Connect + PKCE, same flow as `codecarbon login`), choose or create
  organisation → project → experiment from drop-downs, project API key
  fetched automatically, sessions uploaded through `save_to_api`.
  Later: keep the tokens in the OS keychain instead of a 0600 file.
- [x] **History view**: read `emissions.csv`, show past sessions, totals per
  day/week, and "equivalents" (km by car, smartphone charges…).
- Notifications: "You emitted X g today".

## v0.3 — "keep it fresh"
- Auto-update (Tauri updater, signed) and code-signing / notarisation for
  macOS and Windows so SmartScreen/Gatekeeper stop warning.
- [x] **Localisation**: English and French, panel and tray, following the
  system language or the Settings → Language choice (see DESIGN.md §3.5).
- Per-process view (CodeCarbon `tracking_mode="process"` on a chosen PID).
- Optional elevated helper for RAPL on Linux and `powermetrics` on macOS to
  replace the TDP estimate with real measurements.

## Ideas parked
- Replace PyInstaller with a smaller embedded interpreter (python-build-standalone).
- Prometheus / Logfire outputs are supported by CodeCarbon; expose them in Settings.
- A "pause when on battery" option.
