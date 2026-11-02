from __future__ import annotations

import io
import json
import re
import threading
import uuid

from codecarbon_agent import PROTOCOL_VERSION, __version__
from codecarbon_agent.protocol import (
    Writer,
    encode,
    error_event,
    log_event,
    make_event,
    now_iso,
    ready_event,
    sanitize,
    status_event,
    stopped_event,
)

ISO_WITH_OFFSET = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d[+-]\d\d:\d\d$")


def test_now_iso_has_local_offset():
    assert ISO_WITH_OFFSET.match(now_iso())


def test_make_event_has_event_and_ts():
    event = make_event("status", state="idle")
    assert event["event"] == "status"
    assert ISO_WITH_OFFSET.match(event["ts"])
    assert event["state"] == "idle"


def test_sanitize_replaces_non_finite_numbers():
    value = {"a": float("nan"), "b": [float("inf"), -float("inf"), 1.5], "c": (1, 2)}
    assert sanitize(value) == {"a": None, "b": [None, None, 1.5], "c": [1, 2]}


def test_sanitize_stringifies_unknown_types():
    run_id = uuid.uuid4()
    assert sanitize({"run_id": run_id, 1: True}) == {"run_id": str(run_id), "1": True}


def test_encode_is_one_compact_valid_json_line():
    line = encode({"event": "log", "message": "multi\nline é", "x": float("nan")})
    assert "\n" not in line
    assert json.loads(line) == {"event": "log", "message": "multi\nline é", "x": None}
    assert line.isascii()


def test_ready_event_fields():
    event = ready_event()
    assert event["event"] == "ready"
    assert event["protocol_version"] == PROTOCOL_VERSION == 1
    assert event["agent_version"] == __version__
    for key in ("codecarbon_version", "python_version", "pid", "platform"):
        assert key in event


def test_small_event_builders():
    assert error_event("bad_command", "x")["code"] == "bad_command"
    assert log_event("warning", "w")["level"] == "warning"
    assert status_event("idle", None)["last_measurement"] is None
    assert stopped_event({"run_id": "r"})["summary"] == {"run_id": "r"}


def test_writer_writes_lines_atomically_from_threads():
    stream = io.StringIO()
    writer = Writer(stream)

    def spam(n: int) -> None:
        for i in range(200):
            writer.write({"event": "log", "message": f"{n}-{i}" * 20})

    threads = [threading.Thread(target=spam, args=(n,)) for n in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    lines = stream.getvalue().splitlines()
    assert len(lines) == 1600
    assert all(json.loads(line)["event"] == "log" for line in lines)


class _BrokenStream(io.StringIO):
    def write(self, s: str) -> int:
        raise BrokenPipeError()


def test_writer_marks_itself_broken_instead_of_raising():
    writer = Writer(_BrokenStream())
    assert writer.write({"event": "x"}) is False
    assert writer.broken
    assert writer.write({"event": "y"}) is False
