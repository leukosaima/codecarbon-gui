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
│   history.rs   emissions.csv → totals, equivalents │     │
│   updater.rs   signed updates from GitHub Releases │     │
│        │ events "tracker://state", "history://…"   │     │
│        ▼                                           │     │
│   WebView (TypeScript, Vite)                       │     │
│     Dashboard · History · Settings · About         │     │
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

* Icon: the CodeCarbon molecule mark (from the official logo). Dimmed grey
  when idle, brand lime when measuring; a template icon on macOS (alpha
  only, so idle is 45 % opacity). The app icon is the project's favicon.
* Tooltip: `CodeCarbon — not measuring` / `CodeCarbon — 23.4 W · 0.84 g CO₂e`
  (French: `CodeCarbon — aucune mesure en cours` / `… 23,4 W · 0,84 g CO₂e`, §3.5).
* macOS only: the menu-bar title shows the live power, e.g. `23 W`.
* Menu:
  * **Start measuring** / **Stop measuring**
  * (disabled) `Power: 23.4 W`, `CO₂e: 0.84 g`, `Energy: 12.3 Wh` while running
  * **Show CodeCarbon** (opens the panel)
  * **Settings…**
  * **Open data folder**
  * **Quit**
* Left click opens/toggles the panel; right click opens the menu. On Linux
  (libayatana-appindicator) tray click events are not delivered, so the panel
  is opened through the menu's **Show CodeCarbon** item.

### 3.3 Panel (WebView)

A single ~400 × 600 px window, hidden instead of closed, positioned next to the
tray icon when opened from it. Four views, switched by the tabs of the
brand header under the CodeCarbon logo (the tray's `app://navigate` event
can also select one):

* **Dashboard** — big Start/Stop button; live tiles for Power (with CPU/GPU/RAM
  breakdown), CO₂e, Energy, Duration; a sparkline of the last 5 minutes of
  power; a status line that says *how* the number was obtained
  ("CPU: estimated from load, TDP 45 W · GPU: NVML · France · 56 g/kWh").
  After a stop, the last session summary stays visible, with one everyday
  equivalent of that session ("≈ 0.4 km by car").
* **History** — read from CodeCarbon's `emissions.csv` by `history.rs`
  (`get_history`, re-read on every call: the file is small). Three tiles
  (Today / This week / All time: CO₂e, energy, time), a 30-day bar chart of
  daily CO₂e (inline SVG, Monday ticks, hover title per day), an "In
  everyday terms" card for the all-time total (km by car, smartphone
  charges, hours of LED TV, hours of video streaming) and the sessions list
  (newest first, 20 at a time). Empty state until the first session stops;
  the footer opens the data folder and says how many rows could not be
  read. Refreshed when the tab opens and on `history://changed`, which the
  controller emits when a session stops.
  * The CSV is parsed **by header name** (CodeCarbon adds columns between
    versions): unknown columns are ignored, missing optional ones are
    empty, malformed rows are skipped and counted. `timestamp` is the local
    stop time (no offset), so a session starts at `timestamp − duration`
    and counts for that day; weeks are ISO weeks (Monday first).
  * Equivalence factors live in one place, `history::FACTORS` (with their
    sources); Rust returns raw numbers and `src/equivalents.ts` phrases
    them with the app's number formatting. They are orders of magnitude.
