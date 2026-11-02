# Developing CodeCarbon Desktop

Three parts, three toolchains — but each one is small:

| Part | Where | Tooling |
| --- | --- | --- |
| Python sidecar `codecarbon-agent` | `agent/` | Python ≥ 3.10, pytest, ruff, PyInstaller |
| Tauri app (Rust) | `src-tauri/` | Rust stable, Tauri 2 CLI |
| Panel UI (TypeScript) | `src/` | Node 22, Vite, Vitest |

## One-time setup

* Rust: <https://rustup.rs>
* Node 22: <https://nodejs.org>
* Python 3.12: <https://python.org>
* Tauri prerequisites for your OS: <https://tauri.app/start/prerequisites/>
  (Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf`)

```bash
python -m venv .venv && . .venv/bin/activate      # Windows: .venv\Scripts\activate
pip install -e "agent[dev]" pyinstaller
npm install
```

## Build the sidecar

Tauri needs the frozen agent at `src-tauri/binaries/codecarbon-agent-<host-triple>`
before it can compile. Either build the real one:

```bash
python agent/build.py          # ~1–2 min, writes src-tauri/binaries/codecarbon-agent-<triple>
```

or, for UI work without CodeCarbon, use the fake agent that emits synthetic
numbers:

```bash
scripts/use-fake-agent.sh
```

## Run the app in development

```bash
npm run tauri dev
```

The window stays hidden at start-up (it is a tray app): click the tray icon.
Set `RUST_LOG=debug` to see the sidecar's events.

## Tests

```bash
pytest agent                                   # protocol + state machine (fake tracker)
CODECARBON_AGENT_INTEGRATION=1 pytest agent -m integration   # real tracker, ~3 s
npm test                                       # formatting helpers (vitest)
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
scripts/smoke-test.sh                          # headless launch under Xvfb (Linux)
```

## Build installers locally

```bash
python agent/build.py
npm run tauri build
# → src-tauri/target/release/bundle/{deb,rpm,appimage,dmg,nsis,msi}/
```

CI does the same for all four targets on every push (see
`.github/workflows/ci.yml`) and attaches the files to a GitHub Release when
you push a `vX.Y.Z` tag (`release.yml`).

## Releasing

1. Bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`,
   `package.json` and `agent/pyproject.toml`.
2. Commit, tag `vX.Y.Z`, push the tag.
3. The *Release* workflow creates a **draft** release with the installers;
   review it and publish.
