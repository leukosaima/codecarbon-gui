"""OpenID Connect login to the CodeCarbon dashboard (protocol v2).

Same flow as ``codecarbon login`` (``codecarbon/cli/auth.py``): Authorization
Code + PKCE with authlib's ``OAuth2Session`` against the CodeCarbon Keycloak
realm, a one-shot callback server on ``localhost:8090``, access tokens checked
against the provider's JWKS with joserfc and refreshed with the refresh token.

Differences with the CLI module (which the agent must not import: it writes
``./credentials.json`` in the cwd and prints): the credentials path is
injected, nothing is printed, the callback server polls so that a login can be
cancelled or time out, and a network failure while refreshing keeps the
credentials (only a refusal from the token endpoint deletes them).
"""

from __future__ import annotations

import base64
import contextlib
import errno
import html
import json
import logging
import os
import sys
import tempfile
import threading
import time
from collections.abc import Mapping
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlsplit

import requests
from authlib.common.security import generate_token
from authlib.integrations.requests_client import OAuth2Session
from authlib.oauth2.rfc7636 import create_s256_code_challenge
from joserfc import jwt as jose_jwt
from joserfc.jwk import KeySet

logger = logging.getLogger(__name__)

DEFAULT_WELL_KNOWN = (
    "https://authentication.codecarbon.io/realms/codecarbon/.well-known/openid-configuration"
)
DEFAULT_CLIENT_ID = "codecarbon-cli"
DEFAULT_REDIRECT_PORT = 8090
SCOPE = "openid offline_access"
LOGIN_TIMEOUT_SECS = 300.0
HTTP_TIMEOUT_SECS = 15.0
# An access token that expires within this margin is refreshed right away.
EXPIRY_MARGIN_SECS = 30


class AuthError(Exception):
    """The login flow or a token operation failed; ``str(exc)`` is for the user."""


class LoginCancelled(AuthError):
    """The pending login was cancelled (``auth_cancel``, new ``auth_start``, quit)."""


class NotLoggedIn(AuthError):
    """No credentials, or they are no longer usable (refresh refused)."""


class AuthNetworkError(AuthError):
    """The auth server could not be reached; the credentials are kept."""


@dataclass(frozen=True)
class AuthConfig:
    well_known_url: str = DEFAULT_WELL_KNOWN
    client_id: str = DEFAULT_CLIENT_ID
    redirect_port: int = DEFAULT_REDIRECT_PORT
    login_timeout_secs: float = LOGIN_TIMEOUT_SECS
    http_timeout_secs: float = HTTP_TIMEOUT_SECS

    @classmethod
    def from_env(cls, **overrides: Any) -> AuthConfig:
        """Production defaults, overridden by ``AUTH_SERVER_WELL_KNOWN`` / ``AUTH_CLIENT_ID``."""
        values: dict[str, Any] = {
            "well_known_url": os.environ.get("AUTH_SERVER_WELL_KNOWN") or DEFAULT_WELL_KNOWN,
            "client_id": os.environ.get("AUTH_CLIENT_ID") or DEFAULT_CLIENT_ID,
        }
        values.update(overrides)
        return cls(**values)


# --------------------------------------------------------------------------- JWT helpers


def unverified_claims(token: Any) -> dict[str, Any]:
    """Claims of a compact JWT, *without* checking its signature ({} if unreadable)."""
    if not isinstance(token, str) or token.count(".") != 2:
        return {}
    try:
        payload = token.split(".")[1]
        payload += "=" * (-len(payload) % 4)
        claims = json.loads(base64.urlsafe_b64decode(payload.encode("ascii")))
    except (ValueError, UnicodeError):
        return {}
    return claims if isinstance(claims, dict) else {}


def user_from_claims(claims: Mapping[str, Any] | None) -> dict[str, Any] | None:
    """``{"name", "email"}`` from OIDC claims, or ``None`` if there is neither."""
    if not isinstance(claims, Mapping):
        return None
    fields = claims.get("fields") if isinstance(claims.get("fields"), Mapping) else {}
    name = claims.get("name") or claims.get("preferred_username") or fields.get("name")
    email = claims.get("email")
    if not name and not email:
        return None
    return {"name": str(name) if name else None, "email": str(email) if email else None}


