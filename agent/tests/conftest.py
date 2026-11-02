"""Shared fakes: a fake CodeCarbon tracker and a harness driving the Runner in memory."""

from __future__ import annotations

import io
import json
import queue
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from types import SimpleNamespace
from typing import Any

import pytest

from codecarbon_agent.protocol import Writer
from codecarbon_agent.runner import Runner
from codecarbon_agent.tracker import TrackerSession


class FakePower:
    def __init__(self, watts: float) -> None:
        self.W = watts


# Class names matter: the agent recognises hardware by class name (like CodeCarbon's).
class CPU:
    _mode = "cpu_load"
    _tdp = 28
    _is_generic_tdp = False

    def description(self) -> str:
        return "CPU(Cpu Load)"


class RAM:
    _force_ram_power = None

    def description(self) -> str:
        return "RAM()"


def make_data(duration: float, **overrides: Any) -> SimpleNamespace:
    """An EmissionsData look-alike: 10 W CPU + 5 W RAM, 50 g/kWh."""
    cpu_kwh = 10 * duration / 3.6e6
    ram_kwh = 5 * duration / 3.6e6
    total = cpu_kwh + ram_kwh
    values = dict(
        run_id="run-123",
        duration=duration,
        emissions=total * 0.05,
        emissions_rate=(total * 0.05 / duration) if duration else 0.0,
        cpu_power=10.0,
        gpu_power=0.0,
        ram_power=5.0,
        cpu_energy=cpu_kwh,
        gpu_energy=0.0,
        ram_energy=ram_kwh,
        energy_consumed=total,
        cpu_utilization_percent=12.5,
        ram_used_gb=7.4,
        country_name="France",
        country_iso_code="FRA",
        region=None,
    )
    values.update(overrides)
    return SimpleNamespace(**values)


class FakeTracker:
    """Mimics the parts of a CodeCarbon tracker the agent uses."""

    def __init__(
        self,
        offline: bool,
        kwargs: dict[str, Any],
        *,
        flush_delay: float = 0.0,
        fail_flushes: int = 0,
        data_overrides: dict[str, Any] | None = None,
    ) -> None:
        self.offline = offline
        self.kwargs = kwargs
        self.handlers = kwargs["output_handlers"]
        self.flush_delay = flush_delay
        self.fail_flushes = fail_flushes
        self.data_overrides = data_overrides or {}
        self.run_id = "run-123"
        self._start_time: float | None = None
        self._conf = {
            "cpu_model": "Fake CPU 9000",
            "cpu_count": 8,
            "cpu_physical_count": 4,
            "gpu_model": "",
            "gpu_count": 0,
            "ram_total_size": 31.1234,
            "hardware": ["RAM()", "CPU(Cpu Load)"],
        }
        self._hardware = [RAM(), CPU()]
        self._cpu_power = FakePower(7.5)
        self._gpu_power = FakePower(0.0)
        self._ram_power = FakePower(5.0)
        self.flushes = 0
        self.stopped = False

    def start(self) -> None:
        self._start_time = time.monotonic()

    def _emit(self) -> None:
        data = make_data(time.monotonic() - self._start_time, **self.data_overrides)
        for handler in self.handlers:
            handler.out(data, data)

    def flush(self) -> float | None:
        self.flushes += 1
        if self.flush_delay:
            time.sleep(self.flush_delay)
        if self.fail_flushes > 0:
            self.fail_flushes -= 1
            return None  # like CodeCarbon's @suppress: silently nothing
        self._emit()
        return 0.0

    def stop(self) -> float | None:
        self._emit()
        for handler in self.handlers:
            handler.exit()
        self._start_time = None
        self.stopped = True
        return 0.0


class FakeCsv:
    instances: list[FakeCsv] = []

    def __init__(self, output_dir: str) -> None:
        self.path = f"{output_dir}/emissions.csv"
        self.rows: list[Any] = []
        self._latest = None
        FakeCsv.instances.append(self)

    def out(self, total: Any, delta: Any) -> None:
        self._latest = total

    def exit(self) -> None:
        self.write()

    def discard(self) -> None:
        self._latest = None

    def write(self) -> None:
        if self._latest is not None and not self.rows:
            self.rows.append(self._latest)


def fake_session(**tracker_options: Any) -> tuple[TrackerSession, list[FakeTracker]]:
    created: list[FakeTracker] = []

    def factory(offline: bool, kwargs: dict[str, Any]) -> FakeTracker:
        tracker = FakeTracker(offline, kwargs, **tracker_options)
        created.append(tracker)
        return tracker

    session = TrackerSession(
        tracker_factory=factory, csv_factory=FakeCsv, country_codes=lambda: {"FRA", "USA"}
    )
    return session, created


