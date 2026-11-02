# Agent protocol (app ⇄ `codecarbon-agent`)

Transport: the app spawns `codecarbon-agent` and talks to it through its
**stdin** (commands) and **stdout** (events). One JSON object per line, UTF-8,
`\n` terminated. Nothing else is ever written to stdout; CodeCarbon's own log
lines go to **stderr**. Unknown fields must be ignored by both sides so the two
halves can evolve independently.

Version negotiation: the `ready` event carries `protocol_version` (integer,
currently `2`, see the end of this document for what v2 adds).

## Commands (stdin, app → agent)

| `cmd` | Fields | Effect |
| --- | --- | --- |
| `start` | `config` object (below) | Create and start a tracker. Replies `started` or `error`. |
| `stop` | – | Stop the tracker. Replies `stopped` (with summary) or `error(not_running)`. |
| `status` | – | Replies a `status` event. |
| `quit` | – | Stops a running tracker, then exits 0. EOF on stdin behaves the same. |

### `config` object (all keys optional)

| key | type | default | notes |
| --- | --- | --- | --- |
| `interval_secs` | number | `3` | `measure_power_secs` of the tracker **and** the cadence of `measurement` events (clamped to ≥ 0.5) |
| `country_iso_code` | string | – | 3-letter ISO code. If present → `OfflineEmissionsTracker` (no network). If absent → `EmissionsTracker` (geolocation by IP). |
| `region` | string | – | state / province (US, CA); only used together with `country_iso_code` |
| `tracking_mode` | `"machine"` \| `"process"` | `"machine"` | |
| `project_name` | string | `"codecarbon-gui"` | |
| `save_to_file` | bool | `true` | append **one row per run** (at `stop`) to `output_dir/emissions.csv`, in CodeCarbon's CSV format |
| `output_dir` | string | cwd | created if missing. The app must always pass it (Finder-launched apps have an unwritable cwd; Apple Silicon measurement also writes a temp log there). |
| `save_to_api` | bool | `false` | upload to the CodeCarbon API |
| `api_endpoint` | string | `"https://api.codecarbon.io"` | |
| `api_key` | string | – | required when `save_to_api` |
| `experiment_id` | string | – | required when `save_to_api` |
| `force_carbon_intensity_g_co2e_kwh` | number | – | override the grid intensity |
| `pue` | number | – | power usage effectiveness |
| `log_level` | string | `"warning"` | CodeCarbon log level (stderr) |

## Events (stdout, agent → app)

Every event has an `event` key and a `ts` key (ISO-8601 local time with
offset, e.g. `2026-11-02T20:50:26+01:00`).

### `ready` — once, at start-up
```json
{"event":"ready","ts":"…","protocol_version":2,"agent_version":"0.1.0",
 "codecarbon_version":"3.3.2","python_version":"3.13.1","pid":4242,
 "platform":"Linux-6.8-x86_64"}
```

### `started` — after a successful `start`
```json
{"event":"started","ts":"…","run_id":"uuid","started_at":"…",
 "hardware":{
   "cpu_model":"Intel(R) Core(TM) i7-1185G7","cpu_count":8,"cpu_physical_count":4,
   "cpu_method":"cpu_load","cpu_tdp_w":28,"cpu_tdp_is_generic":false,
   "gpu_model":null,"gpu_count":0,"gpu_method":null,
   "ram_total_gb":31.1,"ram_method":"estimate",
   "descriptions":["RAM()","CPU(Cpu Load)"]},
 "location":{"country_name":"France","country_iso_code":"FRA","region":null,
             "carbon_intensity_g_per_kwh":56.0},
 "config":{…effective config, with "api_key" replaced by "***"…}}
```
`cpu_method` is one of `intel_rapl`, `windows_emi`, `intel_power_gadget`,
`apple_powermetrics`, `cpu_load`, `constant`, `raspberry_pmic`, or whatever
CodeCarbon reports (pass the string through; the UI only needs it for
display). `gpu_method` is `nvml`, `amdsmi`, `apple_powermetrics` or `null`. `carbon_intensity_g_per_kwh` may be `null` if it cannot be
derived.

