"""OIDC login, credentials file and token lifecycle (fake provider on 127.0.0.1)."""

from __future__ import annotations

import json
import socket
import stat
import sys
import threading
import time
import urllib.parse

import pytest

from codecarbon_agent.auth import (
    Authenticator,
    AuthError,
    AuthNetworkError,
    CredentialStore,
    LoginCancelled,
    NotLoggedIn,
    unverified_claims,
    user_from_claims,
)

from .conftest import auth_config, browser_get, callback_url


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def assert_port_closed(port: int) -> None:
    """Nothing listens on the callback port any more."""
    with socket.socket() as sock, pytest.raises(ConnectionRefusedError):
        sock.connect(("127.0.0.1", port))


def make_auth(tmp_path, oidc, **overrides) -> Authenticator:
    store = CredentialStore(tmp_path / "nested" / "dir" / "credentials.json")
    return Authenticator(store, auth_config(oidc, **overrides))


def run_wait(attempt):
    """Run ``attempt.wait()`` on a thread; return (thread, outcome dict)."""
    outcome: dict = {}

    def target():
        try:
            outcome["tokens"] = attempt.wait()
        except Exception as exc:  # noqa: BLE001
            outcome["error"] = exc

    thread = threading.Thread(target=target, daemon=True)
    thread.start()
    return thread, outcome


# --------------------------------------------------------------------------- helpers


def test_unverified_claims_and_user(fake_oidc):
    token = fake_oidc.jwt(name="Ada", email="ada@example.org")
    assert unverified_claims(token)["name"] == "Ada"
    assert unverified_claims("not.a.jwt") == {}
    assert unverified_claims(None) == {}
    assert user_from_claims({"name": "Ada", "email": "a@b.c"}) == {"name": "Ada", "email": "a@b.c"}
    assert user_from_claims({"preferred_username": "ada"}) == {"name": "ada", "email": None}
    assert user_from_claims({"email": "x@y.z", "fields": {"name": "Local"}})["name"] == "Local"
    assert user_from_claims({"sub": "only"}) is None
    assert user_from_claims(None) is None


# --------------------------------------------------------------------------- credentials


def test_credentials_store_roundtrip(tmp_path):
    store = CredentialStore(tmp_path / "a" / "b" / "credentials.json")
    assert store.load() is None
    store.save({"access_token": "x"})
    assert store.load() == {"access_token": "x"}
    assert list(store.path.parent.iterdir()) == [store.path]  # no temp file left
    store.path.write_text("{corrupt")
    assert store.load() is None
    store.path.write_text("[1]")
    assert store.load() is None
    store.delete()
    store.delete()  # idempotent
    assert not store.path.exists()


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX permissions")
def test_credentials_file_mode_is_0600(tmp_path):
    store = CredentialStore(tmp_path / "new" / "credentials.json")
    store.save({"access_token": "x"})
    assert stat.S_IMODE(store.path.stat().st_mode) == 0o600
    store.path.chmod(0o644)
    store.save({"access_token": "y"})
    assert stat.S_IMODE(store.path.stat().st_mode) == 0o600


def test_credentials_unwritable(tmp_path):
    blocker = tmp_path / "file"
    blocker.write_text("")
    store = CredentialStore(blocker / "credentials.json")  # parent is a file
    with pytest.raises(AuthError, match="cannot write the credentials file"):
        store.save({"access_token": "x"})


# --------------------------------------------------------------------------- login


