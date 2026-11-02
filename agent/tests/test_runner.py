from __future__ import annotations

import logging
import socket
import threading
import time
import urllib.parse

import pytest

from codecarbon_agent.dashboard import AccountService
from codecarbon_agent.runner import ACCOUNT_COMMANDS, format_record

from .conftest import auth_config, browser_get, callback_url

FAST = {"interval_secs": 0.5, "save_to_file": False}


def _start(harness, config=None):
    return harness.request({"cmd": "start", "config": config or FAST}, "started")


def test_ready_is_the_first_event(harness_factory):
    harness, _ = harness_factory()
    assert harness.events()[0]["event"] == "ready"
    assert harness.events()[0]["protocol_version"] == 2


def test_status_idle(harness_factory):
    harness, _ = harness_factory()
    status = harness.request({"cmd": "status"}, "status")
    assert status["state"] == "idle"
    assert status["last_measurement"] is None


def test_full_session(harness_factory):
    harness, trackers = harness_factory()
    started = _start(harness)
    assert started["hardware"]["cpu_model"] == "Fake CPU 9000"
    assert started["config"]["interval_secs"] == 0.5
    first = harness.wait_event("measurement", timeout=2)
    assert first["power_w"]["total"] == 12.5
    harness.wait_for(lambda e: harness.names().count("measurement") >= 2, timeout=2)
    status = harness.request({"cmd": "status"}, "status")
    assert status["state"] == "running"
    assert status["last_measurement"]["event"] == "measurement"
    stopped = harness.request({"cmd": "stop"}, "stopped")
    assert stopped["summary"]["run_id"] == "run-123"
    assert trackers[0].stopped
    time.sleep(0.7)  # no measurement may follow `stopped`
    names = harness.names()
    assert "measurement" not in names[names.index("stopped") :]
    assert harness.request({"cmd": "status"}, "status")["state"] == "idle"


def test_measurement_cadence(harness_factory):
    harness, _ = harness_factory()
    _start(harness)
    time.sleep(1.65)
    harness.request({"cmd": "stop"}, "stopped")
    count = harness.names().count("measurement")
    assert 2 <= count <= 4, count


def test_slow_flush_does_not_block_commands(harness_factory):
    harness, _ = harness_factory(flush_delay=0.4)
    _start(harness)
    harness.wait_event("measurement", timeout=3)
    for _ in range(3):
        t0 = time.monotonic()
        harness.request({"cmd": "status"}, "status")
        assert time.monotonic() - t0 < 0.3
        time.sleep(0.1)
    harness.request({"cmd": "stop"}, "stopped", timeout=5)


def test_start_while_running(harness_factory):
    harness, trackers = harness_factory()
    _start(harness)
    error = harness.request({"cmd": "start", "config": FAST}, "error")
    assert error["code"] == "already_running"
    assert len(trackers) == 1
    harness.request({"cmd": "stop"}, "stopped")


def test_stop_while_idle(harness_factory):
    harness, _ = harness_factory()
    assert harness.request({"cmd": "stop"}, "error")["code"] == "not_running"


def test_bad_commands(harness_factory):
    harness, _ = harness_factory()
    for line in (
        "not json",
        "[1, 2]",
        '"start"',
        "{}",
        '{"cmd": 3}',
        '{"cmd": "dance"}',
        '{"cmd": "start", "config": [1]}',
        '{"cmd": "start", "config": {"pue": "x"}}',
    ):
        error = harness.request(line, "error")
        assert error["code"] == "bad_command", (line, error)
    # blank lines are ignored, unknown fields are ignored
    harness.send("")
    assert harness.request({"cmd": "status", "extra": 1}, "status")["state"] == "idle"


def test_tracker_init_failure_then_retry(harness_factory):
    harness, trackers = harness_factory()
    error = harness.request(
        {"cmd": "start", "config": {**FAST, "country_iso_code": "XYZ"}}, "error"
    )
    assert error["code"] == "tracker_init_failed"
    assert harness.request({"cmd": "status"}, "status")["state"] == "idle"
    _start(harness)
    harness.request({"cmd": "stop"}, "stopped")


def test_eof_stops_and_exits_zero(harness_factory):
    harness, trackers = harness_factory()
    _start(harness)
    harness.stdin.close()
    assert harness.join() == 0
    assert harness.names()[-1] == "stopped"
    assert trackers[0].stopped


def test_quit_stops_and_exits_zero(harness_factory):
    harness, trackers = harness_factory()
    _start(harness)
    harness.send({"cmd": "quit"})
    assert harness.join() == 0
    assert harness.names()[-1] == "stopped"


def test_quit_while_idle(harness_factory):
    harness, _ = harness_factory()
    harness.send({"cmd": "quit"})
    assert harness.join() == 0
    assert harness.names() == ["ready"]


def test_termination_request_stops(harness_factory):
    harness, trackers = harness_factory()
    _start(harness)
    harness.runner.request_termination()
    assert harness.join() == 0
    assert trackers[0].stopped
    assert harness.names()[-1] == "stopped"


