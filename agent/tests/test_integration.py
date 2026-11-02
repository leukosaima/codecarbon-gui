"""Runs the real CodeCarbon tracker. Opt-in: CODECARBON_AGENT_INTEGRATION=1."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

pytestmark = [
    pytest.mark.integration,
    pytest.mark.skipif(
        os.environ.get("CODECARBON_AGENT_INTEGRATION") != "1",
        reason="set CODECARBON_AGENT_INTEGRATION=1 to run the real tracker",
    ),
]

AGENT_DIR = Path(__file__).resolve().parents[1]


def _spawn() -> subprocess.Popen:
    env = {**os.environ, "PYTHONPATH": str(AGENT_DIR)}
    return subprocess.Popen(
        [sys.executable, "-m", "codecarbon_agent"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )


def _read_until(proc: subprocess.Popen, name: str, events: list, timeout: float = 60) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        line = proc.stdout.readline()
        if not line:
            break
        event = json.loads(line)  # every stdout line must be protocol JSON
        events.append(event)
        if event["event"] == name:
            return event
    raise AssertionError(f"no {name!r} event; got {[e['event'] for e in events]}")


def test_real_tracker_session(tmp_path):
    proc = _spawn()
    events: list = []
    try:
        _read_until(proc, "ready", events)
        config = {"interval_secs": 0.5, "country_iso_code": "FRA", "output_dir": str(tmp_path)}
        proc.stdin.write(json.dumps({"cmd": "start", "config": config}) + "\n")
        proc.stdin.flush()
        started = _read_until(proc, "started", events)
        assert started["hardware"]["cpu_method"]
        assert started["location"]["country_iso_code"] == "FRA"
        assert started["location"]["carbon_intensity_g_per_kwh"] > 0
        measurement = _read_until(proc, "measurement", events, timeout=10)
        assert measurement["power_w"]["total"] > 0
        assert measurement["energy_kwh"]["total"] > 0
        time.sleep(1.5)
        proc.stdin.write('{"cmd":"stop"}\n{"cmd":"quit"}\n')
        proc.stdin.flush()
        stopped = _read_until(proc, "stopped", events, timeout=30)
        assert stopped["summary"]["duration_s"] >= 2
        assert stopped["summary"]["energy_kwh"]["total"] > 0
        assert proc.wait(timeout=30) == 0
    finally:
        if proc.poll() is None:
            proc.kill()
    assert sum(e["event"] == "measurement" for e in events) >= 2
    rows = (tmp_path / "emissions.csv").read_text().strip().splitlines()
    assert len(rows) == 2  # header + exactly one row for the run


def test_selftest():
    env = {**os.environ, "PYTHONPATH": str(AGENT_DIR)}
    result = subprocess.run(
        [sys.executable, "-m", "codecarbon_agent", "--selftest", "--seconds", "2"],
        capture_output=True,
        text=True,
        timeout=120,
        env=env,
    )
    assert result.returncode == 0, result.stderr
    names = [json.loads(line)["event"] for line in result.stdout.splitlines()]
    assert names[0] == "ready"
    assert {"started", "measurement", "stopped"} <= set(names)