def _signature_ok(token: str, keyset: KeySet) -> bool:
    try:
        decoded = jose_jwt.decode(token, keyset)
        jose_jwt.JWTClaimsRegistry().validate(decoded.claims)
    except Exception:  # noqa: BLE001 - any joserfc error: bad signature, unknown key, expired
        return False
    return True


# --------------------------------------------------------------------------- credentials


class CredentialStore:
    """The credentials JSON file (tokens + cached ``user``), always mode 0600."""

    def __init__(self, path: str | os.PathLike[str]) -> None:
        self.path = Path(path).expanduser()
        self._lock = threading.Lock()

    def load(self) -> dict[str, Any] | None:
        with self._lock:
            try:
                data = json.loads(self.path.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                return None
        return data if isinstance(data, dict) else None

    def save(self, data: Mapping[str, Any]) -> None:
        """Write atomically: a 0600 temp file in the same directory, then rename."""
        with self._lock:
            try:
                self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                fd, tmp = tempfile.mkstemp(
                    prefix=".credentials-", suffix=".tmp", dir=self.path.parent
                )
                try:
                    with os.fdopen(fd, "w", encoding="utf-8") as stream:
                        json.dump(dict(data), stream)
                    with contextlib.suppress(OSError):
                        os.chmod(tmp, 0o600)
                    os.replace(tmp, self.path)
                except BaseException:
                    with contextlib.suppress(OSError):
                        os.unlink(tmp)
                    raise
                with contextlib.suppress(OSError):
                    os.chmod(self.path, 0o600)
            except OSError as exc:
                raise AuthError(f"cannot write the credentials file {self.path}: {exc}") from exc

    def delete(self) -> None:
        with self._lock, contextlib.suppress(FileNotFoundError):
            self.path.unlink()


# --------------------------------------------------------------------------- callback server


_PAGE = (
    "<!doctype html><html><head><meta charset='utf-8'><title>CodeCarbon</title></head>"
    "<body style='font-family:sans-serif;text-align:center;margin-top:15vh'>"
    "<h1>{title}</h1><p>{text}</p></body></html>"
)


class _CallbackServer(HTTPServer):
    # On Windows SO_REUSEADDR lets a second socket steal a busy port: keep the
    # "port already in use" error there.
    allow_reuse_address = sys.platform != "win32"

    def __init__(self, port: int, expected_state: str) -> None:
        super().__init__(("localhost", port), _CallbackHandler)
        self.timeout = 0.2  # handle_request() returns at least this often
        self.expected_state = expected_state
        self.query: str | None = None
        self.error: str | None = None
        self.done = False


class _CallbackHandler(BaseHTTPRequestHandler):
    timeout = 10  # a connection that never sends a request does not block the poll loop
    server: _CallbackServer

    def do_GET(self) -> None:  # noqa: N802 - http.server API
        parts = urlsplit(self.path)
        if parts.path != "/callback" or self.server.done:
            self._reply(404, "Not found", "")
            return
        params = {k: v[0] for k, v in parse_qs(parts.query).items()}
        if "error" in params:
            self.server.error = f"Authorization failed: {params['error']}"
            detail = params.get("error_description", params["error"])
            self._reply(400, "Login failed", detail)
        elif params.get("state") != self.server.expected_state:
            self.server.error = "Authorization failed: state mismatch"
            self._reply(400, "Login failed", "Invalid state, please try again from the app.")
        elif not params.get("code"):
            self.server.error = "Authorization failed: no code in the callback"
            self._reply(400, "Login failed", "No authorization code was received.")
        else:
            self.server.query = parts.query
            self._reply(200, "Login successful!", "You can close this window now.")
        self.server.done = True

    def _reply(self, status: int, title: str, text: str) -> None:
        body = _PAGE.format(title=html.escape(title), text=html.escape(text)).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        pass


@dataclass
class LoginAttempt:
    """A pending browser login: the URL to open, then :meth:`wait` for the tokens."""

    url: str
    redirect_uri: str
    _server: _CallbackServer
    _session: OAuth2Session
    _code_verifier: str
    _token_endpoint: str
    _timeout: float
    _http_timeout: float
    _cancel: threading.Event = field(default_factory=threading.Event)
    _closed: bool = False

    @property
    def cancelled(self) -> bool:
        return self._cancel.is_set()

    def cancel(self) -> None:
        self._cancel.set()

    def close(self) -> None:
        """Release the port (idempotent; :meth:`wait` does it too)."""
        if not self._closed:
            self._closed = True
            with contextlib.suppress(OSError):
                self._server.server_close()

    def wait(self) -> dict[str, Any]:
        """Wait for the browser callback, then exchange the code for tokens.

        Raises :class:`LoginCancelled` or :class:`AuthError` (timeout, error
        from the provider, failed exchange).
        """
        deadline = time.monotonic() + self._timeout
        try:
            while not self._server.done:
                if self._cancel.is_set():
                    raise LoginCancelled("login cancelled")
                if time.monotonic() >= deadline:
                    minutes = self._timeout / 60
                    raise AuthError(
                        f"no answer from the browser after {minutes:g} min, login abandoned"
                    )
                self._server.handle_request()
        finally:
            self.close()
        if self._cancel.is_set():
            raise LoginCancelled("login cancelled")
        if self._server.error:
            raise AuthError(self._server.error)
        callback_url = f"{self.redirect_uri}?{self._server.query}"
        try:
            token = self._session.fetch_token(
                self._token_endpoint,
                authorization_response=callback_url,
                code_verifier=self._code_verifier,
                timeout=self._http_timeout,
            )
        except requests.RequestException as exc:
            raise AuthError(f"Token exchange failed: {exc}") from exc
        except Exception as exc:  # noqa: BLE001 - authlib OAuthError and friends
            raise AuthError(f"Token exchange failed: {exc}") from exc
        finally:
            with contextlib.suppress(Exception):
                self._session.close()
        tokens = dict(token)
        if not tokens.get("access_token"):
            raise AuthError("Token exchange failed: no access token in the answer")
        return tokens


# --------------------------------------------------------------------------- authenticator


class Authenticator:
    """Token lifecycle for one credentials file."""

    def __init__(self, store: CredentialStore, config: AuthConfig | None = None) -> None:
        self.store = store
        self.config = config or AuthConfig.from_env()
        self._lock = threading.Lock()
        self._discovery: dict[str, Any] | None = None
        self._jwks: KeySet | None = None

    # ------------------------------------------------------------- provider metadata

    def _get_json(self, url: str) -> Any:
        response = requests.get(url, timeout=self.config.http_timeout_secs)
        response.raise_for_status()
        return response.json()

    def discover(self) -> dict[str, Any]:
        """The OIDC discovery document (cached once fetched)."""
        with self._lock:
            if self._discovery is not None:
                return self._discovery
        document = self._get_json(self.config.well_known_url)
        if not isinstance(document, dict):
            raise ValueError("invalid OpenID configuration")
        with self._lock:
            self._discovery = document
        return document

    def _keyset(self, refresh: bool = False) -> KeySet:
        with self._lock:
            if self._jwks is not None and not refresh:
                return self._jwks
        keyset = KeySet.import_key_set(self._get_json(self.discover()["jwks_uri"]))
        with self._lock:
            self._jwks = keyset
        return keyset

    # ------------------------------------------------------------- tokens

    def validate_access_token(self, access_token: str) -> bool:
        """Like the CLI: JWKS signature + expiry check; ``True`` when offline.

        An access token that is (about to be) expired is reported invalid
        without any network access.
        """
        claims = unverified_claims(access_token)
        if not claims:
            return False
        exp = claims.get("exp")
        if isinstance(exp, (int, float)) and exp - EXPIRY_MARGIN_SECS <= time.time():
            return False
        try:
            cached = self._jwks is not None
            if _signature_ok(access_token, self._keyset()):
                return True
            # Unknown key with a cached JWKS: the provider may have rotated its keys.
            return cached and _signature_ok(access_token, self._keyset(refresh=True))
        except requests.RequestException as exc:
            logger.info("auth server unreachable, the API will validate the token: %s", exc)
            return True
        except (KeyError, TypeError, ValueError) as exc:  # unusable discovery / JWKS
            logger.info("cannot validate the token locally: %s", exc)
            return True

    def refresh_tokens(self, refresh_token: str) -> dict[str, Any]:
        """New token set from the refresh token.

        Raises :class:`AuthNetworkError` when the server cannot be reached and
        :class:`NotLoggedIn` when it refuses the refresh token.
        """
        try:
            endpoint = self.discover()["token_endpoint"]
            response = requests.post(
                endpoint,
                data={
                    "grant_type": "refresh_token",
                    "refresh_token": refresh_token,
                    "client_id": self.config.client_id,
                },
                timeout=self.config.http_timeout_secs,
            )
        except (requests.RequestException, KeyError, TypeError, ValueError) as exc:
            raise AuthNetworkError(f"cannot reach the authentication server: {exc}") from exc
        if response.status_code >= 500:
            raise AuthNetworkError(f"auth server error: HTTP {response.status_code}")
        try:
            response.raise_for_status()
            tokens = response.json()
        except (requests.HTTPError, ValueError) as exc:
            raise NotLoggedIn("your session has expired, please sign in again") from exc
        if not isinstance(tokens, dict) or not tokens.get("access_token"):
            raise NotLoggedIn("your session has expired, please sign in again")
        return tokens

    def session(self) -> dict[str, Any]:
        """The stored credentials with a valid (possibly refreshed) access token.

        Raises :class:`NotLoggedIn` (credentials deleted when the refresh is
        refused) or :class:`AuthNetworkError` (credentials kept).
        """
        creds = self.store.load()
        if not creds or not isinstance(creds.get("access_token"), str):
            raise NotLoggedIn("not signed in")
        if self.validate_access_token(creds["access_token"]):
            return creds
        refresh_token = creds.get("refresh_token")
        if isinstance(refresh_token, str) and refresh_token:
            try:
                tokens = self.refresh_tokens(refresh_token)
            except NotLoggedIn:
                pass
            else:
                updated = {**creds, **tokens}
                if not tokens.get("refresh_token"):
                    updated["refresh_token"] = refresh_token
                self.store.save(updated)
                return updated
        # Refresh refused: the credentials are stale (expired, provider changed).
        self.store.delete()
        raise NotLoggedIn("your session has expired, please sign in again")

    # ------------------------------------------------------------- login

    def begin_login(self) -> LoginAttempt:
        """Bind the callback server and build the authorization URL.

        Raises :class:`AuthError` when the provider cannot be reached or the
        port is busy.
        """
        try:
            discovery = self.discover()
            authorization_endpoint = discovery["authorization_endpoint"]
            token_endpoint = discovery["token_endpoint"]
        except requests.RequestException as exc:
            raise AuthError(f"cannot reach the authentication server: {exc}") from exc
        except (KeyError, TypeError, ValueError) as exc:
            raise AuthError(f"invalid answer from the authentication server: {exc}") from exc

        state = generate_token(30)
        port = self.config.redirect_port
        try:
            server = _CallbackServer(port, state)
        except OSError as exc:
            if exc.errno in (errno.EADDRINUSE, getattr(errno, "WSAEADDRINUSE", -1), 10013):
                raise AuthError(f"port {port} is already in use") from exc
            raise AuthError(f"cannot listen on localhost:{port}: {exc}") from exc
        redirect_uri = f"http://localhost:{server.server_address[1]}/callback"

        session = OAuth2Session(
            client_id=self.config.client_id,
            redirect_uri=redirect_uri,
            scope=SCOPE,
            token_endpoint_auth_method="none",
        )
        code_verifier = generate_token(48)
        url, _ = session.create_authorization_url(
            authorization_endpoint,
            state=state,
            code_challenge=create_s256_code_challenge(code_verifier),
            code_challenge_method="S256",
        )
        return LoginAttempt(
            url=url,
            redirect_uri=redirect_uri,
            _server=server,
            _session=session,
            _code_verifier=code_verifier,
            _token_endpoint=token_endpoint,
            _timeout=self.config.login_timeout_secs,
            _http_timeout=self.config.http_timeout_secs,
        )