def test_internal_errors_keep_the_loop_alive(harness_factory):
    harness, _ = harness_factory()
    calls = {"n": 0}
    original = harness.runner._cmd_status

    def flaky(message):
        calls["n"] += 1
        if calls["n"] == 1:
            raise RuntimeError("kaboom")
        return original(message)

    harness.runner._cmd_status = flaky
    error = harness.request({"cmd": "status"}, "error")
    assert error["code"] == "internal"
    assert "kaboom" in error["message"]
    assert harness.request({"cmd": "status"}, "status")["state"] == "idle"


def test_sampling_failures_reported_once_then_recover(harness_factory):
    harness, _ = harness_factory(fail_flushes=4)  # initial flush + 3 samples fail
    _start(harness)
    harness.wait_event("measurement", timeout=4)
    errors = [e for e in harness.events() if e["event"] == "error"]
    assert len(errors) == 1
    assert errors[0]["code"] == "internal"
    assert "measurement failed" in errors[0]["message"]
    harness.request({"cmd": "stop"}, "stopped")


def test_non_finite_numbers_become_null(harness_factory):
    harness, _ = harness_factory(data_overrides={"emissions": float("nan")})
    _start(harness)
    measurement = harness.wait_event("measurement", timeout=2)
    assert measurement["emissions_kg"] is None
    harness.request({"cmd": "stop"}, "stopped")


def test_codecarbon_warnings_are_forwarded(harness_factory):
    harness, _ = harness_factory()
    cc_logger = logging.getLogger("codecarbon")
    cc_logger.warning("No CPU tracking mode found. Falling back on CPU load mode.")
    cc_logger.warning("Multiple instances of codecarbon are allowed to run at the same time.")
    cc_logger.error("Something broke")
    harness.wait_for(lambda e: e.get("message") == "Something broke")
    logs = [e for e in harness.events() if e["event"] == "log"]
    assert [(e["level"], e["message"]) for e in logs] == [
        ("warning", "No CPU tracking mode found. Falling back on CPU load mode."),
        ("error", "Something broke"),
    ]


def test_log_handler_removed_after_exit(harness_factory):
    harness, _ = harness_factory()
    harness.send({"cmd": "quit"})
    harness.join()
    handlers = logging.getLogger("codecarbon").handlers
    assert not any(type(h).__name__ == "ProtocolLogHandler" for h in handlers)


def test_format_record_with_exception():
    try:
        raise ValueError("bad value")
    except ValueError:
        import sys

        exc_info = sys.exc_info()
    record = logging.LogRecord(
        "codecarbon", logging.WARNING, __file__, 1, "%s", (Exception,), exc_info
    )
    assert format_record(record) == "ValueError: bad value"
    record = logging.LogRecord(
        "codecarbon", logging.WARNING, __file__, 1, "  line one \n\n  line two ", (), None
    )
    assert format_record(record) == "line one\nline two"


def test_eof_watchdog_forces_exit_when_the_loop_is_stuck(monkeypatch):
    """If the app disappears while the main loop is stuck inside CodeCarbon,
    the reader thread's EOF watchdog must force the exit (no orphan)."""
    import io
    import threading

    from codecarbon_agent.protocol import Writer
    from codecarbon_agent.runner import Runner

    forced = threading.Event()
    runner = Runner(io.StringIO(""), Writer(io.StringIO()), eof_grace_secs=0.05)
    monkeypatch.setattr(runner, "_force_exit", forced.set)
    runner._read_stdin()  # hits EOF at once; the main loop is "stuck" (never drains)
    assert forced.wait(2.0), "the EOF watchdog did not fire"


# --------------------------------------------------------------------------- protocol v2


def _account_options(tmp_path, oidc, api, **config):
    def factory(emit):
        return AccountService(
            tmp_path / "credentials.json",
            emit,
            auth_config=auth_config(oidc, **config),
            api_endpoint="https://api.example.test",
            client_factory=api.factory,
        )

    return {"account_factory": factory}


def _redirect_port(auth_url):
    query = dict(urllib.parse.parse_qsl(urllib.parse.urlsplit(auth_url).query))
    return urllib.parse.urlsplit(query["redirect_uri"]).port


def _assert_port_closed(port):
    with socket.socket() as sock, pytest.raises(ConnectionRefusedError):
        sock.connect(("127.0.0.1", port))


def test_account_commands_unavailable_without_credentials(harness_factory):
    harness, _ = harness_factory()
    for cmd in sorted(ACCOUNT_COMMANDS):
        error = harness.request({"cmd": cmd}, "error")
        assert error["code"] == "auth_unavailable", cmd
        assert "--credentials" in error["message"]