### `measurement` — every `interval_secs` while running
```json
{"event":"measurement","ts":"…","duration_s":12.3,
 "power_w":{"cpu":5.2,"gpu":0.0,"ram":3.0,"total":8.2},
 "energy_kwh":{"cpu":1.1e-5,"gpu":0.0,"ram":0.9e-5,"total":2.0e-5},
 "emissions_kg":1.1e-6,"emissions_rate_kg_s":9.0e-8,
 "cpu_utilization_percent":12.0,"ram_used_gb":7.4,
 "carbon_intensity_g_per_kwh":56.0}
```
`power_w` is the **instantaneous** power from the last sample (not the run
average that CodeCarbon writes in its CSV), because the UI is a live meter.
`energy_kwh` and `emissions_kg` are cumulative since `start`.

### `stopped` — after `stop`
```json
{"event":"stopped","ts":"…","summary":{
  "run_id":"uuid","started_at":"…","stopped_at":"…","duration_s":600.4,
  "energy_kwh":{…},"emissions_kg":…, "emissions_rate_kg_s":…,
  "avg_power_w":{"cpu":…,"gpu":…,"ram":…,"total":…},
  "carbon_intensity_g_per_kwh":56.0,
  "output_file":"/path/to/emissions.csv"}}
```
`output_file` is `null` when `save_to_file` is false.

### `status` — reply to `status`
```json
{"event":"status","ts":"…","state":"idle"|"running","last_measurement":{…}|null}
```
`last_measurement` is the whole last `measurement` event; it is reset at
`start` and kept after `stop`.

### `log` — notable CodeCarbon warnings forwarded for display
```json
{"event":"log","ts":"…","level":"warning","message":"No CPU tracking mode found. Falling back on CPU load mode."}
```
Levels follow Python logging (`warning`, `error`, `critical`). CodeCarbon's
own noise ("Multiple instances … allowed", "graceful shutdown…") is dropped.
Nothing is forwarded when the configured `log_level` is above `warning`.

### `error`
```json
{"event":"error","ts":"…","code":"already_running","message":"…"}
```
Codes and when they are used:

* `bad_command` — unknown command, invalid JSON line, or a malformed
  `config` (wrong types, country code not 3 letters, invalid `tracking_mode`
  / `log_level`, `interval_secs` ≤ 0).
* `already_running` / `not_running` — `start` while running, `stop` while idle.
* `tracker_init_failed` — unknown country, `output_dir` not creatable,
  `save_to_api` without `api_key` or `experiment_id`, or CodeCarbon itself
  failed to start (its `start()` swallows exceptions; the agent detects the
  failure because the tracker never gets a start time).
* `internal` — unexpected exception. One `internal` error
  (`"measurement failed: …"`) is emitted per run of consecutive failed samples;
  sampling continues and the run stays `running`.

After `bad_command` and `tracker_init_failed` the agent is back in `idle` and
accepts a new `start`.

## Exit codes and signals

`0` normal (`quit`, EOF, SIGTERM/SIGINT/SIGHUP — SIGBREAK on Windows — all
stop a running tracker first), `1` unexpected crash (traceback on stderr) or
watchdog kill (the agent stuck for > 10 s while exiting), `2` bad CLI usage.
`quit`/EOF received while a `start` is in progress is handled once that start
has finished.

## CLI

```
codecarbon-agent               # protocol mode (default)
codecarbon-agent --version     # prints "codecarbon-agent 0.1.0 (codecarbon 3.3.2)"
codecarbon-agent --selftest    # starts a 3 s run with the real tracker, prints the
                               # events, exits 0 on success. Used by CI on every OS.
```

## Protocol version 2: CodeCarbon dashboard account (API)

