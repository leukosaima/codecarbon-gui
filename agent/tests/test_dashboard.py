"""AccountService: the auth_* / api_* commands (fake provider, fake ApiClient)."""

from __future__ import annotations

import json
import socket
import stat
import sys
import threading
import time
import urllib.parse

import pytest
import requests

from codecarbon_agent import dashboard
from codecarbon_agent.dashboard import AccountService, DashboardClient

from .conftest import EventLog, auth_config, browser_get, callback_url, http_error

ENDPOINT = "https://api.example.test"


@pytest.fixture
def make_service(tmp_path, fake_oidc, fake_api):
    services = []

    def make(**config_overrides):
        log = EventLog()
        service = AccountService(
            tmp_path / "cfg" / "credentials.json",
            log,
            auth_config=auth_config(fake_oidc, **config_overrides),
            api_endpoint=ENDPOINT + "/",
            client_factory=fake_api.factory,
        )
        services.append(service)
        return service, log

    yield make
    for service in services:
        service.shutdown()


def logged_in(service, oidc, **extra):
    creds = {**oidc.tokens(), **extra}
    service.store.save(creds)
    return creds


def login(service, log, oidc):
    """Drive auth_start → browser callback; return the auth_result event."""
    oidc.token_replies.append((200, oidc.tokens()))
    seen = len(log.events)
    service.handle({"cmd": "auth_start"})
    url = log.wait("auth_url", after=seen)["url"]
    status, _ = browser_get(callback_url(url, code="c0de"))
    assert status == 200
    return log.wait("auth_result", after=seen)


# --------------------------------------------------------------------------- auth


def test_auth_status_logged_out(make_service):
    service, log = make_service()
    service.handle({"cmd": "auth_status"})
    assert log.events[-1]["event"] == "auth_status"
    assert log.events[-1]["logged_in"] is False
    assert log.events[-1]["user"] is None
    assert log.events[-1]["api_endpoint"] == ENDPOINT


def test_api_endpoint_from_env(monkeypatch, tmp_path):
    monkeypatch.setenv("CODECARBON_API_ENDPOINT", "https://staging.example/")
    service = AccountService(tmp_path / "c.json", EventLog())
    assert service.api_endpoint == "https://staging.example"
    monkeypatch.delenv("CODECARBON_API_ENDPOINT")
    assert AccountService(tmp_path / "c.json", EventLog()).api_endpoint == (
        "https://api.codecarbon.io"
    )


def test_login_success_saves_credentials(make_service, fake_oidc, fake_api):
    service, log = make_service()
    result = login(service, log, fake_oidc)
    assert result == {**result, "ok": True, "user": {"name": "Ada", "email": "ada@example.org"}}
    assert log.names() == ["auth_url", "auth_result"]
    assert fake_api.calls == [("check_auth", None)]  # registers the user on the dashboard
    creds = json.loads(service.store.path.read_text())
    assert creds["access_token"] == fake_api.tokens[-1]
    assert creds["refresh_token"].startswith("refresh-")
    assert creds["user"] == {"name": "Ada", "email": "ada@example.org"}
    if sys.platform != "win32":
        assert stat.S_IMODE(service.store.path.stat().st_mode) == 0o600
    assert not service.login_pending
    # the tokens never leave the agent
    assert creds["access_token"] not in json.dumps(log.events)


def test_login_user_falls_back_to_id_token(make_service, fake_oidc, fake_api):
    service, log = make_service()
    fake_api.user = {"sub": "user-1"}  # no name / e-mail in /auth/check
    result = login(service, log, fake_oidc)
    assert result["ok"] is True
    assert result["user"] == {"name": "Ada Lovelace", "email": "ada@example.org"}


def test_login_auth_check_failure(make_service, fake_oidc, fake_api):
    service, log = make_service()
    fake_api.fail["check_auth"] = http_error(500, {"detail": "database down"})
    result = login(service, log, fake_oidc)
    assert result["ok"] is False and result["cancelled"] is False
    assert result["code"] == "auth_failed"
    assert result["message"] == "Authentication check failed: HTTP 500: database down"
    assert not service.store.path.exists()


def test_login_provider_error(make_service):
    service, log = make_service()
    service.handle({"cmd": "auth_start"})
    url = log.wait("auth_url")["url"]
    browser_get(callback_url(url, error="access_denied"))
    result = log.wait("auth_result")
    assert result["ok"] is False
    assert result["message"] == "Authorization failed: access_denied"


