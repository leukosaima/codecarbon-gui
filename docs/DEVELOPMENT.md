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
Set `RUST_LOG=debug` to see the sidecar's events, and `CODECARBON_SHOW_PANEL=1`
to show the panel immediately (handy on desktops without a tray).

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

## Enabling automatic updates

The app checks
`https://github.com/mlco2/codecarbon-gui/releases/latest/download/latest.json`
and installs updates signed with the project's updater key. Until that key
exists, `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` is the
placeholder `REPLACE_WITH_UPDATER_PUBKEY`: the updater is not loaded, the
About view says updates are not configured, and the Release workflow builds
the installers without updater files. To switch updates on (once):

1. Generate the key pair, with a password:

   ```bash
   npm run tauri signer generate -- -w ~/.tauri/codecarbon.key
   ```

   This writes the private key to `~/.tauri/codecarbon.key` and the public
   key to `~/.tauri/codecarbon.key.pub`. Keep the private key and its
   password somewhere safe and offline (a password manager): **if it is
   lost, installed apps can never be updated again.**
2. Put the **public** key (the one-line content of `codecarbon.key.pub`) in
   `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`, and commit it.
3. Add two repository secrets (*Settings → Secrets and variables →
   Actions*): `TAURI_SIGNING_PRIVATE_KEY` = the content of
   `codecarbon.key`, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = its password.
4. Release as usual. With the secret present, `release.yml` adds
   `--config src-tauri/updater.conf.json` (`createUpdaterArtifacts: true`)
   and the macOS `app` bundle: the release then also has `.sig` files, the
   macOS `.app.tar.gz` and a `latest.json` (also listed in `SHA256SUMS.txt`).
   The workflow fails early if the secret is set but the public key is still
   the placeholder, and the `SHA256SUMS` job checks that `latest.json` lists
   all four platforms.
5. **Roll it out**: GitHub's `releases/latest` URL skips pre-releases, and
   our releases are created as pre-releases. Installed apps see a release
   only once you edit it and untick *Set as a pre-release* (*Set as the
   latest release*). Until then it can be tested by downloading it by hand.

Installed apps built **with** the public key then update themselves (a
banner on the Dashboard, an item in the tray, *About → Check for updates*).
Installs built **before** the key was set (every version older than the
first one with the key) have no key to verify with and cannot auto-update:
users must download that first version once by hand.

Notes:

* Testing locally: the placeholder key means `npm run tauri dev` never
  checks. To try the flow, build with a test key pair in a scratch config
  and serve a hand-made `latest.json`; never commit a test public key.
* Installs only trust the key they were built with. Changing it means
  shipping one release signed with the old key that carries the new public
  key; treat the key as permanent.
* A `latest.json` endpoint that answers 404 (no release with updater files
  is "latest" yet) is treated as "up to date", not as an error.
