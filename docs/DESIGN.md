# CodeCarbon Desktop — Design

> Status: v0.1 design, written before the first line of code. Update it when the
> implementation diverges.

## 1. Goal

A small application that lives in the **taskbar / menu bar / system tray** of
Windows, macOS and Linux and lets a person who has never opened a terminal:

1. install it by double-clicking an installer,
2. click **Start** to measure the power draw and CO₂ emissions of their
   computer with [CodeCarbon](https://github.com/mlco2/codecarbon),
3. see live **Power (W)**, **Energy (Wh)** and **CO₂e (g)** figures,
4. click **Stop** and get a session summary,
5. later: log in to the CodeCarbon API and upload their measurements.

Non-goals for v0.1: per-process attribution, remote dashboards, charts over
weeks of data, replacing the Python package for developers.

## 2. Stack decision

| Option | Pros | Cons |
| --- | --- | --- |
| **Pure Python** (pystray + PyInstaller) | one language, direct `import codecarbon` | tray libraries are limited (menus only, no live panel), ugly on Windows/Linux, large start-up, hard to make installers |
| **Pure Rust** (re-implement measurement) | tiny binary | duplicates CodeCarbon's hardware logic, TDP tables and carbon intensity data; numbers would drift from the library |
| **Tauri 2 (Rust) + Python sidecar** ✅ | native tray on 3 OSes, HTML/CSS panel, official installers (NSIS/MSI, DMG, AppImage/deb/rpm), tiny Rust shell, measurement done by **the real CodeCarbon library** | two toolchains (Rust + Python) in CI |

We chose **Tauri 2 + a Python sidecar**:

* **Tauri** gives us the tray icon, the menu, a small always-available window,
  auto-start at login, single-instance, and the installers for the three
  platforms from one `tauri build` command. Binaries are ~10 MB on top of the
  sidecar.
* **The sidecar (`codecarbon-agent`)** is a ~200-line Python program that
  wraps `codecarbon.EmissionsTracker` and speaks a JSON-lines protocol over
  stdin/stdout (see [PROTOCOL.md](PROTOCOL.md)). It is frozen with PyInstaller
  into a single executable per platform and bundled by Tauri as an
  `externalBin`. The user never sees Python.
* Keeping CodeCarbon as the engine means the GUI shows **exactly** the numbers
  the Python library would compute (same TDP tables, same RAPL / NVML /
  Windows EMI backends, same carbon-intensity data) and inherits every future
  improvement of the library.

Languages: Rust (app shell), TypeScript (panel UI, no framework, Vite), Python
(sidecar). All three are kept deliberately small and boring so Python-first
contributors can follow the Rust and TS parts.

## 3. Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Tauri app (Rust)                                        │
│                                                          │
│   tray.rs ──── menu / icon / tooltip                     │
│   state.rs ─── TrackerState (idle / running, latest...)  │
│   agent.rs ─── spawns + supervises the sidecar  ◄──┐     │
│   settings.rs  JSON file in the app config dir     │     │
│   commands.rs  #[tauri::command] API for the UI    │     │
│        │ events "tracker://state"                  │     │
│        ▼                                           │     │
│   WebView (TypeScript, Vite)                       │     │
│     Dashboard · Settings · About                   │     │
└────────────────────────────────────────────────────┼─────┘
                                                     │ stdin  : {"cmd": ...}
                                                     │ stdout : {"event": ...}
┌────────────────────────────────────────────────────┴─────┐
│  codecarbon-agent (Python, PyInstaller one-file binary)  │
│    codecarbon.EmissionsTracker / OfflineEmissionsTracker  │
│    + custom BaseOutput handler → JSON on stdout           │
└──────────────────────────────────────────────────────────┘
```

### 3.1 Lifecycle

1. App starts hidden in the tray (no main window, no dock icon on macOS).
2. It spawns the sidecar immediately (`ready` event, ~2–5 s because of the
   PyInstaller one-file extraction) and keeps it alive for the whole session.
   If the sidecar dies it is restarted (max 3 times, with back-off); the UI
   shows an error state.
3. **Start** (tray menu, panel button, or "start measuring on launch" setting)
   sends `{"cmd":"start","config":{...}}`. The agent creates a tracker and
   emits `started` (hardware + location) then a `measurement` every
   `interval_secs`.
4. **Stop** sends `{"cmd":"stop"}`; the agent emits `stopped` with the session
   summary. CodeCarbon also appends the run to `emissions.csv` in the app data
   folder (users can open that folder from the menu).
5. **Quit** stops a running measurement, sends `quit`, waits ≤ 3 s, kills.

### 3.2 Tray

* Icon: monochrome leaf (template icon on macOS). Grey/outlined when idle,
  filled green when measuring.
* Tooltip: `CodeCarbon — not measuring` / `CodeCarbon — 23.4 W · 0.84 g CO₂e`.
* macOS only: the menu-bar title shows the live power, e.g. `23 W`.
* Menu:
  * **Start measuring** / **Stop measuring**
  * (disabled) `Power: 23.4 W`, `CO₂e: 0.84 g`, `Energy: 12.3 Wh` while running
  * **Show CodeCarbon** (opens the panel)
  * **Settings…**
  * **Open data folder**
  * **Quit**
* Left click opens/toggles the panel; right click (or left on macOS when the
  panel is already positioned) opens the menu.

### 3.3 Panel (WebView)

A single ~400 × 560 px window, hidden instead of closed, positioned next to the
tray icon when opened from it. Three views, switched by a top tab bar:

* **Dashboard** — big Start/Stop button; live tiles for Power (with CPU/GPU/RAM
  breakdown), CO₂e, Energy, Duration; a sparkline of the last 5 minutes of
  power; a status line that says *how* the number was obtained
  ("CPU: estimated from load, TDP 45 W · GPU: NVML · France · 56 g/kWh").
  After a stop, the last session summary stays visible.
* **Settings** — measurement interval, country override (3-letter ISO) or
  "detect from IP", keep CSV history, start measuring on launch, launch at
  login, and an **API** section (endpoint, API key, experiment id, "upload to
  CodeCarbon API") that is plumbed through to the tracker but marked
  *experimental* until the login flow (roadmap) exists.
* **About** — versions (app, agent, CodeCarbon), links, licence.

Light and dark themes follow the OS. No framework: `src/main.ts` + small
modules (`api.ts`, `format.ts`, `views/*.ts`).

### 3.4 Data & privacy

* Settings: `<app config dir>/settings.json`.
* History: `<app data dir>/emissions.csv` written by CodeCarbon itself.
* Network: none unless the user leaves country on "detect from IP" (CodeCarbon
  calls its geolocation endpoint once per start) or enables API upload.

## 4. Repository layout

```
agent/            Python sidecar (package `codecarbon_agent`, tests, PyInstaller spec)
src/              Front-end (Vite + TypeScript)
src-tauri/        Rust application (Cargo project, tauri.conf.json, icons, capabilities)
src-tauri/binaries/   built sidecar binaries, git-ignored (`codecarbon-agent-<target-triple>`)
docs/             this folder
.github/workflows CI (tests + installers for the 3 OSes) and release on tags
scripts/          helper scripts used by CI and developers
```

## 5. Implementation plan (incremental)

1. **Docs** — design, protocol, roadmap (this commit).
2. **Agent** — `codecarbon_agent` package, protocol implementation, unit tests
   with a fake tracker, a `--selftest` mode, PyInstaller build script.
3. **App** — Tauri scaffold, sidecar supervision, state machine, tray, panel,
   settings. Compiles and runs on Linux in CI/this container.
4. **CI** — tests on every push; installers for Windows, macOS (Intel +
   Apple Silicon) and Linux uploaded as workflow artifacts; GitHub Release
   on `v*` tags.
5. **Polish & iterate** — fix what the CI matrix reveals, write the
   installation guide, first tagged release.

## 6. Testing strategy

* Agent: `pytest` with the tracker mocked (protocol, state machine, error
  paths) plus one opt-in integration test that runs the real tracker for a few
  seconds.
* Rust: unit tests for the event parser, the state reducer and the number
  formatting; `cargo clippy -D warnings`.
* End-to-end: the `--selftest` mode of the frozen agent is executed on every
  CI platform so a broken PyInstaller bundle fails the build, not the user.
* Manual: installers downloaded from the CI artifacts.

See [ROADMAP.md](ROADMAP.md) for what comes next.