def test_login_end_to_end(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    tokens = fake_oidc.tokens()
    fake_oidc.token_replies.append((200, tokens))
    attempt = auth.begin_login()

    parts = urllib.parse.urlsplit(attempt.url)
    query = dict(urllib.parse.parse_qsl(parts.query))
    assert (
        f"{parts.scheme}://{parts.netloc}{parts.path}"
        == fake_oidc.discovery["authorization_endpoint"]
    )
    assert query["client_id"] == "test-client"
    assert query["response_type"] == "code"
    assert query["scope"] == "openid offline_access"
    assert query["code_challenge_method"] == "S256"
    assert query["redirect_uri"] == attempt.redirect_uri
    assert attempt.redirect_uri.startswith("http://localhost:")
    assert attempt.redirect_uri.endswith("/callback")

    thread, outcome = run_wait(attempt)
    port = urllib.parse.urlsplit(attempt.redirect_uri).port
    assert browser_get(f"http://localhost:{port}/favicon.ico")[0] == 404  # ignored
    status, page = browser_get(callback_url(attempt.url, code="the-code"))
    assert status == 200 and "Login successful" in page
    thread.join(5)
    assert outcome.get("tokens", {}).get("access_token") == tokens["access_token"], outcome
    form = fake_oidc.token_requests[-1]
    assert form["grant_type"] == "authorization_code"
    assert form["code"] == "the-code"
    assert form["client_id"] == "test-client"
    assert form["redirect_uri"] == attempt.redirect_uri
    assert len(form["code_verifier"]) >= 43
    assert_port_closed(port)


def test_default_redirect_port_is_8090(tmp_path, fake_oidc):
    from codecarbon_agent.auth import AuthConfig

    assert AuthConfig().redirect_port == 8090
    assert AuthConfig().client_id == "codecarbon-cli"
    assert AuthConfig().well_known_url.startswith(
        "https://authentication.codecarbon.io/realms/codecarbon/"
    )


def test_env_overrides(monkeypatch):
    from codecarbon_agent.auth import AuthConfig

    monkeypatch.setenv("AUTH_SERVER_WELL_KNOWN", "https://staging/.well-known/x")
    monkeypatch.setenv("AUTH_CLIENT_ID", "staging-cli")
    config = AuthConfig.from_env()
    assert config.well_known_url == "https://staging/.well-known/x"
    assert config.client_id == "staging-cli"


def test_login_error_from_provider(tmp_path, fake_oidc):
    attempt = make_auth(tmp_path, fake_oidc).begin_login()
    thread, outcome = run_wait(attempt)
    status, page = browser_get(
        callback_url(attempt.url, error="access_denied", error_description="<b>no</b>")
    )
    assert status == 400 and "&lt;b&gt;" in page  # escaped
    thread.join(5)
    assert isinstance(outcome["error"], AuthError)
    assert str(outcome["error"]) == "Authorization failed: access_denied"
    assert fake_oidc.token_requests == []


def test_login_state_mismatch(tmp_path, fake_oidc):
    attempt = make_auth(tmp_path, fake_oidc).begin_login()
    thread, outcome = run_wait(attempt)
    assert browser_get(callback_url(attempt.url, code="c", state="forged"))[0] == 400
    thread.join(5)
    assert "state mismatch" in str(outcome["error"])


def test_login_token_exchange_refused(tmp_path, fake_oidc):
    attempt = make_auth(tmp_path, fake_oidc).begin_login()
    thread, outcome = run_wait(attempt)
    browser_get(callback_url(attempt.url, code="c"))  # token endpoint answers 400
    thread.join(5)
    assert str(outcome["error"]).startswith("Token exchange failed")


def test_login_cancel_releases_the_port(tmp_path, fake_oidc):
    attempt = make_auth(tmp_path, fake_oidc).begin_login()
    port = urllib.parse.urlsplit(attempt.redirect_uri).port
    thread, outcome = run_wait(attempt)
    time.sleep(0.1)
    t0 = time.monotonic()
    attempt.cancel()
    thread.join(2)
    assert not thread.is_alive()
    assert time.monotonic() - t0 < 1.0
    assert isinstance(outcome["error"], LoginCancelled)
    assert_port_closed(port)


def test_login_timeout(tmp_path, fake_oidc):
    attempt = make_auth(tmp_path, fake_oidc, login_timeout_secs=0.3).begin_login()
    thread, outcome = run_wait(attempt)
    thread.join(3)
    assert not thread.is_alive()
    assert type(outcome["error"]) is AuthError
    assert "no answer from the browser" in str(outcome["error"])


def test_login_port_busy(tmp_path, fake_oidc):
    port = free_port()
    with socket.socket() as busy:
        busy.bind(("localhost", port))
        busy.listen()
        auth = make_auth(tmp_path, fake_oidc, redirect_port=port)
        with pytest.raises(AuthError, match=f"^port {port} is already in use$"):
            auth.begin_login()


def test_login_provider_unreachable(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc, well_known_url=f"http://127.0.0.1:{free_port()}/x")
    with pytest.raises(AuthError, match="cannot reach the authentication server"):
        auth.begin_login()


# --------------------------------------------------------------------------- tokens


def test_validate_access_token(tmp_path, fake_oidc, rsa_key):
    from joserfc.jwk import RSAKey

    auth = make_auth(tmp_path, fake_oidc)
    assert auth.validate_access_token(fake_oidc.jwt())
    hits = len(fake_oidc.hits)
    assert auth.validate_access_token(fake_oidc.jwt())  # discovery + JWKS cached
    assert len(fake_oidc.hits) == hits
    # expired: rejected without any network access
    assert not auth.validate_access_token(fake_oidc.jwt(exp=int(time.time()) - 10))
    assert len(fake_oidc.hits) == hits
    assert not auth.validate_access_token("garbage")
    # another key (other provider): rejected, after one JWKS re-fetch
    other = RSAKey.generate_key(2048, parameters={"kid": "test-key"})
    assert not auth.validate_access_token(fake_oidc.jwt(key=other))
    assert fake_oidc.hits[hits:] == ["/jwks"]


def test_validate_access_token_offline_counts_as_valid(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc, well_known_url=f"http://127.0.0.1:{free_port()}/x")
    assert auth.validate_access_token(fake_oidc.jwt())


def test_session_valid_token(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    creds = {**fake_oidc.tokens(), "user": {"name": "Ada", "email": None}}
    auth.store.save(creds)
    assert auth.session() == creds
    assert fake_oidc.token_requests == []


def test_session_refreshes_expired_token(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    old = {
        **fake_oidc.tokens(exp=int(time.time()) - 5),
        "refresh_token": "old-refresh",
        "user": {"name": "Ada", "email": "ada@example.org"},
    }
    auth.store.save(old)
    new_tokens = fake_oidc.tokens()
    fake_oidc.token_replies.append((200, new_tokens))
    creds = auth.session()
    assert creds["access_token"] == new_tokens["access_token"]
    assert creds["user"] == old["user"]  # cached user kept
    assert fake_oidc.token_requests[-1] == {
        "grant_type": "refresh_token",
        "refresh_token": "old-refresh",
        "client_id": "test-client",
    }
    assert json.loads(auth.store.path.read_text())["access_token"] == new_tokens["access_token"]


def test_session_keeps_refresh_token_when_not_rotated(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    auth.store.save({"access_token": fake_oidc.jwt(exp=1), "refresh_token": "keep-me"})
    fake_oidc.token_replies.append((200, {"access_token": fake_oidc.jwt()}))
    assert auth.session()["refresh_token"] == "keep-me"


def test_session_refresh_refused_deletes_credentials(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    auth.store.save({"access_token": fake_oidc.jwt(exp=1), "refresh_token": "revoked"})
    fake_oidc.token_replies.append((400, {"error": "invalid_grant"}))
    with pytest.raises(NotLoggedIn):
        auth.session()
    assert not auth.store.path.exists()


def test_session_without_refresh_token_deletes_credentials(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    auth.store.save({"access_token": fake_oidc.jwt(exp=1)})
    with pytest.raises(NotLoggedIn):
        auth.session()
    assert not auth.store.path.exists()


def test_session_refresh_offline_keeps_credentials(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc, well_known_url=f"http://127.0.0.1:{free_port()}/x")
    auth.store.save({"access_token": fake_oidc.jwt(exp=1), "refresh_token": "r"})
    with pytest.raises(AuthNetworkError):
        auth.session()
    assert auth.store.path.exists()


def test_session_refresh_server_error_keeps_credentials(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    auth.store.save({"access_token": fake_oidc.jwt(exp=1), "refresh_token": "r"})
    fake_oidc.token_replies.append((503, {"error": "unavailable"}))
    with pytest.raises(AuthNetworkError):
        auth.session()
    assert auth.store.path.exists()


def test_session_not_logged_in(tmp_path, fake_oidc):
    auth = make_auth(tmp_path, fake_oidc)
    with pytest.raises(NotLoggedIn):
        auth.session()