def test_credentials_path_enables_auth_status(harness_factory, tmp_path, monkeypatch):
    monkeypatch.delenv("CODECARBON_API_ENDPOINT", raising=False)
    path = tmp_path / "x" / "credentials.json"
    harness, _ = harness_factory(runner_options={"credentials_path": str(path)})
    status = harness.request({"cmd": "auth_status"}, "auth_status")
    assert status["logged_in"] is False
    assert status["user"] is None
    assert status["api_endpoint"] == "https://api.codecarbon.io"
    assert not path.exists()


def test_login_through_the_runner(harness_factory, tmp_path, fake_oidc, fake_api):
    harness, _ = harness_factory(runner_options=_account_options(tmp_path, fake_oidc, fake_api))
    fake_oidc.token_replies.append((200, fake_oidc.tokens()))
    url = harness.request({"cmd": "auth_start"}, "auth_url")["url"]
    assert browser_get(callback_url(url, code="abc"))[0] == 200
    result = harness.wait_event("auth_result")
    assert result["ok"] is True
    assert result["user"] == {"name": "Ada", "email": "ada@example.org"}
    status = harness.request({"cmd": "auth_status"}, "auth_status")
    assert status["logged_in"] is True
    assert status["api_endpoint"] == "https://api.example.test"
    catalog = harness.request({"cmd": "api_catalog"}, "api_catalog")
    assert catalog["organizations"][0]["projects"][0]["experiments"][0]["id"] == "e1"
    assert harness.request({"cmd": "auth_logout"}, "auth_status")["logged_in"] is False
    error = harness.request({"cmd": "api_catalog"}, "error")
    assert error["code"] == "not_logged_in"


def test_measurement_continues_while_login_pending(harness_factory, tmp_path, fake_oidc, fake_api):
    harness, trackers = harness_factory(
        runner_options=_account_options(tmp_path, fake_oidc, fake_api)
    )
    _start(harness)
    seen = len(harness.events())
    url = harness.request({"cmd": "auth_start"}, "auth_url")["url"]
    # the sampler keeps going and the stdin loop keeps answering
    harness.wait_for(
        lambda e: [x["event"] for x in harness.events()[seen:]].count("measurement") >= 2,
        timeout=3,
    )
    t0 = time.monotonic()
    assert harness.request({"cmd": "status"}, "status")["state"] == "running"
    assert time.monotonic() - t0 < 0.5
    # the account worker is free too: other commands are answered
    assert harness.request({"cmd": "api_catalog"}, "error")["code"] == "not_logged_in"
    harness.request({"cmd": "stop"}, "stopped")
    result = harness.request({"cmd": "auth_cancel"}, "auth_result")
    assert result["cancelled"] is True
    _assert_port_closed(_redirect_port(url))
    assert trackers[0].stopped


def test_quit_while_login_pending(harness_factory, tmp_path, fake_oidc, fake_api):
    harness, _ = harness_factory(runner_options=_account_options(tmp_path, fake_oidc, fake_api))
    url = harness.request({"cmd": "auth_start"}, "auth_url")["url"]
    t0 = time.monotonic()
    harness.send({"cmd": "quit"})
    assert harness.join(5) == 0
    assert time.monotonic() - t0 < 2.0
    last = harness.events()[-1]
    assert last["event"] == "auth_result" and last["cancelled"] is True
    _assert_port_closed(_redirect_port(url))


def test_eof_while_login_pending_with_a_run(harness_factory, tmp_path, fake_oidc, fake_api):
    harness, trackers = harness_factory(
        runner_options=_account_options(tmp_path, fake_oidc, fake_api)
    )
    _start(harness)
    url = harness.request({"cmd": "auth_start"}, "auth_url")["url"]
    harness.stdin.close()
    assert harness.join(5) == 0
    names = harness.names()
    assert names[-1] == "stopped"
    assert harness.events()[names.index("auth_url") :][-2]["cancelled"] is True
    assert trackers[0].stopped
    _assert_port_closed(_redirect_port(url))


def test_account_commands_run_in_order_off_the_main_loop(harness_factory):
    release = threading.Event()
    handled = []

    class SlowAccount:
        def __init__(self, emit):
            self.emit = emit

        def handle(self, message):
            if message["cmd"] == "api_catalog" and not handled:
                handled.append("boom")
                raise RuntimeError("kaboom")
            release.wait(5)
            handled.append(message["cmd"])
            self.emit({"event": "done", "ts": "", "cmd": message["cmd"]})

        def shutdown(self):
            release.set()

    harness, _ = harness_factory(runner_options={"account_factory": SlowAccount})
    error = harness.request({"cmd": "api_catalog"}, "error")
    assert error["code"] == "internal" and "kaboom" in error["message"]
    harness.send({"cmd": "auth_status"})
    harness.send({"cmd": "api_key", "project_id": "p"})
    t0 = time.monotonic()
    assert harness.request({"cmd": "status"}, "status")["state"] == "idle"
    assert time.monotonic() - t0 < 0.5  # the worker is blocked, the loop is not
    release.set()
    harness.wait_for(lambda e: handled[-1:] == ["api_key"])
    assert handled == ["boom", "auth_status", "api_key"]