* **Settings** — measurement interval, country override (3-letter ISO) or
  "detect from IP", keep CSV history, start measuring on launch, launch at
  login, language (System default / English / Français), and an **API** section (endpoint, API key, experiment id, "upload to
  CodeCarbon API") that is plumbed through to the tracker but marked
  *experimental* until the login flow (roadmap) exists.
* **About** — versions (app, agent, CodeCarbon), **Check for updates**
  (§3.3c), links, licence.

Light and dark schemes follow the OS (§3.3d). No framework: `src/main.ts` +
small modules (`api.ts`, `format.ts`, `equivalents.ts`, `barchart.ts`,
`brand.ts`, `i18n/`, `views/*.ts`).

### 3.3b First run and notifications

* **Welcome card** (Dashboard): shown until the user has stopped a first
  session or pressed "Got it" (`welcome_dismissed` setting). The rule lives in
  `src/welcome.ts`; `get_first_run_info` returns whether the history has
  sessions and the platform (the GNOME tray hint is Linux-only).
* **Session-end notification**: native notification "Session: 12 min ·
  0.04 Wh · 0.003 g CO₂e" sent by Rust (tauri-plugin-notification) when a
  session stops, unless disabled (`notify_on_stop`) or the app is quitting.
  The panel has no notification permission; only Rust notifies.
* Narrow windows (340 px): the power breakdown wraps instead of truncating;
  History tile numbers shrink to fit.

### 3.3c Automatic updates

`updater.rs`, with `tauri-plugin-updater`. Releases are GitHub Releases;
`latest.json` and the `.sig` signatures are made by `release.yml` once the
signing key exists (docs/DEVELOPMENT.md, "Enabling automatic updates").

* **Not configured**: while `plugins.updater.pubkey` in tauri.conf.json is
  the placeholder (or empty), `lib.rs` does not register the plugin; no
  request is made, About and Settings say updates are not configured.
* **Checks**: in the background ~10 s after the agent's first `ready`, then
  when the last check is 24 h old (wall clock, looked at hourly, so a
  computer that slept checks soon after waking), if `settings.check_updates`
  (default on). *About → Check for updates* checks at any time. A release
  is an update only if its semver is higher (build metadata ignored); a
  404 for `latest.json` means "nothing published yet", not an error.
* **State**: `TrackerState.update` (`UpdateInfo { version, date, notes }`)
  and `TrackerState.updater` (`configured`, `phase` idle / checking /
  downloading / installing, `checked`, `percent`, `error`), changed only
  by the pure `update_*` transitions in `state.rs`. The panel derives its
  texts with `src/update.ts`.
* **Shown as**: a green banner on the Dashboard "CodeCarbon 0.2.0 is
  available — Install" (with "the current measurement will be stopped and
  saved first" while measuring), the tray item "Update to 0.2.0…" (opens
  the Dashboard), and the line under About's button ("You are up to
  date", "Version 0.2.0 available", "Update check failed: …").