Added so the app can connect to the free CodeCarbon dashboard
(<https://dashboard.codecarbon.io>) the same way `codecarbon login` /
`codecarbon config` do: an OpenID Connect login (authorization code + PKCE
against `https://authentication.codecarbon.io/realms/codecarbon`, public
client `codecarbon-cli`, redirect `http://localhost:8090/callback`, scope
`openid offline_access`), then the REST API at `https://api.codecarbon.io`
with the bearer token to pick or create an organisation → project →
experiment, and finally a **project API key** (`POST /projects/{id}/api-tokens`)
that the tracker uses through `save_to_api` + `api_key` + `experiment_id`.

The agent is told where to keep the credentials with a CLI flag:
`codecarbon-agent --credentials <path-to-credentials.json>` (created with
mode 0600; without the flag no login is possible and `auth_*` commands reply
`error(auth_unavailable)`). The access/refresh tokens never leave the agent:
the app only ever sees the user's display name and e-mail.

`ready.protocol_version` is now `2`.

### Commands

| `cmd` | Fields | Reply |
| --- | --- | --- |
| `auth_start` | – | `auth_url` at once (the app opens it in the browser), then `auth_result` when the browser comes back or after 5 min. A second `auth_start` while one is pending cancels the first (`auth_result` with `cancelled`). |
| `auth_cancel` | – | `auth_result` with `cancelled: true` if one was pending, else nothing. |
| `auth_status` | – | `auth_status`. Validates / refreshes the stored token; never opens a browser. |
| `auth_logout` | – | deletes the credentials file, then `auth_status` (`logged_in: false`). |
| `api_catalog` | – | `api_catalog`: every organisation the user belongs to with its projects and their experiments. |
| `api_create` | `kind`: `organization` \| `project` \| `experiment`, `name`, optional `description`, `parent_id` (organisation id for a project, project id for an experiment; omitted for an organisation) | `api_created`. Experiments are created with `on_cloud: false` and no country override (the tracker fills location in). |
| `api_key` | `project_id` | `api_key`: a fresh API key for the project (the app stores it in its settings and passes it as `config.api_key`). |

All `auth_*` / `api_*` commands are allowed while a measurement is running.
They never block the measurement loop (run on their own thread).

### Events

```json
{"event":"auth_url","ts":"…","url":"https://authentication.codecarbon.io/…"}
{"event":"auth_result","ts":"…","ok":true,"user":{"name":"Ada","email":"ada@example.org"}}
{"event":"auth_result","ts":"…","ok":false,"cancelled":false,"message":"Authorization failed: access_denied"}
{"event":"auth_status","ts":"…","logged_in":true,"user":{"name":"…","email":"…"},"api_endpoint":"https://api.codecarbon.io"}
{"event":"api_catalog","ts":"…","organizations":[{"id":"…","name":"…","projects":[{"id":"…","name":"…","experiments":[{"id":"…","name":"…"}]}]}]}
{"event":"api_created","ts":"…","kind":"project","item":{"id":"…","name":"…","parent_id":"…"}}
{"event":"api_key","ts":"…","project_id":"…","api_key":"…"}
```
`user` is taken from `GET /auth/check` (which also creates the dashboard
account on first call), falling back to the ID token claims; fields may be
`null`.

Details of the implemented behaviour:

* `auth_result` shapes: success `{ok:true, user}`; failure
  `{ok:false, cancelled:false, code:"auth_failed", message}`; cancel
  `{ok:false, cancelled:true, message:"login cancelled"}`. The 5-minute
  timeout is a failure with message "no answer from the browser after 5 min,
  login abandoned".
* If `auth_start` fails before a URL exists (provider unreachable, port 8090
  busy) there is **no** `auth_url`, only the `auth_result`.
* If `GET /auth/check` fails after the code exchange the login fails and
  nothing is saved.
* A `401` from the API is reported as `error(not_logged_in)`; other HTTP /
  network failures as `error(api_error)` with messages like
  `HTTP <status>: <detail>` or `cannot reach the CodeCarbon API: …`.
* Token refresh: only a refusal from the token endpoint (4xx) deletes the
  credentials. A network failure keeps them: `auth_status` still answers
  `logged_in: true` with the cached user and `api_*` commands reply
  `api_error`, so going offline does not sign the user out.
* Catalog items also carry `description`; `api_created.item.parent_id` is
  the organisation id for a project, the project id for an experiment and
  `null` for an organisation.
* `error` events do not say which request they answer; the app treats any
  `api_error` as the end of every pending account operation.
* Agents older than v2 reject `--credentials` with exit code 2; the app then
  restarts them without the flag and shows that a newer engine is needed. Creating an organisation whose name already
  exists returns the existing one (CodeCarbon library behaviour).
* Invalid `api_create` fields or a missing `api_key.project_id` give
  `error(bad_command)`. `description` defaults to `""`.
* `auth_cancel` is queued behind any account command already running (at
  most ~15 s, the API timeout); `quit`/EOF cancel a pending login at once.
* The credentials file holds the token response plus a cached `user`
  object, so `auth_status` needs no network while the token is valid.

### Errors

New codes: `auth_unavailable` (no `--credentials` path), `not_logged_in`
(`api_*` without a valid session; the app should show the sign-in button),
`api_error` (HTTP / network failure; `message` holds the API's text),
`auth_failed` (login flow failed; carried inside `auth_result`, not as an
`error` event).

### Environment overrides (for staging servers)

`CODECARBON_API_ENDPOINT`, `AUTH_SERVER_WELL_KNOWN`, `AUTH_CLIENT_ID` are
honoured by the agent, with the production defaults above.
