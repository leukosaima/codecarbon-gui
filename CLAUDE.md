# CodeCarbon Desktop — notes for AI assistants and new contributors

Read `docs/DESIGN.md` (architecture), `docs/PROTOCOL.md` (app ⇄ sidecar
contract) and `docs/DEVELOPMENT.md` (toolchains, commands) first.

## Layout
- `agent/` — Python sidecar `codecarbon-agent` (drives the CodeCarbon library;
  pytest + ruff; frozen with PyInstaller by `agent/build.py`).
- `src-tauri/` — Rust/Tauri 2 shell (tray, state, settings, sidecar supervision,
  history, i18n). `cargo clippy --all-targets -- -D warnings` must be clean.
- `src/` — vanilla TypeScript panel (Vite, Vitest). No UI framework.
- `scripts/` — fake agent for UI work, headless smoke test (Xvfb).
- `.github/workflows/` — CI (tests + installers on 4 targets), CodeQL,
  dependency audit, Release (manual dispatch or `v*` tag).

## Rules of thumb
- Every user-visible string goes through i18n (`src/i18n/*.ts`, `src-tauri/src/i18n.rs`),
  in English **and** French. Errors from Rust to the panel are codes, not text.
- Measurement numbers come from CodeCarbon, never re-implemented. Private
  attribute access in `agent/` must degrade to `null`, never crash.
- The sidecar's stdout is protocol JSON only; everything else goes to stderr.
- Keep the three toolchains boring: Python-first contributors must be able to
  follow the Rust and TypeScript.
- Before pushing: `pytest agent`, `npm test && npm run build`,
  `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`,
  `scripts/smoke-test.sh`.
- Releases: bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
  (+ `cargo update -p codecarbon-desktop`), `package.json` (+ lock),
  `agent/pyproject.toml`, `agent/codecarbon_agent/__init__.py`; add a
  `CHANGELOG.md` entry; dispatch the Release workflow on `main`.
