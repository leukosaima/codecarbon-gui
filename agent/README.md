# codecarbon-agent

The Python sidecar of CodeCarbon Desktop. It wraps the real
[CodeCarbon](https://github.com/mlco2/codecarbon) tracker and talks to the Tauri
app over a JSON-lines protocol: commands on **stdin**, events on **stdout**,
CodeCarbon's own logs on **stderr**. The contract is
[docs/PROTOCOL.md](../docs/PROTOCOL.md); the architecture is in
[docs/DESIGN.md](../docs/DESIGN.md).

```
codecarbon_agent/
  protocol.py   event builders, JSON sanitising (no NaN), thread-safe Writer
  tracker.py    TrackerSession: config → CodeCarbon tracker → started/measurement/stopped payloads
  runner.py     stdin command loop + background sampling thread + log forwarding
                + "account" worker thread for the auth_* / api_* commands
  auth.py       dashboard login: OIDC code + PKCE, callback server, credentials file, refresh
  dashboard.py  auth_* / api_* commands; REST calls through CodeCarbon's ApiClient
  compat.py     py-cpuinfo workaround for the frozen binary
  __main__.py   CLI: protocol mode, --version, --selftest, --credentials
```

## Develop and test

```sh
cd agent
python -m venv .venv && . .venv/bin/activate      # Python >= 3.10
pip install -e '.[dev]'
pytest                                            # unit tests, fake tracker, a few seconds
CODECARBON_AGENT_INTEGRATION=1 pytest -m integration   # real tracker, ~10 s
ruff check . && ruff format --check .
```

Try it by hand:

```sh
printf '%s\n' '{"cmd":"start","config":{"interval_secs":1,"country_iso_code":"FRA","save_to_file":false}}' \
  | python -m codecarbon_agent          # EOF on stdin stops the run and exits 0
python -m codecarbon_agent --selftest --seconds 3
```

## CodeCarbon dashboard account (protocol v2)

`codecarbon-agent --credentials <path>/credentials.json` enables the
`auth_start` / `auth_cancel` / `auth_status` / `auth_logout` and
`api_catalog` / `api_create` / `api_key` commands (without the flag they reply
`error(auth_unavailable)`). The login is the one of `codecarbon login`
(Keycloak realm `codecarbon`, client `codecarbon-cli`, callback on
`http://localhost:8090/callback`, 5 min timeout); tokens are stored in that
file (mode 0600) and never sent to the app. `CODECARBON_API_ENDPOINT`,
`AUTH_SERVER_WELL_KNOWN` and `AUTH_CLIENT_ID` point it at a staging server.
These commands run one at a time on their own thread, so they never delay a
running measurement. The tests use a fake OpenID provider on 127.0.0.1 and a
fake `ApiClient`: no network.

```sh
printf '%s\n' '{"cmd":"auth_status"}' '{"cmd":"auth_start"}' \
  | python -m codecarbon_agent --credentials /tmp/cc/credentials.json
```

## Build the sidecar binary

```sh
pip install pyinstaller
python build.py                 # or: python build.py --target aarch64-apple-darwin
```

This runs PyInstaller with `codecarbon-agent.spec` (one file, console) and copies
`dist/codecarbon-agent[.exe]` to
`../src-tauri/binaries/codecarbon-agent-<target-triple>[.exe]`, the name Tauri
expects for an `externalBin`. The triple is `--target`, else the host of
`rustc -vV`, else a guess from the platform. PyInstaller does not
cross-compile: build each target on its own OS / architecture. Check the
result with `codecarbon-agent-<triple> --selftest`.

## Notes

* The CSV history (`<output_dir>/emissions.csv`) gets **one row per run**,
  written at `stop` with CodeCarbon's own `FileOutput`. The agent calls
  `tracker.flush()` every `interval_secs`, and CodeCarbon's own CSV output
  would append a row on every flush.
* `measurement.power_w` is the instantaneous power of the last sample
  (`tracker._cpu_power` ...); `stopped.summary.avg_power_w` is the run average.
* Private CodeCarbon attributes are only read defensively: if a future
  release renames them, the affected fields become `null`.