* **Install**: download with progress (percent in the banner); the plugin
  verifies the signature against the compiled-in public key
  (`requireSignedVersion`: the signed version must match). Only then the
  quit sequence runs without exiting (stop → wait for `stopped` → `quit`
  the agent → kill), the installer runs (on Windows it takes over and
  exits the app) and the app relaunches (`AppHandle::request_restart`,
  which is all that tauri-plugin-process's `restart` command does).
  If installing fails, the agent is started again and the banner shows
  the error.
* The panel only calls `check_for_updates` / `install_update`; it has no
  updater permission (capabilities/default.json is unchanged).

### 3.3d Visual design

The panel follows the CodeCarbon design system, Figma file **"Code Carbon -
Master"** (pages "Logo & colors", "Font styles", "UI Kit Website", "UI Kit
Platform", "Project I - Detailed view"). All of it is in `src/styles.css`
as `--cc-*` custom properties; the views only use classes.

* **Colours.** Dark is the brand's primary scheme (the web dashboard's):
  background `--cc-bg` #181818, raised rows `--cc-surface` #2B2B2B,
  dividers `--cc-border` #464646, highlights `--cc-lime` #BFFB4F (hover
  `--cc-lime-hover` #A0C55B), text `--cc-text` #FFFFFF, secondary text
  `--cc-text-2` #949494, disabled `--cc-disabled` #949494, typed text
  #D4D4D4. The light scheme (when the OS is light) is white with #D4D4D4
  inputs and dividers, black text and #666666 secondary text; lime is then
  only a fill (buttons, bars, the icons' dark slots), never text. The
  header (logo + tabs, 4 px #A0C55B bottom border) is dark in both.
* **Alerts** (`--cc-alert-*`, radius 4 px, bold, drop shadow): success
  #A0C55B / border #BFFB4F (update banner), warning #D4D068 (notices),
  error (error banner) with white text and border.
* **Type.** IBM Plex Mono for everything (400 body, 500 headings, 700
  buttons), bundled from `@fontsource/ibm-plex-mono` (OFL-1.1; latin and
  latin-ext only, `src/fonts.css`): the panel is offline and its CSP is
  `'self'`. Numbers (KPI tiles, session totals, equivalents) use the "Data"
  font `--cc-font-data`: Disket Mono if it is installed on the computer
  (it is not redistributable, so not bundled), else IBM Plex Mono Medium.
  Section titles are uppercase Medium 13 px with a small lime square (the
  brand's overline).
* **Shapes.** Radius `--cc-radius` 2 px (buttons, inputs, tiles, cards),
  `--cc-radius-alert` 4 px; flat 1 px borders, no soft shadows. Buttons:
  `.btn-primary` lime fill (Start, Sign in, Save, Got it, Install), `.btn`
  lime outline (Stop, Retry, Cancel, Show more, Check for updates).
* **Pictures** (`src/assets/brand/`, from Figma, via `src/brand.ts`): the
  logo (91 × 52) and the lime pixel icons of the equivalents (truck for km
  by car, TV for LED TV, house for the empty History).
* Departures, for WCAG AA: secondary text is #949494 on dark (Figma's
  #666666 is 3.1:1 on #181818), the error alert's fill is #9A5F4D instead
  of #B27663 (white text 5.1:1 instead of 3.7:1), placeholders are #808080
  (dark) / #595959 (light).

### 3.4 Data & privacy

* Settings: `<app config dir>/settings.json`.
* History: `<app data dir>/measurements/emissions.csv`, one row per session,
  written with CodeCarbon's own CSV output and only read by the app (History
  view); it never leaves the computer.
* Network: none unless the user leaves country on "detect from IP" (CodeCarbon
  calls its geolocation endpoint once per start) or enables API upload;
  plus, in builds with an updater key, the update check (§3.3c) unless
  "Check for updates automatically" is off.

### 3.5 Localisation

English (default and fallback) and French. `settings.language` is
`"system"` (default), `"en"` or `"fr"`; "system" takes the first of the
OS's preferred languages that we have (`fr*` → French), else English.

* **Panel**: `src/i18n/en.ts` and `fr.ts` are flat message tables (`fr`
  is typed from `en`, so a missing key does not compile; a Vitest test
  also compares keys and placeholders). `t(key, {name})` interpolates,
  `n(key, count)` picks `key_one` / `key_other` with `Intl.PluralRules`
  (French: 0 and 1 are singular). Numbers go through `Intl.NumberFormat`
  (no grouping, rounded like the tray), dates through `Intl.DateTimeFormat`,
  both with the chosen language (the user's regional variant when it
  matches: `en-GB`, `fr-CA`). French units are joined with a non-breaking
  space. The OS's language list comes from Rust (`get_system_languages`,
  `sys-locale`), because WebViews do not reliably expose it (WebKitGTK
  follows the C locale, macOS the app bundle's localisations). Text built
  once uses `live(key)` nodes; changing the language dispatches
  `locale-changed` and the views render again, without a reload.
* **Rust**: `i18n.rs` holds the ~20 tray / tooltip / macOS-menu strings
  (`tr(lang, Key)`, one macro list so both languages are always present);
  the tray reads the language from the settings on every refresh and
  relabels itself when it changes, the macOS app menu is rebuilt.
  `format.rs` only switches the decimal separator (`23,4 W`).
* **Errors** shown in the panel are `AppError`s (`error.rs`), serialised as
  `{code, detail, message}`: the panel translates `error.<code>` and
  inserts the untranslated `detail` (OS error, path); `message` is the
  English text, used in the logs and for unknown codes.
* Not translated: CodeCarbon's own warnings ("Notes from CodeCarbon"),
  hardware names, country names reported by CodeCarbon, and messages
  coming from the login server or the dashboard API (shown as `detail`).

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
