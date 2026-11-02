"""CodeCarbon dashboard account: the protocol v2 ``auth_*`` / ``api_*`` commands.

:class:`AccountService` implements the commands on top of
:class:`~codecarbon_agent.auth.Authenticator` (login, tokens) and
:class:`DashboardClient` (REST API, through CodeCarbon's own ``ApiClient``).
The runner calls :meth:`AccountService.handle` from a dedicated worker thread;
a pending browser login waits on a thread of its own, so the worker stays free
for ``auth_cancel`` and the other commands.
"""

from __future__ import annotations

import contextlib
import os
import threading
from collections.abc import Callable, Mapping
from datetime import datetime
from typing import Any

import requests

from .auth import (
    AuthConfig,
    Authenticator,
    AuthError,
    AuthNetworkError,
    CredentialStore,
    LoginAttempt,
    LoginCancelled,
    NotLoggedIn,
    unverified_claims,
    user_from_claims,
)
from .protocol import (
    API_ERROR,
    BAD_COMMAND,
    NOT_LOGGED_IN,
    api_catalog_event,
    api_created_event,
    api_key_event,
    auth_failure_event,
    auth_status_event,
    auth_success_event,
    auth_url_event,
    error_event,
)

DEFAULT_API_ENDPOINT = "https://api.codecarbon.io"
API_TIMEOUT_SECS = 15.0
KINDS = ("organization", "project", "experiment")
LOGIN_JOIN_SECS = 5.0

Emit = Callable[[Mapping[str, Any]], Any]


def api_endpoint_from_env() -> str:
    return (os.environ.get("CODECARBON_API_ENDPOINT") or DEFAULT_API_ENDPOINT).rstrip("/")


class ApiFailure(Exception):
    """An API call failed; ``code`` is the protocol error code to report."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


def _response_message(response: requests.Response | None) -> str:
    """``HTTP 403: <the API's own message>``."""
    if response is None:
        return "no answer from the API"
    detail: Any = None
    try:
        body = response.json()
        detail = body.get("detail", body) if isinstance(body, dict) else body
    except ValueError:
        detail = (response.text or "").strip()
    if isinstance(detail, list):  # FastAPI validation errors
        detail = "; ".join(
            str(item.get("msg", item)) if isinstance(item, dict) else str(item) for item in detail
        )
    text = str(detail)[:500] if detail not in (None, "", {}) else (response.reason or "")
    return f"HTTP {response.status_code}: {text}".rstrip(": ")


def api_failure(exc: Exception) -> ApiFailure:
    """Map a ``requests`` exception to ``api_error`` (or ``not_logged_in`` for a 401)."""
    if isinstance(exc, requests.HTTPError):
        response = exc.response
        code = NOT_LOGGED_IN if response is not None and response.status_code == 401 else API_ERROR
        return ApiFailure(code, _response_message(response))
    if isinstance(exc, requests.RequestException):
        return ApiFailure(API_ERROR, f"cannot reach the CodeCarbon API: {exc}")
    return ApiFailure(API_ERROR, f"{type(exc).__name__}: {exc}")


def _make_api_client(endpoint: str, access_token: str) -> Any:
    """CodeCarbon's ``ApiClient`` with a longer timeout and no error logging.

    (``ApiClient`` logs every failure on the ``codecarbon`` logger, which the
    agent forwards as ``log`` events; the failure is already an ``api_error``.)
    """
    from codecarbon.core.api_client import ApiClient

    class _Client(ApiClient):
        def _request(self, method, url, payload=None, expected_status=200):  # type: ignore[override]
            response = method(
                url=url, json=payload, timeout=API_TIMEOUT_SECS, headers=self._get_headers()
            )
            if response.status_code != expected_status:
                response.raise_for_status()
                raise requests.HTTPError(
                    f"Unexpected status {response.status_code} from {url}", response=response
                )
            return response

    client = _Client(endpoint_url=endpoint, create_run_automatically=False)
    client.set_access_token(access_token)
    return client


def _item(raw: Any, parent_id: str | None = None) -> dict[str, Any]:
    raw = raw if isinstance(raw, Mapping) else {}
    item = {
        "id": str(raw["id"]) if raw.get("id") is not None else None,
        "name": raw.get("name"),
        "description": raw.get("description"),
    }
    if parent_id is not None:
        item["parent_id"] = parent_id
    return item


