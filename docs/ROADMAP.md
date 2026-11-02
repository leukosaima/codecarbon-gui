# Roadmap

## v0.1 — "it measures" (this iteration)
- [x] Design, protocol, stack decision
- [ ] Python sidecar wrapping CodeCarbon, tested, frozen with PyInstaller
- [ ] Tauri tray app: start/stop, live power / energy / CO₂e, session summary
- [ ] Settings: interval, country, CSV history, start on launch, launch at login
- [ ] Installers for Windows (.exe/.msi), macOS (.dmg, Intel + Apple Silicon), Linux (.AppImage/.deb/.rpm) built by CI
- [ ] Installation guide for non-technical users

## v0.2 — "share it"
- **CodeCarbon API login.** Device-code / browser flow against the CodeCarbon
  dashboard, storing the token in the OS keychain (`keyring` crate), so the
  user never pastes an API key. Then choose organisation → project →
  experiment from drop-downs instead of typing an experiment id.
- **Upload sessions** to the API (already plumbed in the protocol through
  `save_to_api`, `api_key`, `experiment_id`).
- **History view**: read `emissions.csv`, show past sessions, totals per
  day/week, and "equivalents" (km by car, smartphone charges…).
- Notifications: "You emitted X g today".

## v0.3 — "keep it fresh"
- Auto-update (Tauri updater, signed) and code-signing / notarisation for
  macOS and Windows so SmartScreen/Gatekeeper stop warning.
- Localisation (fr, en first).
- Per-process view (CodeCarbon `tracking_mode="process"` on a chosen PID).
- Optional elevated helper for RAPL on Linux and `powermetrics` on macOS to
  replace the TDP estimate with real measurements.

## Ideas parked
- Replace PyInstaller with a smaller embedded interpreter (python-build-standalone).
- Prometheus / Logfire outputs are supported by CodeCarbon; expose them in Settings.
- A "pause when on battery" option.
