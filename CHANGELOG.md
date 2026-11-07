# Changelog

All notable changes to CodeCarbon Desktop. Versions are tagged `vX.Y.Z` and
published as GitHub pre-releases with installers for Windows, macOS (Apple
Silicon and Intel) and Linux.

## 0.1.7 — CodeCarbon design system
- The panel follows the CodeCarbon design system (Figma "Code Carbon -
  Master"): dark brand header with the logo, IBM Plex Mono throughout
  (bundled, works offline), lime primary and outline buttons, brand
  inputs, alerts, list rows and KPI tiles, lime charts, and the brand's
  pixel icons for the everyday equivalents. Light mode follows the
  design system's light schemes.
- A few colours are slightly darker or lighter than in Figma so that text
  meets WCAG AA contrast (see docs/DESIGN.md).

## 0.1.6 — diagnostics and Intel-Mac sign-in
- When the measurement engine stops unexpectedly, the error banner now
  includes the engine's own last error line, and the full error output
  is written to the app log (instead of only "exit code 255").
- Intel Macs: dashboard sign-in no longer fails with an OpenSSL
  `ImportError` (the build now uses a prebuilt cryptography wheel).
- CI and releases start the whole macOS app from the signed DMG on both
  Apple Silicon and Intel and check that a measurement runs.

## 0.1.5 — automatic updates (needs the signing key)
- The app can check GitHub Releases for a new version (shortly after launch,
  then daily; *Settings → Updates*), show it on the Dashboard and in the
  tray, and install it after stopping and saving a running measurement.
  Only updates signed with the project's key are accepted; switched on once
  the maintainers set up the key (builds without it never check).
- About: *Check for updates*.

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