def test_auth_cancel(make_service):
    service, log = make_service()
    service.handle({"cmd": "auth_cancel"})  # nothing pending: nothing emitted
    assert log.events == []
    service.handle({"cmd": "auth_start"})
    url = log.wait("auth_url")["url"]
    redirect_uri = dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(url).query))["redirect_uri"]
    port = urllib.parse.urlsplit(redirect_uri).port
    service.handle({"cmd": "auth_cancel"})
    result = log.events[-1]  # emitted before auth_cancel returns
    assert result["event"] == "auth_result"
    assert result["ok"] is False and result["cancelled"] is True
    assert not service.login_pending
    with socket.socket() as sock, pytest.raises(ConnectionRefusedError):
        sock.connect(("127.0.0.1", port))


def test_second_auth_start_cancels_the_first(make_service, fake_oidc):
    service, log = make_service()
    service.handle({"cmd": "auth_start"})
    first = log.wait("auth_url")["url"]
    service.handle({"cmd": "auth_start"})
    assert log.names() == ["auth_url", "auth_result", "auth_url"]
    assert log.events[1]["cancelled"] is True
    second = log.events[2]["url"]
    assert first != second
    fake_oidc.token_replies.append((200, fake_oidc.tokens()))
    browser_get(callback_url(second, code="c"))
    assert log.wait("auth_result", after=3)["ok"] is True


def test_login_timeout(make_service):
    service, log = make_service(login_timeout_secs=0.2)
    service.handle({"cmd": "auth_start"})
    result = log.wait("auth_result", timeout=3)
    assert result["ok"] is False and result["cancelled"] is False
    assert "no answer from the browser" in result["message"]


def test_auth_start_port_busy(make_service):
    with socket.socket() as busy:
        busy.bind(("localhost", 0))
        busy.listen()
        port = busy.getsockname()[1]
        service, log = make_service(redirect_port=port)
        service.handle({"cmd": "auth_start"})
    assert log.names() == ["auth_result"]
    assert log.events[0]["message"] == f"port {port} is already in use"
    assert log.events[0]["code"] == "auth_failed"


def test_shutdown_cancels_pending_login(make_service):
    service, log = make_service()
    service.handle({"cmd": "auth_start"})
    log.wait("auth_url")
    service.shutdown()
    assert log.events[-1]["cancelled"] is True
    service.handle({"cmd": "auth_start"})  # queued before quit: ignored
    assert log.names() == ["auth_url", "auth_result"]


def test_shutdown_racing_with_auth_url(tmp_path, fake_oidc, fake_api):
    """quit / EOF right after ``auth_url`` (before the login thread started)."""
    outcome = {}
    log = EventLog()

    def emit(event):
        log(event)
        if event["event"] == "auth_url":

            def quit_now():
                try:
                    service.shutdown()
                except Exception as exc:  # noqa: BLE001
                    outcome["error"] = exc

            racer = threading.Thread(target=quit_now)
            outcome["thread"] = racer
            racer.start()
            time.sleep(0.1)

    service = AccountService(
        tmp_path / "c.json",
        emit,
        auth_config=auth_config(fake_oidc),
        client_factory=fake_api.factory,
    )
    service.handle({"cmd": "auth_start"})
    outcome["thread"].join(5)
    assert "error" not in outcome
    assert log.names() == ["auth_url", "auth_result"]
    assert log.events[-1]["cancelled"] is True


def test_auth_status_uses_cached_user_without_network(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc, user={"name": "Cached", "email": "c@example.org"})
    service.handle({"cmd": "auth_status"})
    hits = len(fake_oidc.hits)
    service.handle({"cmd": "auth_status"})
    assert log.events[-1]["logged_in"] is True
    assert log.events[-1]["user"] == {"name": "Cached", "email": "c@example.org"}
    assert fake_api.calls == []
    assert len(fake_oidc.hits) == hits  # JWKS cached


def test_auth_status_fetches_and_caches_user(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc)
    service.handle({"cmd": "auth_status"})
    assert log.events[-1]["user"] == {"name": "Ada", "email": "ada@example.org"}
    assert json.loads(service.store.path.read_text())["user"]["name"] == "Ada"
    service.handle({"cmd": "auth_status"})
    assert fake_api.calls == [("check_auth", None)]


def test_auth_status_refreshes(make_service, fake_oidc):
    service, log = make_service()
    logged_in(service, fake_oidc, access_token=fake_oidc.jwt(exp=1), user={"name": "A"})
    fresh = fake_oidc.tokens()
    fake_oidc.token_replies.append((200, fresh))
    service.handle({"cmd": "auth_status"})
    assert log.events[-1]["logged_in"] is True
    assert json.loads(service.store.path.read_text())["access_token"] == fresh["access_token"]


