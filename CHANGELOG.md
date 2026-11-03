# Changelog

All notable changes to CodeCarbon Desktop. Versions are tagged `vX.Y.Z` and
published as GitHub pre-releases with installers for Windows, macOS (Apple
Silicon and Intel) and Linux.

## 0.1.4 — first-run help and notifications
- Welcome card on first launch explaining the tray, Start/Stop and privacy.
- Native notification with the session summary when a measurement stops
  (can be switched off in Settings).
- Layout fixes at the minimum window width.

## 0.1.3 — localisation
- English and French, auto-detected from the system, with a language setting.

## 0.1.2 — History
- History tab: today / this week / all time totals, 30-day chart, everyday
  equivalents (car km, smartphone charges, TV and streaming hours), sessions
  list read from CodeCarbon's `emissions.csv`.
- One equivalent shown under the last-session summary on the dashboard.

## 0.1.1 — dashboard account and logo
- Sign in to the free CodeCarbon dashboard from Settings, pick or create an
  organisation / project / experiment, upload sessions automatically.
- Official CodeCarbon logo for the tray and the app icon.
- Security: CodeQL, Dependabot, dependency audits, build-provenance
  attestations and `SHA256SUMS.txt` on every release; `SECURITY.md`.
- macOS: ad-hoc signed and sealed bundle (Gatekeeper "Open Anyway" works),
  no Dock icon, Cmd+Q stops the measurement first, re-launch shows the panel.
- Windows: tray click toggles the panel, 32 px tray icon, logoff stops the
  measurement.

## 0.1.0 — first release
- Tray / menu-bar app that starts and stops a CodeCarbon measurement and
  shows live power, energy and CO₂e, with a session summary.
- Settings: interval, country, CSV history, start on launch, launch at login.
- Python sidecar driving the real CodeCarbon library over a JSON-lines
  protocol; installers built by CI for all platforms.