class PushInput:
    """A stdin replacement fed line by line from the test."""

    def __init__(self) -> None:
        self._queue: queue.Queue[str] = queue.Queue()

    def send(self, line: str) -> None:
        self._queue.put(line + "\n")

    def close(self) -> None:
        self._queue.put("")

    def readline(self) -> str:
        return self._queue.get()


class Harness:
    """Runs a Runner in a thread over in-memory stdin / stdout."""

    def __init__(
        self, session_factory: Callable[[], TrackerSession], **runner_options: Any
    ) -> None:
        self.stdin = PushInput()
        self.stdout = io.StringIO()
        self.writer = Writer(self.stdout)
        self.runner = Runner(self.stdin, self.writer, session_factory, **runner_options)
        self.exit_code: int | None = None
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _run(self) -> None:
        self.exit_code = self.runner.run()

    def send(self, obj: Any) -> None:
        self.stdin.send(obj if isinstance(obj, str) else json.dumps(obj))

    def events(self) -> list[dict[str, Any]]:
        return [json.loads(line) for line in self.stdout.getvalue().splitlines()]

    def names(self) -> list[str]:
        return [e["event"] for e in self.events()]

    def wait_for(
        self, predicate: Callable[[dict[str, Any]], bool], timeout: float = 5.0, after: int = 0
    ) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for event in self.events()[after:]:
                if predicate(event):
                    return event
            time.sleep(0.01)
        raise AssertionError(f"event not seen; got {self.names()}")

    def wait_event(self, name: str, timeout: float = 5.0, after: int = 0) -> dict[str, Any]:
        return self.wait_for(lambda e: e["event"] == name, timeout, after)

    def request(self, obj: Any, name: str, timeout: float = 5.0) -> dict[str, Any]:
        """Send a command and wait for the next event called ``name``."""
        seen = len(self.events())
        self.send(obj)
        return self.wait_event(name, timeout, after=seen)

    def join(self, timeout: float = 10.0) -> int | None:
        self._thread.join(timeout)
        assert not self._thread.is_alive(), "runner did not exit"
        return self.exit_code


@pytest.fixture
def harness_factory():
    harnesses: list[Harness] = []

    def make(
        runner_options: dict[str, Any] | None = None, **tracker_options: Any
    ) -> tuple[Harness, list[FakeTracker]]:
        session, created = fake_session(**tracker_options)
        harness = Harness(lambda: session, **(runner_options or {}))
        harnesses.append(harness)
        harness.wait_event("ready")
        return harness, created

    yield make
    for harness in harnesses:
        harness.stdin.close()
        harness.join()


# --------------------------------------------------------------------------- protocol v2 fakes


@pytest.fixture(autouse=True)
def _no_proxy_for_loopback(monkeypatch):
    """The fake servers live on 127.0.0.1 / localhost: never go through a proxy."""
    for name in ("NO_PROXY", "no_proxy"):
        monkeypatch.setenv(name, "*")


@pytest.fixture(scope="session")
def rsa_key():
    from joserfc.jwk import RSAKey

    return RSAKey.generate_key(2048, parameters={"kid": "test-key"})


class FakeOidc:
    """A tiny OpenID provider on 127.0.0.1: discovery, JWKS and token endpoints.

    ``token_replies`` is a queue of ``(status, json)`` answers for the token
    endpoint (default: 400 ``invalid_grant``); ``token_requests`` records the
    form bodies it received.
    """

    def __init__(self, key: Any) -> None:
        self.key = key
        self.token_replies: list[tuple[int, Any]] = []
        self.token_requests: list[dict[str, str]] = []
        self.hits: list[str] = []
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                fake.hits.append(self.path)
                if self.path == "/.well-known/openid-configuration":
                    self._json(200, fake.discovery)
                elif self.path == "/jwks":
                    from joserfc.jwk import KeySet

                    self._json(200, KeySet([fake.key]).as_dict(private=False))
                else:
                    self._json(404, {"error": "not found"})

            def do_POST(self) -> None:  # noqa: N802
                fake.hits.append(self.path)
                length = int(self.headers.get("Content-Length") or 0)
                form = dict(urllib.parse.parse_qsl(self.rfile.read(length).decode()))
                fake.token_requests.append(form)
                status, body = (
                    fake.token_replies.pop(0)
                    if fake.token_replies
                    else (400, {"error": "invalid_grant"})
                )
                self._json(status, body)

            def _json(self, status: int, body: Any) -> None:
                data = json.dumps(body).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"
        self.well_known = f"{self.base}/.well-known/openid-configuration"
        self.discovery = {
            "issuer": self.base,
            "authorization_endpoint": f"{self.base}/auth",
            "token_endpoint": f"{self.base}/token",
            "jwks_uri": f"{self.base}/jwks",
        }
        self._thread = threading.Thread(
            target=self.server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True
        )
        self._thread.start()

    def jwt(self, key: Any = None, **claims: Any) -> str:
        from joserfc import jwt

        now = int(time.time())
        payload = {"iss": self.base, "sub": "user-1", "iat": now, "exp": now + 300, **claims}
        return jwt.encode({"alg": "RS256", "kid": "test-key"}, payload, key or self.key)

    def tokens(self, **access_claims: Any) -> dict[str, Any]:
        """A token-endpoint answer like Keycloak's."""
        return {
            "access_token": self.jwt(**access_claims),
            "refresh_token": f"refresh-{time.monotonic_ns()}",
            "id_token": self.jwt(name="Ada Lovelace", email="ada@example.org"),
            "token_type": "Bearer",
            "expires_in": 300,
        }

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()