def test_auth_status_refresh_refused_logs_out(make_service, fake_oidc):
    service, log = make_service()
    logged_in(service, fake_oidc, access_token=fake_oidc.jwt(exp=1))
    service.handle({"cmd": "auth_status"})  # token endpoint answers 400
    assert log.events[-1]["logged_in"] is False
    assert not service.store.path.exists()


def test_auth_status_offline_keeps_session(make_service, fake_oidc):
    service, log = make_service(well_known_url="http://127.0.0.1:9/unreachable")
    logged_in(service, fake_oidc, access_token=fake_oidc.jwt(exp=1), user={"name": "A"})
    service.handle({"cmd": "auth_status"})
    assert log.events[-1]["logged_in"] is True
    assert service.store.path.exists()


def test_auth_logout(make_service, fake_oidc):
    service, log = make_service()
    logged_in(service, fake_oidc)
    service.handle({"cmd": "auth_logout"})
    assert log.events[-1]["event"] == "auth_status"
    assert log.events[-1]["logged_in"] is False
    assert not service.store.path.exists()


# --------------------------------------------------------------------------- api


@pytest.mark.parametrize(
    "message",
    [
        {"cmd": "api_catalog"},
        {"cmd": "api_create", "kind": "organization", "name": "x"},
        {"cmd": "api_key", "project_id": "p1"},
    ],
)
def test_api_not_logged_in(make_service, fake_api, message):
    service, log = make_service()
    service.handle(message)
    assert log.events[-1]["event"] == "error"
    assert log.events[-1]["code"] == "not_logged_in"
    assert fake_api.calls == []


def test_api_catalog(make_service, fake_oidc, fake_api):
    service, log = make_service()
    creds = logged_in(service, fake_oidc)
    service.handle({"cmd": "api_catalog"})
    event = log.events[-1]
    assert event["event"] == "api_catalog"
    assert event["organizations"] == [
        {
            "id": "o1",
            "name": "Org",
            "description": "d",
            "projects": [
                {
                    "id": "p1",
                    "name": "Proj",
                    "description": "pd",
                    "experiments": [{"id": "e1", "name": "Exp", "description": "ed"}],
                }
            ],
        }
    ]
    assert fake_api.tokens == [creds["access_token"]]
    assert fake_api.endpoint == ENDPOINT


def test_api_catalog_http_error(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc)
    fake_api.fail["list_projects_from_organization"] = http_error(403, {"detail": "Forbidden!"})
    service.handle({"cmd": "api_catalog"})
    assert log.events[-1] == {**log.events[-1], "event": "error", "code": "api_error"}
    assert log.events[-1]["message"] == "HTTP 403: Forbidden!"


def test_api_unauthorized_is_not_logged_in(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc)
    fake_api.fail["get_list_organizations"] = http_error(401, {"detail": "token rejected"})
    service.handle({"cmd": "api_catalog"})
    assert log.events[-1]["code"] == "not_logged_in"
    assert log.events[-1]["message"] == "HTTP 401: token rejected"


def test_api_network_error(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc)
    fake_api.fail["get_list_organizations"] = requests.ConnectionError("no route")
    service.handle({"cmd": "api_catalog"})
    assert log.events[-1]["code"] == "api_error"
    assert "cannot reach the CodeCarbon API" in log.events[-1]["message"]


def test_api_create_each_kind(make_service, fake_oidc, fake_api):
    service, log = make_service()
    logged_in(service, fake_oidc)

    service.handle({"cmd": "api_create", "kind": "organization", "name": " Org2 "})
    assert log.events[-1] == {
        **log.events[-1],
        "event": "api_created",
        "kind": "organization",
        "item": {"id": "o2", "name": "Org2", "description": "", "parent_id": None},
    }
    org = fake_api.calls[-1][1]
    assert (org.name, org.description) == ("Org2", "")

    service.handle(
        {"cmd": "api_create", "kind": "project", "name": "P", "description": "D", "parent_id": "o1"}
    )
    assert log.events[-1]["item"] == {
        "id": "p2",
        "name": "P",
        "description": None,
        "parent_id": "o1",
    }
    project = fake_api.calls[-1][1]
    assert (project.name, project.description, project.organization_id) == ("P", "D", "o1")

    service.handle({"cmd": "api_create", "kind": "experiment", "name": "E", "parent_id": "p1"})
    assert log.events[-1]["kind"] == "experiment"
    assert log.events[-1]["item"]["parent_id"] == "p1"
    experiment = fake_api.calls[-1][1]
    assert experiment.on_cloud is False
    assert experiment.project_id == "p1"
    assert experiment.country_iso_code is None and experiment.region is None
    assert isinstance(experiment.timestamp, str)  # JSON-serialisable


