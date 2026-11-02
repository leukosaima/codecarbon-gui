# Security

CodeCarbon Desktop is installed on people's personal computers, so we treat
the supply chain with the same care as the code.

## Reporting a vulnerability

Please do **not** open a public issue. Use GitHub's private reporting:
*Security → Report a vulnerability* on this repository, or e-mail the
CodeCarbon maintainers (see <https://codecarbon.io>). We aim to acknowledge
reports within a week.

## What the app does, and does not do

* It runs two processes: the Tauri shell and the `codecarbon-agent` sidecar
  (a frozen Python program). They talk over stdin/stdout only.
* **Network access** happens only when the user allows it: one geolocation
  request at the start of a measurement when no country is set in Settings,
  and uploads when the user enables the CodeCarbon API. There is no
  telemetry, no auto-update (yet), no remote code.
* **Files written**: `settings.json` in the app's config folder,
  `measurements/emissions.csv` in its data folder, and the WebView's cache.
* **Privileges**: none. The app runs as the user and never asks for
  administrator rights; RAPL / `powermetrics` measurements are used only
  when the system already exposes them.
* The panel's WebView has a strict Content-Security-Policy and only the
  Tauri commands listed in `src-tauri/capabilities/default.json`.

## Automated checks

| What | Where | When |
| --- | --- | --- |
| CodeQL static analysis (TypeScript, Python, Rust, workflows) | `.github/workflows/codeql.yml` | every PR, push to main, weekly |
| Dependency advisories: `cargo audit`, `pip-audit`, `npm audit` | `.github/workflows/security.yml` | every PR, push to main, weekly |
| Dependency updates (Cargo, npm, pip, Actions) | `.github/dependabot.yml` | weekly |
| Build provenance attestations for every release asset | `.github/workflows/release.yml` | each release |
| `SHA256SUMS.txt` attached to every release | `.github/workflows/release.yml` | each release |
| Linting, unit and end-to-end tests | `.github/workflows/ci.yml` | every PR |

Workflows run with read-only `GITHUB_TOKEN` permissions except the release
workflow, which needs `contents: write` to upload assets and `id-token` /
`attestations: write` to sign provenance.

## Repository settings to keep enabled

These live in *Settings → Code security* and cannot be set from the code:

* Dependabot alerts and security updates
* Secret scanning with push protection
* Code scanning (CodeQL) default alerts on pull requests
* Branch protection on `main`: require the CI and CodeQL checks, no force-push

## Verifying a download

Every release ships `SHA256SUMS.txt` and Sigstore build-provenance
attestations. To check that an installer is the one CI built from the tagged
source:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing
gh attestation verify CodeCarbon_0.1.0_aarch64.dmg -R mlco2/codecarbon-gui
```

## Known gaps (roadmap)

* The installers are **not code-signed** yet (no Apple / Microsoft
  certificate). macOS and Windows show a warning on first launch; the
  attestations above are the way to verify authenticity in the meantime.
* No auto-updater: users must download new versions themselves. When added
  it will use Tauri's signed updater.