@pytest.fixture
def fake_oidc(rsa_key):
    oidc = FakeOidc(rsa_key)
    yield oidc
    oidc.close()


def auth_config(oidc: FakeOidc, **overrides: Any):
    """An AuthConfig pointing at the fake provider, callback on a free port."""
    from codecarbon_agent.auth import AuthConfig

    values: dict[str, Any] = {
        "well_known_url": oidc.well_known,
        "client_id": "test-client",
        "redirect_port": 0,
        "http_timeout_secs": 5.0,
    }
    values.update(overrides)
    return AuthConfig(**values)


def browser_get(url: str) -> tuple[int, str]:
    """What the browser does when the provider redirects to the callback."""
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(url, timeout=5) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.read().decode()


def callback_url(auth_url: str, **params: str) -> str:
    """The redirect the provider would send for ``auth_url`` (state copied unless given)."""
    query = dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(auth_url).query))
    values = {"state": query["state"], **params}
    return f"{query['redirect_uri']}?{urllib.parse.urlencode(values)}"


class FakeApi:
    """Stands in for CodeCarbon's ``ApiClient`` (same method names)."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, Any]] = []
        self.tokens: list[str] = []
        self.user: Any = {"sub": "user-1", "name": "Ada", "email": "ada@example.org"}
        self.fail: dict[str, Exception] = {}
        self.orgs = [{"id": "o1", "name": "Org", "description": "d"}]
        self.projects = {"o1": [{"id": "p1", "name": "Proj", "description": "pd"}]}
        self.experiments = {"p1": [{"id": "e1", "name": "Exp", "description": "ed"}]}

    def factory(self, endpoint: str, access_token: str) -> FakeApi:
        self.endpoint = endpoint
        self.tokens.append(access_token)
        return self

    def _call(self, name: str, arg: Any = None) -> None:
        self.calls.append((name, arg))
        if name in self.fail:
            raise self.fail[name]

    def check_auth(self):
        self._call("check_auth")
        return {"user": self.user}

    def get_list_organizations(self):
        self._call("get_list_organizations")
        return self.orgs

    def list_projects_from_organization(self, organization_id):
        self._call("list_projects_from_organization", organization_id)
        return self.projects.get(organization_id, [])

    def list_experiments_from_project(self, project_id):
        self._call("list_experiments_from_project", project_id)
        return self.experiments.get(project_id, [])

    def create_organization(self, organization):
        self._call("create_organization", organization)
        return {"id": "o2", "name": organization.name, "description": organization.description}

    def create_project(self, project):
        self._call("create_project", project)
        return {"id": "p2", "name": project.name, "organization_id": project.organization_id}

    def add_experiment(self, experiment):
        self._call("add_experiment", experiment)
        return {"id": "e2", "name": experiment.name, "project_id": experiment.project_id}


def http_error(status: int, body: Any):
    """A ``requests.HTTPError`` carrying a real ``Response``."""
    import requests

    response = requests.Response()
    response.status_code = status
    response._content = json.dumps(body).encode() if not isinstance(body, bytes) else body
    response.reason = "Error"
    return requests.HTTPError(f"{status} Error", response=response)


@pytest.fixture
def fake_api():
    return FakeApi()


class EventLog:
    """An ``emit`` callback that records events and lets a test wait for one."""

    def __init__(self) -> None:
        self.events: list[dict[str, Any]] = []
        self._cond = threading.Condition()

    def __call__(self, event: Any) -> None:
        with self._cond:
            self.events.append(json.loads(json.dumps(dict(event))))
            self._cond.notify_all()

    def names(self) -> list[str]:
        return [e["event"] for e in self.events]

    def wait(self, name: str, timeout: float = 5.0, after: int = 0) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        with self._cond:
            while True:
                for event in self.events[after:]:
                    if event["event"] == name:
                        return event
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise AssertionError(f"no {name!r} event; got {self.names()}")
                self._cond.wait(remaining)