@pytest.mark.parametrize(
    "message",
    [
        {"kind": "team", "name": "x"},
        {"kind": "organization"},
        {"kind": "organization", "name": "  "},
        {"kind": "organization", "name": "x", "description": 3},
        {"kind": "project", "name": "x"},
        {"kind": "experiment", "name": "x", "parent_id": ""},
    ],
)
def test_api_create_bad_command(make_service, fake_oidc, fake_api, message):
    service, log = make_service()
    logged_in(service, fake_oidc)
    service.handle({"cmd": "api_create", **message})
    assert log.events[-1]["code"] == "bad_command"
    assert fake_api.calls == []


class FakeResponse:
    def __init__(self, status, body):
        self.status_code = status
        self._body = body

    def json(self):
        return self._body

    def raise_for_status(self):
        if self.status_code >= 400:
            raise http_error(self.status_code, self._body)


def test_api_key(make_service, fake_oidc, monkeypatch):
    service, log = make_service()
    creds = logged_in(service, fake_oidc)
    posts = []

    def fake_post(url, **kwargs):
        posts.append((url, kwargs))
        return FakeResponse(201, {"id": "t1", "token": "secret-key", "project_id": "p1"})

    monkeypatch.setattr(dashboard.requests, "post", fake_post)
    service.handle({"cmd": "api_key", "project_id": "p1"})
    assert log.events[-1] == {
        **log.events[-1],
        "event": "api_key",
        "project_id": "p1",
        "api_key": "secret-key",
    }
    url, kwargs = posts[0]
    assert url == f"{ENDPOINT}/projects/p1/api-tokens"
    assert kwargs["json"] == {"project_id": "p1", "name": "api token", "x_token": "???"}
    assert kwargs["headers"] == {"Authorization": f"Bearer {creds['access_token']}"}
    assert kwargs["timeout"] > 0


def test_api_key_errors(make_service, fake_oidc, monkeypatch):
    service, log = make_service()
    logged_in(service, fake_oidc)
    replies = [FakeResponse(404, {"detail": "Project not found"}), FakeResponse(201, {})]
    monkeypatch.setattr(dashboard.requests, "post", lambda url, **kw: replies.pop(0))
    service.handle({"cmd": "api_key", "project_id": "nope"})
    assert log.events[-1]["code"] == "api_error"
    assert log.events[-1]["message"] == "HTTP 404: Project not found"
    service.handle({"cmd": "api_key", "project_id": "p1"})
    assert log.events[-1]["message"] == "the API did not return a token"
    service.handle({"cmd": "api_key"})
    assert log.events[-1]["code"] == "bad_command"


def test_real_api_client_is_used_by_default(monkeypatch):
    """The default factory wraps CodeCarbon's ApiClient (bearer token, no run created)."""
    sent = []

    def fake_get(url, json=None, timeout=None, headers=None):
        sent.append((url, headers, timeout))
        return FakeResponse(200, {"user": {"name": "Ada", "email": "a@b.c"}})

    monkeypatch.setattr(requests, "get", fake_get)
    client = DashboardClient(ENDPOINT, "tok")
    assert client.check_auth() == {"name": "Ada", "email": "a@b.c"}
    url, headers, timeout = sent[0]
    assert url == f"{ENDPOINT}/auth/check"
    assert headers["Authorization"] == "Bearer tok"
    assert timeout == dashboard.API_TIMEOUT_SECS

    def failing_get(url, json=None, timeout=None, headers=None):
        return FakeResponse(422, {"detail": [{"msg": "field required"}]})

    monkeypatch.setattr(requests, "get", failing_get)
    with pytest.raises(requests.HTTPError) as info:
        client.api.get_list_organizations()
    assert dashboard.api_failure(info.value).message == "HTTP 422: field required"


def test_unexpected_success_status_is_an_error(monkeypatch):
    monkeypatch.setattr(requests, "post", lambda **kw: FakeResponse(200, {"id": "x"}))
    from codecarbon.core.schemas import ProjectCreate

    client = DashboardClient(ENDPOINT, "tok")
    with pytest.raises(requests.HTTPError, match="Unexpected status 200"):
        client.api.create_project(ProjectCreate(name="n", description="d", organization_id="o"))