def _as_list(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


class DashboardClient:
    """The REST calls the app needs, with a bearer token."""

    def __init__(self, endpoint: str, access_token: str, client_factory=_make_api_client) -> None:
        self.endpoint = endpoint.rstrip("/")
        self.access_token = access_token
        self.api = client_factory(self.endpoint, access_token)

    def check_auth(self) -> dict[str, Any] | None:
        """``GET /auth/check`` → ``{"user": <access-token claims>}`` (registers the user)."""
        answer = self.api.check_auth()
        claims = answer.get("user") if isinstance(answer, Mapping) else None
        return user_from_claims(claims)

    def catalog(self) -> list[dict[str, Any]]:
        organizations = []
        for raw_org in _as_list(self.api.get_list_organizations()):
            org = _item(raw_org)
            org["projects"] = []
            for raw_project in _as_list(self.api.list_projects_from_organization(org["id"])):
                project = _item(raw_project)
                project["experiments"] = [
                    _item(raw)
                    for raw in _as_list(self.api.list_experiments_from_project(project["id"]))
                ]
                org["projects"].append(project)
            organizations.append(org)
        return organizations

    def create(
        self, kind: str, name: str, description: str, parent_id: str | None
    ) -> dict[str, Any]:
        from codecarbon.core.schemas import ExperimentCreate, OrganizationCreate, ProjectCreate

        if kind == "organization":
            created = self.api.create_organization(
                OrganizationCreate(name=name, description=description)
            )
        elif kind == "project":
            created = self.api.create_project(
                ProjectCreate(name=name, description=description, organization_id=parent_id)
            )
        else:
            created = self.api.add_experiment(
                ExperimentCreate(
                    timestamp=datetime.now().astimezone().isoformat(),
                    name=name,
                    description=description,
                    on_cloud=False,
                    project_id=parent_id,
                )
            )
        return _item(created, parent_id)

    def project_api_key(self, project_id: str) -> str:
        """``POST /projects/{id}/api-tokens``, with the body ``codecarbon config`` sends."""
        response = requests.post(
            f"{self.endpoint}/projects/{project_id}/api-tokens",
            json={"project_id": project_id, "name": "api token", "x_token": "???"},
            headers={"Authorization": f"Bearer {self.access_token}"},
            timeout=API_TIMEOUT_SECS,
        )
        response.raise_for_status()
        token = response.json().get("token")
        if not isinstance(token, str) or not token:
            raise ApiFailure(API_ERROR, "the API did not return a token")
        return token


class AccountService:
    """Serves the ``auth_*`` / ``api_*`` commands for one credentials file."""

    def __init__(
        self,
        credentials_path: str | os.PathLike[str],
        emit: Emit,
        *,
        auth_config: AuthConfig | None = None,
        api_endpoint: str | None = None,
        client_factory: Callable[[str, str], Any] = _make_api_client,
    ) -> None:
        self.store = CredentialStore(credentials_path)
        self.auth = Authenticator(self.store, auth_config)
        self.api_endpoint = (api_endpoint or api_endpoint_from_env()).rstrip("/")
        self._client_factory = client_factory
        self._emit = emit
        self._lock = threading.Lock()
        self._login: LoginAttempt | None = None
        self._login_thread: threading.Thread | None = None
        self._closed = False

    # ----------------------------------------------------------------- dispatch

    def handle(self, message: Mapping[str, Any]) -> None:
        handlers: dict[str, Callable[[Mapping[str, Any]], None]] = {
            "auth_start": self.auth_start,
            "auth_cancel": self.auth_cancel,
            "auth_status": self.auth_status,
            "auth_logout": self.auth_logout,
            "api_catalog": self.api_catalog,
            "api_create": self.api_create,
            "api_key": self.api_key,
        }
        handlers[str(message.get("cmd"))](message)

    def shutdown(self) -> None:
        """Cancel a pending login and wait (briefly) for it to release the port."""
        with self._lock:
            self._closed = True
        self._cancel_login()

    @property
    def login_pending(self) -> bool:
        with self._lock:
            return self._login is not None

    # ----------------------------------------------------------------- auth

    def auth_start(self, message: Mapping[str, Any]) -> None:
        if self._closed:  # queued before quit / EOF
            return
        self._cancel_login()  # a second auth_start replaces the first
        try:
            attempt = self.auth.begin_login()
        except AuthError as exc:
            self._emit(auth_failure_event(str(exc)))
            return
        with self._lock:
            if self._closed:
                attempt.close()
                return
            self._login = attempt
            thread = threading.Thread(
                target=self._complete_login, args=(attempt,), name="login", daemon=True
            )
            self._login_thread = thread
            # Under the lock: a concurrent cancel sees a started thread, and
            # ``auth_url`` always precedes this login's ``auth_result``.
            self._emit(auth_url_event(attempt.url))
            thread.start()

    def auth_cancel(self, message: Mapping[str, Any]) -> None:
        self._cancel_login()

    def _cancel_login(self) -> None:
        with self._lock:
            attempt, thread = self._login, self._login_thread
        if attempt is None:
            return
        attempt.cancel()
        if thread is not None and thread is not threading.current_thread():
            thread.join(LOGIN_JOIN_SECS)

    def _complete_login(self, attempt: LoginAttempt) -> None:
        try:
            tokens = attempt.wait()
            user = self._registered_user(tokens)
            if attempt.cancelled:
                raise LoginCancelled("login cancelled")
            self.store.save({**tokens, "user": user})
            event = auth_success_event(user)
        except LoginCancelled as exc:
            event = auth_failure_event(str(exc), cancelled=True)
        except AuthError as exc:
            event = auth_failure_event(str(exc))
        except Exception as exc:  # noqa: BLE001 - never kill the thread silently
            event = auth_failure_event(f"{type(exc).__name__}: {exc}")
        finally:
            attempt.close()
            with self._lock:
                if self._login is attempt:
                    self._login, self._login_thread = None, None
        self._emit(event)

    def _registered_user(self, tokens: Mapping[str, Any]) -> dict[str, Any]:
        """``GET /auth/check`` (it also creates the account on first login)."""
        try:
            user = self._client(tokens["access_token"]).check_auth()
        except Exception as exc:  # noqa: BLE001
            failure = api_failure(exc) if not isinstance(exc, ApiFailure) else exc
            raise AuthError(f"Authentication check failed: {failure.message}") from exc
        return user or self._id_token_user(tokens) or {"name": None, "email": None}

    @staticmethod
    def _id_token_user(creds: Mapping[str, Any]) -> dict[str, Any] | None:
        return user_from_claims(unverified_claims(creds.get("id_token")))

    def auth_status(self, message: Mapping[str, Any] | None = None) -> None:
        try:
            creds = self.auth.session()
        except NotLoggedIn:
            self._emit(auth_status_event(False, None, self.api_endpoint))
            return
        except AuthNetworkError:
            # Offline with an expired access token: the refresh token may still
            # be good, so the session is kept.
            creds = self.store.load() or {}
        user = creds.get("user") if isinstance(creds.get("user"), Mapping) else None
        if user is None:
            user = self._fetch_user(creds)
        self._emit(auth_status_event(True, user, self.api_endpoint))

    def _fetch_user(self, creds: dict[str, Any]) -> dict[str, Any]:
        user = None
        with contextlib.suppress(Exception):
            user = self._client(creds["access_token"]).check_auth()
        user = user or self._id_token_user(creds) or {"name": None, "email": None}
        with contextlib.suppress(AuthError):
            current = self.store.load()
            if current and current.get("access_token") == creds.get("access_token"):
                self.store.save({**current, "user": user})
        return user

    def auth_logout(self, message: Mapping[str, Any]) -> None:
        self.store.delete()
        self._emit(auth_status_event(False, None, self.api_endpoint))

    # ----------------------------------------------------------------- api

    def _client(self, access_token: str) -> DashboardClient:
        return DashboardClient(self.api_endpoint, access_token, self._client_factory)

    def _call(self, action: Callable[[DashboardClient], Any]) -> Any:
        """Run ``action`` with a logged-in client; report failures as ``error`` events."""
        try:
            try:
                creds = self.auth.session()
            except NotLoggedIn as exc:
                raise ApiFailure(NOT_LOGGED_IN, str(exc)) from exc
            except AuthNetworkError as exc:
                raise ApiFailure(API_ERROR, str(exc)) from exc
            try:
                return action(self._client(creds["access_token"]))
            except ApiFailure:
                raise
            except Exception as exc:  # noqa: BLE001 - requests / unexpected payloads
                raise api_failure(exc) from exc
        except ApiFailure as failure:
            self._emit(error_event(failure.code, failure.message))
            return None

    def api_catalog(self, message: Mapping[str, Any]) -> None:
        organizations = self._call(lambda client: client.catalog())
        if organizations is not None:
            self._emit(api_catalog_event(organizations))

    def api_create(self, message: Mapping[str, Any]) -> None:
        kind = message.get("kind")
        name = message.get("name")
        description = message.get("description")
        parent_id = message.get("parent_id")
        problem = None
        if kind not in KINDS:
            problem = f"'kind' must be one of {', '.join(KINDS)}"
        elif not isinstance(name, str) or not name.strip():
            problem = "'name' must be a non-empty string"
        elif description is not None and not isinstance(description, str):
            problem = "'description' must be a string"
        elif kind != "organization" and (not isinstance(parent_id, str) or not parent_id):
            problem = f"'parent_id' is required to create a {kind}"
        if problem is not None:
            self._emit(error_event(BAD_COMMAND, problem))
            return
        parent = parent_id if kind != "organization" else None
        item = self._call(
            lambda client: client.create(kind, name.strip(), description or "", parent)
        )
        if item is not None:
            item.setdefault("parent_id", None)
            self._emit(api_created_event(kind, item))

    def api_key(self, message: Mapping[str, Any]) -> None:
        project_id = message.get("project_id")
        if not isinstance(project_id, str) or not project_id:
            self._emit(error_event(BAD_COMMAND, "'project_id' must be a non-empty string"))
            return
        key = self._call(lambda client: client.project_api_key(project_id))
        if key is not None:
            self._emit(api_key_event(project_id, key))
